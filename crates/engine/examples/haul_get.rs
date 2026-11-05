//! A command-line downloader for trying the engine by hand.
//!
//! cargo run -p haul-engine --release --example haul_get -- <url> [--conns N] [--out DIR] [--limit BYTES_PER_SEC]
//!
//! Ctrl+C pauses cleanly; running the same command again resumes (state lives in <out>/.haul-cli.db).

use haul_engine::{
    DownloadState, Event, EventBus, HostLimiter, NewDownload, Priority, Queue, RateLimiter, Store, TransferConfig,
    TransferCtx, http_client,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

struct Args {
    url: String,
    conns: u32,
    out: PathBuf,
    limit: Option<u64>,
}

fn parse_args() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let (mut url, mut conns, mut out, mut limit) = (None, 8u32, PathBuf::from("."), None);
    while let Some(a) = it.next() {
        let mut val = |name: &str| it.next().ok_or(format!("{name} needs a value"));
        match a.as_str() {
            "--conns" => conns = val("--conns")?.parse().map_err(|_| "bad --conns")?,
            "--out" => out = PathBuf::from(val("--out")?),
            "--limit" => limit = Some(val("--limit")?.parse().map_err(|_| "bad --limit")?),
            s if s.starts_with("--") => return Err(format!("unknown flag {s}")),
            s => url = Some(s.to_string()),
        }
    }
    let url = url.ok_or("usage: haul_get <url> [--conns N] [--out DIR] [--limit BYTES_PER_SEC]")?;
    Ok(Args { url, conns: conns.clamp(1, 32), out, limit })
}

fn mb(bytes: f64) -> f64 {
    bytes / 1_000_000.0
}

#[tokio::main]
async fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let store = Store::open(&args.out.join(".haul-cli.db")).expect("open state db");
    let ctx = TransferCtx {
        client: http_client(),
        store: store.clone(),
        events: EventBus::new(),
        global_limit: Arc::new(RateLimiter::new(None)),
        hosts: Arc::new(HostLimiter::new(16)),
        config: TransferConfig::default(),
    };
    let mut rx = ctx.events.subscribe();

    // Resume an unfinished download of the same URL into the same folder.
    let existing = store
        .list_downloads()
        .unwrap_or_default()
        .into_iter()
        .find(|d| d.url == args.url && d.target_dir == args.out && d.state != DownloadState::Completed);
    let queue = Queue::start(ctx.clone(), 1);
    let id = match existing {
        Some(d) => {
            println!("resuming download {}", d.id);
            queue.resume(d.id);
            d.id
        }
        None => queue
            .add(NewDownload {
                package_id: None,
                url: args.url.clone(),
                headers_json: None,
                target_dir: args.out.clone(),
                file_name: None,
                priority: Priority::Normal,
                connections: args.conns,
                speed_limit: args.limit,
            })
            .expect("add download"),
    };

    let started = Instant::now();
    let mut last_print = Instant::now() - Duration::from_secs(1);
    let mut last = (0u64, None::<u64>);
    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);
    loop {
        tokio::select! {
            _ = &mut ctrl_c => {
                queue.shutdown().await;
                let pct = last.1.map_or(0.0, |s| last.0 as f64 * 100.0 / s.max(1) as f64);
                println!("\npaused at {pct:.1}%; run the same command again to resume");
                return;
            }
            ev = rx.recv() => match ev {
                Ok(Event::Progress { id: pid, written, size, speed_bps, segments }) if pid == id => {
                    last = (written, size);
                    if last_print.elapsed() >= Duration::from_secs(1) {
                        last_print = Instant::now();
                        let active = segments.iter().filter(|(_, p, e)| p < e).count();
                        match size {
                            Some(s) => println!(
                                "{:5.1}% {:7.1} MB/s {} conn",
                                written as f64 * 100.0 / s.max(1) as f64,
                                mb(speed_bps as f64),
                                active
                            ),
                            None => println!("{:9.1} MB {:7.1} MB/s", mb(written as f64), mb(speed_bps as f64)),
                        }
                    }
                }
                Ok(Event::StateChanged { id: pid, state, error }) if pid == id => match state {
                    DownloadState::Completed => {
                        let d = store.get_download(id).expect("row");
                        let secs = started.elapsed().as_secs_f64();
                        let size = d.probe.and_then(|p| p.size).unwrap_or(0);
                        println!(
                            "done: {} ({} bytes, {:.1} s, avg {:.1} MB/s)",
                            d.target_path.map(|p| p.display().to_string()).unwrap_or_default(),
                            size,
                            secs,
                            mb(size as f64) / secs.max(0.001)
                        );
                        queue.shutdown().await;
                        return;
                    }
                    DownloadState::Failed | DownloadState::Offline | DownloadState::LinkExpired => {
                        eprintln!("failed: {state:?} {}", error.unwrap_or_default());
                        queue.shutdown().await;
                        std::process::exit(1);
                    }
                    DownloadState::Paused if error.is_some() => {
                        eprintln!("paused: {}", error.unwrap_or_default());
                        queue.shutdown().await;
                        std::process::exit(1);
                    }
                    _ => {}
                },
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(_) => return,
            }
        }
    }
}
