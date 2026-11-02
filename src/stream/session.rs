use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use super::StreamVariant;
use super::backoff::Backoff;
use super::frame::{Frame, SharedFrame};

#[derive(Debug, Clone, PartialEq)]
pub enum SessionStatus {
    Connecting,
    Live { has_audio: bool },
    Reconnecting { attempt: u32, retry_in: Duration, reason: String },
    AuthFailed,
    Stopped,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SourceError {
    /// RTSP 401: wrong user or password. Retrying will not help.
    Auth,
    Unreachable(String),
    Stalled,
    Other(String),
}

impl SourceError {
    /// User-facing message (pt-BR).
    pub fn message(&self) -> String {
        match self {
            SourceError::Auth => "Usuário ou senha incorretos".into(),
            SourceError::Unreachable(host) => format!("Câmera não encontrada em {host}"),
            SourceError::Stalled => "A câmera parou de enviar vídeo".into(),
            SourceError::Other(msg) => msg.clone(),
        }
    }
}

pub enum SourceEvent {
    Video(Frame),
    /// Mono f32 samples at 48 kHz.
    Audio(Vec<f32>),
}

/// An open camera stream that yields decoded events.
pub trait Source: Send {
    fn next_event(&mut self) -> Result<SourceEvent, SourceError>;
    fn has_audio(&self) -> bool;
}

/// Opens a camera stream for a given quality.
pub trait SourceFactory: Send + 'static {
    fn open(&self, v: StreamVariant) -> Result<Box<dyn Source>, SourceError>;
}

#[derive(Debug)]
pub enum Command {
    SetVariant(StreamVariant),
    Stop,
}

pub type AudioSink = Box<dyn FnMut(&[f32]) + Send>;
pub type Repaint = Box<dyn Fn() + Send>;

pub enum WaitResult {
    Timeout,
    Cmd(Command),
    Closed,
}

/// Waits for the backoff delay, returning early if a command arrives.
pub type Waiter = Box<dyn FnMut(Duration, &Receiver<Command>) -> WaitResult + Send>;

fn real_waiter() -> Waiter {
    Box::new(|d, rx| match rx.recv_timeout(d) {
        Ok(c) => WaitResult::Cmd(c),
        Err(RecvTimeoutError::Timeout) => WaitResult::Timeout,
        Err(RecvTimeoutError::Disconnected) => WaitResult::Closed,
    })
}

pub struct SessionHandle {
    pub frame: SharedFrame,
    pub status: Arc<Mutex<SessionStatus>>,
    variant: Arc<Mutex<StreamVariant>>,
    tx: Sender<Command>,
    thread: Option<JoinHandle<()>>,
}

impl SessionHandle {
    pub fn send(&self, c: Command) {
        let _ = self.tx.send(c);
    }

    pub fn status(&self) -> SessionStatus {
        self.status.lock().unwrap().clone()
    }

    pub fn variant(&self) -> StreamVariant {
        *self.variant.lock().unwrap()
    }
}

impl Drop for SessionHandle {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Stop);
        // An FFmpeg read can block for up to the 5 s I/O timeout; don't freeze the
        // UI for that. Wait briefly, then let the thread finish on its own.
        if let Some(t) = self.thread.take() {
            let deadline = std::time::Instant::now() + Duration::from_millis(300);
            while !t.is_finished() && std::time::Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(5));
            }
            if t.is_finished() {
                let _ = t.join();
            }
        }
    }
}

pub struct Session;

impl Session {
    pub fn spawn(factory: impl SourceFactory, initial: StreamVariant, audio: AudioSink, repaint: Repaint) -> SessionHandle {
        Self::spawn_with_waiter(factory, initial, audio, repaint, real_waiter())
    }

    pub(crate) fn spawn_with_waiter(
        factory: impl SourceFactory,
        initial: StreamVariant,
        audio: AudioSink,
        repaint: Repaint,
        waiter: Waiter,
    ) -> SessionHandle {
        let (tx, rx) = mpsc::channel();
        let frame: SharedFrame = Arc::default();
        let status = Arc::new(Mutex::new(SessionStatus::Connecting));
        let variant = Arc::new(Mutex::new(initial));
        let worker = Worker {
            factory: Box::new(factory),
            rx,
            frame: frame.clone(),
            status: status.clone(),
            variant: variant.clone(),
            audio,
            repaint,
            waiter,
            backoff: Backoff::new(),
            attempt: 0,
            seq: 0,
        };
        let thread = std::thread::Builder::new()
            .name("vyu-session".into())
            .spawn(move || worker.run())
            .expect("spawn session thread");
        SessionHandle { frame, status, variant, tx, thread: Some(thread) }
    }
}

struct Worker {
    factory: Box<dyn SourceFactory>,
    rx: Receiver<Command>,
    frame: SharedFrame,
    status: Arc<Mutex<SessionStatus>>,
    variant: Arc<Mutex<StreamVariant>>,
    audio: AudioSink,
    repaint: Repaint,
    waiter: Waiter,
    backoff: Backoff,
    attempt: u32,
    seq: u64,
}

/// What the worker should do after handling an event.
enum Next {
    Reopen,
    Exit,
}

impl Worker {
    fn set_status(&self, s: SessionStatus) {
        *self.status.lock().unwrap() = s;
        (self.repaint)();
    }

    fn current_variant(&self) -> StreamVariant {
        *self.variant.lock().unwrap()
    }

    fn apply(&mut self, c: Command) -> Option<Next> {
        match c {
            Command::Stop => Some(Next::Exit),
            Command::SetVariant(v) if v != self.current_variant() => {
                *self.variant.lock().unwrap() = v;
                Some(Next::Reopen)
            }
            Command::SetVariant(_) => None,
        }
    }

    fn run(mut self) {
        loop {
            let next = match self.factory.open(self.current_variant()) {
                Ok(src) => self.pump(src),
                Err(e) => self.on_error(e),
            };
            if let Next::Exit = next {
                break;
            }
        }
        self.set_status(SessionStatus::Stopped);
    }

    /// Reads events until an error or a command forces a reopen/exit.
    fn pump(&mut self, mut src: Box<dyn Source>) -> Next {
        let mut live = false;
        loop {
            match self.rx.try_recv() {
                Ok(c) => {
                    if let Some(n) = self.apply(c) {
                        return n;
                    }
                }
                Err(TryRecvError::Disconnected) => return Next::Exit,
                Err(TryRecvError::Empty) => {}
            }
            match src.next_event() {
                Ok(SourceEvent::Video(mut f)) => {
                    self.seq += 1;
                    f.seq = self.seq;
                    *self.frame.lock().unwrap() = Some(f);
                    if !live {
                        live = true;
                        self.backoff.reset();
                        self.attempt = 0;
                        self.set_status(SessionStatus::Live { has_audio: src.has_audio() });
                    } else {
                        (self.repaint)();
                    }
                }
                Ok(SourceEvent::Audio(samples)) => {
                    if !samples.is_empty() {
                        (self.audio)(&samples);
                    }
                }
                Err(e) => return self.on_error(e),
            }
        }
    }

    fn on_error(&mut self, e: SourceError) -> Next {
        if e == SourceError::Auth {
            self.set_status(SessionStatus::AuthFailed);
            // Do not retry on our own; wait for a command (e.g. quality change).
            return match self.rx.recv() {
                Ok(c) => self.apply(c).unwrap_or(Next::Reopen),
                Err(_) => Next::Exit,
            };
        }
        self.attempt += 1;
        let delay = self.backoff.next();
        tracing::warn!("sessão: {} — nova tentativa em {}s", e.message(), delay.as_secs());
        self.set_status(SessionStatus::Reconnecting { attempt: self.attempt, retry_in: delay, reason: e.message() });
        match (self.waiter)(delay, &self.rx) {
            WaitResult::Timeout => Next::Reopen,
            WaitResult::Cmd(c) => self.apply(c).unwrap_or(Next::Reopen),
            WaitResult::Closed => Next::Exit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::time::Instant;

    type Script = VecDeque<Result<Vec<SourceEvent>, SourceError>>;

    struct FakeSource {
        events: VecDeque<SourceEvent>,
    }

    impl Source for FakeSource {
        fn next_event(&mut self) -> Result<SourceEvent, SourceError> {
            match self.events.pop_front() {
                Some(e) => Ok(e),
                None => {
                    std::thread::sleep(Duration::from_millis(2));
                    Ok(SourceEvent::Audio(Vec::new()))
                }
            }
        }
        fn has_audio(&self) -> bool {
            true
        }
    }

    #[derive(Clone)]
    struct FakeFactory {
        script: Arc<Mutex<Script>>,
        opened: Arc<Mutex<Vec<StreamVariant>>>,
    }

    impl FakeFactory {
        fn new(script: Vec<Result<Vec<SourceEvent>, SourceError>>) -> Self {
            Self { script: Arc::new(Mutex::new(script.into())), opened: Arc::default() }
        }
        fn opened(&self) -> Vec<StreamVariant> {
            self.opened.lock().unwrap().clone()
        }
    }

    impl SourceFactory for FakeFactory {
        fn open(&self, v: StreamVariant) -> Result<Box<dyn Source>, SourceError> {
            self.opened.lock().unwrap().push(v);
            match self.script.lock().unwrap().pop_front() {
                Some(Ok(events)) => Ok(Box::new(FakeSource { events: events.into() })),
                Some(Err(e)) => Err(e),
                None => Err(SourceError::Other("fim do roteiro".into())),
            }
        }
    }

    fn frame() -> SourceEvent {
        SourceEvent::Video(Frame { width: 2, height: 2, rgba: vec![0; 16], seq: 0 })
    }

    /// Records each backoff wait instead of sleeping.
    fn recording_waiter() -> (Waiter, Arc<Mutex<Vec<Duration>>>) {
        let log = Arc::new(Mutex::new(Vec::new()));
        let l = log.clone();
        let waiter: Waiter = Box::new(move |d, rx| {
            l.lock().unwrap().push(d);
            match rx.try_recv() {
                Ok(c) => WaitResult::Cmd(c),
                Err(TryRecvError::Empty) => WaitResult::Timeout,
                Err(TryRecvError::Disconnected) => WaitResult::Closed,
            }
        });
        (waiter, log)
    }

    fn wait_until(what: &str, mut cond: impl FnMut() -> bool) {
        let start = Instant::now();
        while !cond() {
            assert!(start.elapsed() < Duration::from_secs(3), "timeout waiting for {what}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn spawn(f: &FakeFactory, waiter: Waiter) -> SessionHandle {
        Session::spawn_with_waiter(f.clone(), StreamVariant::Low, Box::new(|_| {}), Box::new(|| {}), waiter)
    }

    #[test]
    fn live_publishes_frames() {
        let f = FakeFactory::new(vec![Ok(vec![frame(), frame(), frame()])]);
        let (w, _) = recording_waiter();
        let h = spawn(&f, w);
        wait_until("seq 3", || h.frame.lock().unwrap().as_ref().map(|fr| fr.seq) == Some(3));
        assert_eq!(h.status(), SessionStatus::Live { has_audio: true });
    }

    #[test]
    fn auth_error_stops_retrying() {
        let f = FakeFactory::new(vec![Err(SourceError::Auth)]);
        let (w, waits) = recording_waiter();
        let h = spawn(&f, w);
        wait_until("auth failed", || h.status() == SessionStatus::AuthFailed);
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(f.opened().len(), 1);
        assert!(waits.lock().unwrap().is_empty());
    }

    #[test]
    fn unreachable_retries_with_backoff() {
        let f = FakeFactory::new(vec![
            Err(SourceError::Unreachable("10.0.0.9".into())),
            Err(SourceError::Unreachable("10.0.0.9".into())),
            Ok(vec![frame()]),
        ]);
        let (w, waits) = recording_waiter();
        let h = spawn(&f, w);
        wait_until("live", || matches!(h.status(), SessionStatus::Live { .. }));
        assert_eq!(f.opened().len(), 3);
        assert_eq!(*waits.lock().unwrap(), vec![Duration::from_secs(1), Duration::from_secs(2)]);
    }

    #[test]
    fn reconnecting_status_has_reason() {
        let f = FakeFactory::new(vec![Err(SourceError::Unreachable("10.0.0.9".into()))]);
        let (_, _) = recording_waiter();
        // A waiter that blocks so we can observe the Reconnecting status.
        let waiter: Waiter = Box::new(|_, rx| match rx.recv() {
            Ok(c) => WaitResult::Cmd(c),
            Err(_) => WaitResult::Closed,
        });
        let h = spawn(&f, waiter);
        wait_until("reconnecting", || matches!(h.status(), SessionStatus::Reconnecting { .. }));
        match h.status() {
            SessionStatus::Reconnecting { attempt, retry_in, reason } => {
                assert_eq!(attempt, 1);
                assert_eq!(retry_in, Duration::from_secs(1));
                assert_eq!(reason, "Câmera não encontrada em 10.0.0.9");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn quality_change_restarts_source() {
        let f = FakeFactory::new(vec![Ok(vec![frame()]), Ok(vec![frame()])]);
        let (w, _) = recording_waiter();
        let h = spawn(&f, w);
        wait_until("first frame", || h.frame.lock().unwrap().is_some());
        h.send(Command::SetVariant(StreamVariant::High));
        wait_until("reopen", || f.opened().len() == 2);
        assert_eq!(f.opened(), vec![StreamVariant::Low, StreamVariant::High]);
        assert_eq!(h.variant(), StreamVariant::High);
        // The old frame stays visible until the new source delivers one.
        assert!(h.frame.lock().unwrap().is_some());
        wait_until("seq 2", || h.frame.lock().unwrap().as_ref().map(|fr| fr.seq) == Some(2));
    }

    #[test]
    fn drop_stops_session() {
        let f = FakeFactory::new(vec![Ok(vec![frame()])]);
        let (w, _) = recording_waiter();
        let h = spawn(&f, w);
        let status = h.status.clone();
        wait_until("live", || matches!(h.status(), SessionStatus::Live { .. }));
        drop(h);
        assert_eq!(*status.lock().unwrap(), SessionStatus::Stopped);
    }
}
