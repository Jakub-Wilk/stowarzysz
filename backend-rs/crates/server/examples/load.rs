//! Load generator for a running server; see `docs/load-testing.md`.
//!
//! ```sh
//! cargo run --release --example load -- --base http://127.0.0.1:8099 --user perf --password ... \
//!     http --concurrency 64 --seconds 10 /api/ledger/balances /api/polls
//! cargo run --release --example load -- ... sse --clients 1000 --events 20 --poll 10
//! cargo run --release --example load -- ... upload --file big.png --concurrency 4
//! ```
#![allow(clippy::print_stdout, clippy::unwrap_used, clippy::cast_precision_loss)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use clap::{Parser, Subcommand};
use futures_util::StreamExt as _;
use serde_json::{Value, json};
use tokio::sync::Barrier;

#[derive(Parser)]
struct Cli {
    #[arg(long, default_value = "http://127.0.0.1:8099")]
    base: String,
    #[arg(long)]
    user: String,
    #[arg(long)]
    password: String,
    #[command(subcommand)]
    test: Test,
}

#[derive(Subcommand)]
enum Test {
    /// Hammer each path with `concurrency` workers for `seconds`; prints throughput and latency.
    Http {
        #[arg(long, default_value_t = 64)]
        concurrency: usize,
        #[arg(long, default_value_t = 10)]
        seconds: u64,
        paths: Vec<String>,
    },
    /// Open `clients` event streams, then send `events` reactions to closed poll `poll` (each is
    /// broadcast to every stream) and measure how long each takes to reach every client.
    Sse {
        #[arg(long, default_value_t = 1000)]
        clients: usize,
        #[arg(long, default_value_t = 20)]
        events: usize,
        #[arg(long)]
        poll: i64,
    },
    /// Upload `file` (`image/*`) `concurrency` times at once.
    Upload {
        #[arg(long)]
        file: std::path::PathBuf,
        #[arg(long, default_value_t = 4)]
        concurrency: usize,
    },
}

/// Latency summary in milliseconds.
fn summary(mut samples: Vec<Duration>) -> String {
    if samples.is_empty() {
        return "no samples".into();
    }
    samples.sort_unstable();
    let at = |percent: usize| samples[(samples.len() - 1) * percent / 100].as_secs_f64() * 1000.0;
    format!(
        "p50 {:.2} ms  p90 {:.2} ms  p99 {:.2} ms  max {:.2} ms",
        at(50),
        at(90),
        at(99),
        at(100)
    )
}

async fn login(client: &reqwest::Client, cli: &Cli) -> anyhow::Result<String> {
    let response: Value = client
        .post(format!("{}/api/auth/login", cli.base))
        .json(&json!({"username": cli.user, "password": cli.password}))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(response["access"].as_str().unwrap_or_default().to_owned())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let client = reqwest::Client::builder()
        .pool_max_idle_per_host(2048)
        .build()?;
    let token = login(&client, &cli).await?;
    match &cli.test {
        Test::Http {
            concurrency,
            seconds,
            paths,
        } => {
            for path in paths {
                http(&client, &cli.base, &token, path, *concurrency, *seconds).await;
            }
        }
        Test::Sse {
            clients,
            events,
            poll,
        } => sse(&client, &cli.base, &token, *clients, *events, *poll).await?,
        Test::Upload { file, concurrency } => {
            upload(&client, &cli.base, &token, file, *concurrency).await?;
        }
    }
    Ok(())
}

async fn http(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    path: &str,
    concurrency: usize,
    seconds: u64,
) {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let errors = Arc::new(AtomicUsize::new(0));
    let mut workers = Vec::new();
    for _ in 0..concurrency {
        let (client, url, token, errors) = (
            client.clone(),
            format!("{base}{path}"),
            token.to_owned(),
            Arc::clone(&errors),
        );
        workers.push(tokio::spawn(async move {
            let mut samples = Vec::new();
            while Instant::now() < deadline {
                let started = Instant::now();
                let ok = match client.get(&url).bearer_auth(&token).send().await {
                    Ok(response) => {
                        let ok = response.status().is_success();
                        ok && response.bytes().await.is_ok()
                    }
                    Err(_) => false,
                };
                if ok {
                    samples.push(started.elapsed());
                } else {
                    errors.fetch_add(1, Ordering::Relaxed);
                }
            }
            samples
        }));
    }
    let mut samples = Vec::new();
    for worker in workers {
        samples.extend(worker.await.unwrap());
    }
    println!(
        "{path:<40} {:>8.0} req/s  {}  errors {}",
        samples.len() as f64 / seconds as f64,
        summary(samples),
        errors.load(Ordering::Relaxed)
    );
}

async fn sse(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    clients: usize,
    events: usize,
    poll: i64,
) -> anyhow::Result<()> {
    let ready = Arc::new(Barrier::new(clients + 1));
    let mut streams = Vec::new();
    let opened = Instant::now();
    for _ in 0..clients {
        let (client, url, token, ready) = (
            client.clone(),
            format!("{base}/api/events"),
            token.to_owned(),
            Arc::clone(&ready),
        );
        streams.push(tokio::spawn(async move {
            let response = client.get(url).bearer_auth(token).send().await?;
            anyhow::ensure!(response.status().is_success(), "{}", response.status());
            let mut body = response.bytes_stream();
            let mut buffer = String::new();
            let mut arrivals = Vec::new();
            let mut is_ready = false;
            while let Some(chunk) = body.next().await {
                buffer.push_str(&String::from_utf8_lossy(&chunk?));
                while let Some(end) = buffer.find("\n\n") {
                    let message: String = buffer.drain(..end + 2).collect();
                    if message.contains("event: ready") && !is_ready {
                        is_ready = true;
                        ready.wait().await;
                    } else if message.contains("event: poll.reaction") {
                        arrivals.push(Instant::now());
                        if arrivals.len() == events {
                            return Ok(arrivals);
                        }
                    }
                }
            }
            anyhow::bail!("stream ended early")
        }));
    }
    ready.wait().await;
    println!(
        "{clients} streams open and ready in {:.2} s",
        opened.elapsed().as_secs_f64()
    );
    let mut sent = Vec::new();
    for i in 0..events {
        let emoji = ["❤️", "🔥", "😭", "👎", "🤣"][i % 5];
        sent.push(Instant::now());
        client
            .post(format!("{base}/api/polls/{poll}/reactions"))
            .bearer_auth(token)
            .json(&json!({"emoji": emoji}))
            .send()
            .await?
            .error_for_status()?;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let mut latencies = Vec::new();
    let mut failed = 0;
    for stream in streams {
        match tokio::time::timeout(Duration::from_secs(30), stream).await {
            Ok(Ok(Ok(arrivals))) => {
                latencies.extend(arrivals.iter().zip(&sent).map(|(got, at)| *got - *at));
            }
            _ => failed += 1,
        }
    }
    println!(
        "{events} broadcasts x {clients} clients: delivered {} of {}, {failed} clients incomplete",
        latencies.len(),
        clients * events
    );
    println!("delivery latency: {}", summary(latencies));
    Ok(())
}

async fn upload(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    file: &std::path::Path,
    concurrency: usize,
) -> anyhow::Result<()> {
    let bytes = bytes::Bytes::from(std::fs::read(file)?);
    println!(
        "uploading {:.1} MB x {concurrency} at once",
        bytes.len() as f64 / 1e6
    );
    let started = Instant::now();
    let mut uploads = Vec::new();
    for _ in 0..concurrency {
        let (client, url, token, bytes) = (
            client.clone(),
            format!("{base}/api/uploads?purpose=photo"),
            token.to_owned(),
            bytes.clone(),
        );
        uploads.push(tokio::spawn(async move {
            let at = Instant::now();
            let response = client
                .post(url)
                .bearer_auth(token)
                .header("content-type", "image/png")
                .body(bytes)
                .send()
                .await?;
            anyhow::Ok((response.status(), at.elapsed()))
        }));
    }
    for upload in uploads {
        let (status, took) = upload.await??;
        println!("  {status} in {:.2} s", took.as_secs_f64());
    }
    println!("all done in {:.2} s", started.elapsed().as_secs_f64());
    Ok(())
}
