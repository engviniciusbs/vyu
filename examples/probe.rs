//! Diagnose a camera without the UI:
//!   cargo run --example probe -- <host> <user> <password> [high|low] [port]

use std::time::{Duration, Instant};

use vyu::stream::StreamVariant;
use vyu::stream::ffmpeg::{FfmpegFactory, init};
use vyu::stream::session::{SourceEvent, SourceFactory};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        eprintln!("uso: probe <host> <usuario> <senha> [high|low] [porta]");
        std::process::exit(2);
    }
    init();
    let variant = if args.get(3).map(String::as_str) == Some("high") { StreamVariant::High } else { StreamVariant::Low };
    let port = args.get(4).and_then(|p| p.parse().ok()).unwrap_or(554);
    let factory = FfmpegFactory { host: args[0].clone(), port, user: args[1].clone(), pass: args[2].clone() };

    let start = Instant::now();
    let mut src = match factory.open(variant) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("falhou: {}", e.message());
            std::process::exit(1);
        }
    };
    println!("conectado em {:?}; áudio: {}", start.elapsed(), src.has_audio());

    let (mut frames, mut samples) = (0u32, 0usize);
    let until = Instant::now() + Duration::from_secs(5);
    while Instant::now() < until {
        match src.next_event() {
            Ok(SourceEvent::Video(f)) => {
                if frames == 0 {
                    println!("primeiro quadro {}x{} em {:?}", f.width, f.height, start.elapsed());
                }
                frames += 1;
            }
            Ok(SourceEvent::Audio(s)) => samples += s.len(),
            Err(e) => {
                eprintln!("erro: {}", e.message());
                break;
            }
        }
    }
    println!("5 s: {frames} quadros ({:.1} fps), {samples} amostras de áudio", frames as f32 / 5.0);
}
