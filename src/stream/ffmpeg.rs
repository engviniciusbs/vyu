//! FFmpeg-backed [`Source`]: RTSP over TCP → decoded RGBA video + 48 kHz mono audio.

use std::collections::VecDeque;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use ffmpeg_next as ff;
use ff::ffi;
use ff::format::{Pixel, Sample, sample};
use ff::software::{resampling, scaling};
use ff::util::channel_layout::ChannelLayout;
use ff::{Dictionary, Packet, codec, decoder, format, frame, media};

use super::StreamVariant;
use super::frame::Frame;
use super::session::{Source, SourceError, SourceEvent, SourceFactory};
use super::url::{mask, rtsp_url};

/// No decoded video for this long → the stream is considered stalled.
const STALL: Duration = Duration::from_secs(5);
/// Socket I/O timeout handed to FFmpeg, in microseconds.
const IO_TIMEOUT_US: &str = "5000000";
const AUDIO_RATE: u32 = 48_000;

/// Set after a hardware frame transfer fails, so later opens use software decoding.
static HW_BROKEN: AtomicBool = AtomicBool::new(false);

/// Call once at startup.
pub fn init() {
    let _ = ff::init();
    ff::util::log::set_level(ff::util::log::Level::Error);
}

#[derive(Clone)]
pub struct FfmpegFactory {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub pass: String,
}

impl SourceFactory for FfmpegFactory {
    fn open(&self, v: StreamVariant) -> Result<Box<dyn Source>, SourceError> {
        let url = rtsp_url(&self.host, self.port, &self.user, &self.pass, v);
        tracing::info!("abrindo {}", mask(&url));
        FfmpegSource::open(&url, &self.host).map(|s| Box::new(s) as Box<dyn Source>)
    }
}

/// Opens the stream and waits for the first video frame (used by "Testar conexão").
pub fn probe(factory: &FfmpegFactory, v: StreamVariant, timeout: Duration) -> Result<Frame, SourceError> {
    let start = Instant::now();
    let mut src = factory.open(v)?;
    while start.elapsed() < timeout {
        if let SourceEvent::Video(f) = src.next_event()? {
            return Ok(f);
        }
    }
    Err(SourceError::Other("Tempo esgotado esperando o vídeo".into()))
}

fn map_err(e: ff::Error, host: &str) -> SourceError {
    if matches!(e, ff::Error::HttpUnauthorized | ff::Error::HttpForbidden) {
        return SourceError::Auth;
    }
    let msg = e.to_string();
    let lower = msg.to_lowercase();
    if lower.contains("401") || lower.contains("unauthorized") {
        SourceError::Auth
    } else if ["refused", "timed out", "timeout", "unreachable", "no route", "input/output", "resolve", "host"]
        .iter()
        .any(|k| lower.contains(k))
    {
        SourceError::Unreachable(host.to_string())
    } else {
        SourceError::Other(msg)
    }
}

struct AudioPath {
    stream: usize,
    decoder: decoder::Audio,
    resampler: Option<resampling::Context>,
}

pub struct FfmpegSource {
    input: format::context::Input,
    host: String,
    video_stream: usize,
    video: decoder::Video,
    scaler: Option<(scaling::Context, (Pixel, u32, u32))>,
    audio: Option<AudioPath>,
    pending: VecDeque<SourceEvent>,
    last_video: Instant,
}

// The FFmpeg contexts are only touched from the session thread that owns the source.
unsafe impl Send for FfmpegSource {}

impl FfmpegSource {
    fn open(url: &str, host: &str) -> Result<Self, SourceError> {
        let mut opts = Dictionary::new();
        opts.set("rtsp_transport", "tcp");
        opts.set("timeout", IO_TIMEOUT_US);
        opts.set("flags", "low_delay");
        opts.set("analyzeduration", "1000000");
        opts.set("probesize", "500000");
        let input = format::input_with_dictionary(url, opts).map_err(|e| map_err(e, host))?;

        let vstream = input
            .streams()
            .best(media::Type::Video)
            .ok_or_else(|| SourceError::Other("A câmera não enviou vídeo".into()))?;
        let video_stream = vstream.index();
        let mut vctx = codec::Context::from_parameters(vstream.parameters()).map_err(|e| map_err(e, host))?;
        unsafe {
            let raw = vctx.as_mut_ptr();
            (*raw).thread_count = 2;
            (*raw).flags |= ffi::AV_CODEC_FLAG_LOW_DELAY as i32;
        }
        let hw = std::env::var_os("VYU_NO_HW").is_none() && !HW_BROKEN.load(Ordering::Relaxed) && unsafe { attach_d3d11(&mut vctx) };
        let video = vctx.decoder().video().map_err(|e| map_err(e, host))?;
        tracing::info!("vídeo {}x{} ({:?}), aceleração por hardware: {hw}", video.width(), video.height(), video.id());

        let audio = input.streams().best(media::Type::Audio).and_then(|s| {
            let ctx = codec::Context::from_parameters(s.parameters()).ok()?;
            let decoder = ctx.decoder().audio().ok()?;
            Some(AudioPath { stream: s.index(), decoder, resampler: None })
        });

        Ok(Self {
            input,
            host: host.to_string(),
            video_stream,
            video,
            scaler: None,
            audio,
            pending: VecDeque::new(),
            last_video: Instant::now(),
        })
    }

    fn drain_video(&mut self) -> Result<(), SourceError> {
        let mut decoded = frame::Video::empty();
        while self.video.receive_frame(&mut decoded).is_ok() {
            let f = self.to_rgba(&decoded)?;
            self.pending.push_back(SourceEvent::Video(f));
            self.last_video = Instant::now();
        }
        Ok(())
    }

    fn to_rgba(&mut self, src: &frame::Video) -> Result<Frame, SourceError> {
        let is_hw = unsafe { (*src.as_ptr()).format == ffi::AVPixelFormat::AV_PIX_FMT_D3D11 as i32 };
        let mut transferred = frame::Video::empty();
        let src = if is_hw {
            let r = unsafe { ffi::av_hwframe_transfer_data(transferred.as_mut_ptr(), src.as_ptr(), 0) };
            if r < 0 {
                HW_BROKEN.store(true, Ordering::Relaxed);
                return Err(SourceError::Other("Falha na decodificação por hardware; usando software".into()));
            }
            &transferred
        } else {
            src
        };

        let key = (src.format(), src.width(), src.height());
        if self.scaler.as_ref().is_none_or(|(_, k)| *k != key) {
            let ctx = scaling::Context::get(key.0, key.1, key.2, Pixel::RGBA, key.1, key.2, scaling::Flags::BILINEAR)
                .map_err(|e| SourceError::Other(e.to_string()))?;
            self.scaler = Some((ctx, key));
        }
        let (scaler, _) = self.scaler.as_mut().expect("scaler just created");
        let mut out = frame::Video::empty();
        scaler.run(src, &mut out).map_err(|e| SourceError::Other(e.to_string()))?;

        let (w, h) = (out.width() as usize, out.height() as usize);
        let stride = out.stride(0);
        let data = out.data(0);
        let mut rgba = Vec::with_capacity(w * h * 4);
        for row in 0..h {
            rgba.extend_from_slice(&data[row * stride..row * stride + w * 4]);
        }
        Ok(Frame { width: w as u32, height: h as u32, rgba, seq: 0 })
    }

    fn drain_audio(&mut self) {
        let Some(a) = self.audio.as_mut() else { return };
        let mut decoded = frame::Audio::empty();
        while a.decoder.receive_frame(&mut decoded).is_ok() {
            if a.resampler.is_none() {
                let layout = ChannelLayout::default(decoded.channels() as i32);
                match resampling::Context::get(
                    decoded.format(),
                    layout,
                    decoded.rate(),
                    Sample::F32(sample::Type::Packed),
                    ChannelLayout::MONO,
                    AUDIO_RATE,
                ) {
                    Ok(r) => a.resampler = Some(r),
                    Err(e) => {
                        tracing::warn!("áudio desativado: {e}");
                        self.audio = None;
                        return;
                    }
                }
            }
            let resampler = a.resampler.as_mut().expect("resampler just created");
            // G.711 frames come with an "unspecified" layout; make it match the
            // native layout the resampler was configured with.
            unsafe {
                let raw = decoded.as_mut_ptr();
                let channels = (*raw).ch_layout.nb_channels;
                ffi::av_channel_layout_uninit(&mut (*raw).ch_layout);
                ffi::av_channel_layout_default(&mut (*raw).ch_layout, channels);
            }
            // Room for the upsampled output (8 kHz → 48 kHz is 6x) plus internal delay.
            let capacity = decoded.samples() * (AUDIO_RATE as usize) / (decoded.rate().max(1) as usize) + 256;
            let mut out = frame::Audio::new(Sample::F32(sample::Type::Packed), capacity, ChannelLayout::MONO);
            if let Err(e) = resampler.run(&decoded, &mut out) {
                tracing::debug!("reamostragem falhou: {e}");
            } else if out.samples() > 0 {
                let samples = out.plane::<f32>(0)[..out.samples()].to_vec();
                self.pending.push_back(SourceEvent::Audio(samples));
            }
        }
    }
}

impl Source for FfmpegSource {
    fn next_event(&mut self) -> Result<SourceEvent, SourceError> {
        loop {
            if let Some(e) = self.pending.pop_front() {
                return Ok(e);
            }
            if self.last_video.elapsed() > STALL {
                return Err(SourceError::Stalled);
            }
            let mut packet = Packet::empty();
            match packet.read(&mut self.input) {
                Ok(()) => {}
                Err(ff::Error::Eof) => return Err(SourceError::Other("A câmera encerrou o stream".into())),
                Err(ff::Error::Other { errno }) if errno == ff::util::error::EAGAIN => continue,
                Err(e) => return Err(map_err(e, &self.host)),
            }
            let idx = packet.stream();
            if idx == self.video_stream {
                if let Err(e) = self.video.send_packet(&packet) {
                    tracing::debug!("pacote de vídeo descartado: {e}");
                }
                self.drain_video()?;
            } else if let Some(a) = self.audio.as_mut().filter(|a| a.stream == idx) {
                if a.decoder.send_packet(&packet).is_ok() {
                    self.drain_audio();
                }
            }
        }
    }

    fn has_audio(&self) -> bool {
        self.audio.is_some()
    }
}

/// Tries to attach a D3D11VA device so the decoder outputs GPU frames.
unsafe fn attach_d3d11(ctx: &mut codec::Context) -> bool {
    let mut dev: *mut ffi::AVBufferRef = ptr::null_mut();
    let r = unsafe {
        ffi::av_hwdevice_ctx_create(&mut dev, ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_D3D11VA, ptr::null(), ptr::null_mut(), 0)
    };
    if r < 0 || dev.is_null() {
        return false;
    }
    // The codec context takes ownership of this reference and unrefs it on close.
    unsafe { (*ctx.as_mut_ptr()).hw_device_ctx = dev };
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unauthorized_maps_to_auth() {
        assert_eq!(map_err(ff::Error::HttpUnauthorized, "h"), SourceError::Auth);
    }

    #[test]
    fn unknown_error_keeps_message() {
        assert!(matches!(map_err(ff::Error::InvalidData, "h"), SourceError::Other(_)));
    }
}

/// Decodes a generated H.264 + G.711 A-law file (Tapo's codecs) through the
/// real FFmpeg path. Needs `ffmpeg.exe` on PATH (vendor/ffmpeg/bin); skipped otherwise.
#[cfg(test)]
mod file_tests {
    use super::*;

    fn make_fixture(dir: &std::path::Path) -> Option<std::path::PathBuf> {
        let out = dir.join("tapo-like.mkv");
        let status = std::process::Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y"])
            .args(["-f", "lavfi", "-i", "testsrc=size=640x360:rate=15"])
            .args(["-f", "lavfi", "-i", "sine=frequency=440:sample_rate=8000"])
            .args(["-t", "2", "-c:v", "h264_mf", "-c:a", "pcm_alaw", "-ar", "8000", "-ac", "1"])
            .arg(&out)
            .status()
            .ok()?;
        status.success().then_some(out)
    }

    #[test]
    fn decodes_h264_video_and_alaw_audio() {
        init();
        let dir = tempfile::tempdir().unwrap();
        let Some(path) = make_fixture(dir.path()) else {
            eprintln!("ffmpeg.exe indisponível; teste ignorado");
            return;
        };
        let mut src = FfmpegSource::open(path.to_str().unwrap(), "local").expect("open");
        assert!(src.has_audio());

        let (mut frames, mut samples) = (0, 0usize);
        loop {
            let ev = match src.next_event() {
                Ok(ev) => ev,
                Err(e) => {
                    eprintln!("fim: {e:?}");
                    break;
                }
            };
            match ev {
                SourceEvent::Video(f) => {
                    assert_eq!((f.width, f.height), (640, 360));
                    assert_eq!(f.rgba.len(), 640 * 360 * 4);
                    frames += 1;
                }
                SourceEvent::Audio(s) => samples += s.len(),
            }
        }
        assert!(frames >= 25, "frames = {frames}");
        // 2 s of 8 kHz audio resampled to 48 kHz ≈ 96 000 samples.
        assert!((80_000..=100_000).contains(&samples), "samples = {samples}");
    }
}
