use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, bail};
use app::clock::SystemClock;
use app::config::Config;
use app::push::{PushWorker, WebPushTransport};
use app::scheduler::Scheduler;
use app::{Context, db, listener};
use clap::{Parser, Subcommand};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

#[derive(Debug, Parser)]
#[command(name = "stowarzysz", version, about = "The stowarzysz backend")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Apply migrations, then serve the HTTP API and run the background workers (the default).
    Serve,
    /// Apply pending database migrations and exit.
    Migrate,
    /// Run every background job once (and deliver queued push notifications), then exit.
    RunJobs,
    /// Create an administrator who can sign in straight away (the first account, typically).
    CreateSuperuser {
        #[arg(long)]
        username: String,
        /// Read the password from the first line of standard input instead of prompting.
        #[arg(long)]
        password_stdin: bool,
    },
    /// Print fresh secrets for a new deployment (`.env` lines).
    GenerateKeys,
    /// Read a receipt photo with the configured OCR candidates and print the result as JSON
    /// (progress goes to standard error). Needs no database.
    Ocr {
        /// The image file.
        file: std::path::PathBuf,
    },
    /// Exit 0 if the local server answers `GET /api/health` (for container health checks).
    Healthcheck {
        /// `host:port` of the server; defaults to the port of `BIND_ADDR` on localhost.
        #[arg(long)]
        addr: Option<String>,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => serve().await,
        Command::Migrate => migrate().await,
        Command::RunJobs => run_jobs().await,
        Command::CreateSuperuser {
            username,
            password_stdin,
        } => create_superuser(&username, password_stdin).await,
        Command::GenerateKeys => {
            generate_keys();
            Ok(())
        }
        Command::Healthcheck { addr } => healthcheck(addr).await,
        Command::Ocr { file } => ocr(file).await,
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// Configuration, logging and a context over a migrated database.
async fn context() -> anyhow::Result<Arc<Context>> {
    let config = Config::from_env()?;
    server::telemetry::init(config.log_format);
    let pool = db::pool(&config)?;
    migrate_with_retry(&pool).await?;
    let ctx = Context::new(config, pool, Arc::new(SystemClock));
    ctx.media
        .init()
        .await
        .context("cannot prepare MEDIA_ROOT")?;
    Ok(Arc::new(ctx))
}

/// The database may still be starting (compose brings services up together).
async fn migrate_with_retry(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    let mut attempt = 1;
    loop {
        match db::migrate(pool).await {
            Ok(()) => return Ok(()),
            Err(error) if attempt < 10 => {
                tracing::warn!(
                    attempt,
                    error = format!("{error:#}"),
                    "database not ready; retrying"
                );
                tokio::time::sleep(Duration::from_secs(2)).await;
                attempt += 1;
            }
            Err(error) => return Err(error.context("cannot migrate the database")),
        }
    }
}

async fn serve() -> anyhow::Result<()> {
    let ctx = context().await?;
    let config = &ctx.config;
    tracing::info!(
        env = ?config.env,
        bind = %config.bind_addr,
        push = config.vapid.is_some(),
        gemini = config.gemini.api_key.is_some(),
        vertex = config.vertex.is_some(),
        "starting stowarzysz",
    );

    let shutdown = CancellationToken::new();
    let tasks = TaskTracker::new();
    tasks.spawn(listener::run(
        ctx.db.clone(),
        ctx.hub.clone(),
        ctx.signals.clone(),
        shutdown.clone(),
    ));
    if let Some(vapid) = &config.vapid {
        let transport = WebPushTransport::new(vapid).context("invalid VAPID keys")?;
        let worker = PushWorker::new(ctx.db.clone(), Arc::new(transport));
        tasks.spawn(worker.run(Arc::clone(&ctx.signals.push), shutdown.clone()));
    }
    tasks.spawn(Scheduler::standard().run(Arc::clone(&ctx), shutdown.clone()));
    tasks.close();

    let listener = TcpListener::bind(config.bind_addr)
        .await
        .with_context(|| format!("cannot bind {}", config.bind_addr))?;
    let state = server::AppState::with_shutdown(Arc::clone(&ctx), shutdown.clone());
    let app = server::http::router(state);
    let stop = shutdown.clone();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        server::shutdown::signal().await;
        stop.cancel(); // ends SSE streams and background loops so the server can drain
    })
    .await
    .context("server error")?;

    shutdown.cancel();
    if tokio::time::timeout(Duration::from_secs(10), tasks.wait())
        .await
        .is_err()
    {
        tracing::warn!("background tasks did not stop within 10 s");
    }
    tracing::info!("stopped");
    Ok(())
}

async fn migrate() -> anyhow::Result<()> {
    let config = Config::from_env()?;
    server::telemetry::init(config.log_format);
    let pool = db::pool(&config)?;
    db::migrate(&pool).await?;
    tracing::info!("migrations applied");
    Ok(())
}

async fn run_jobs() -> anyhow::Result<()> {
    let ctx = context().await?;
    let mut failed = false;
    for (job, result) in Scheduler::standard().run_once(&ctx).await {
        match result {
            Ok(next) => tracing::info!(job, ?next, "job ran"),
            Err(error) => {
                failed = true;
                tracing::error!(job, error = format!("{error:#}"), "job failed");
            }
        }
    }
    if let Some(vapid) = &ctx.config.vapid {
        let transport = WebPushTransport::new(vapid).context("invalid VAPID keys")?;
        PushWorker::new(ctx.db.clone(), Arc::new(transport))
            .drain()
            .await?;
    }
    if failed {
        bail!("some jobs failed");
    }
    Ok(())
}

async fn create_superuser(username: &str, password_stdin: bool) -> anyhow::Result<()> {
    let password = if password_stdin {
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        line.trim_end_matches(['\r', '\n']).to_owned()
    } else {
        let first = rpassword::prompt_password("Hasło: ")?;
        let second = rpassword::prompt_password("Powtórz hasło: ")?;
        if first != second {
            bail!("the passwords differ");
        }
        first
    };
    let ctx = context().await?;
    match app::accounts::users::create_superuser(&ctx, username, &password).await {
        Ok(id) => {
            tracing::info!(id, username, "administrator created");
            Ok(())
        }
        Err(app::AppError::Invalid(errors)) => {
            let reasons: Vec<String> = errors.iter().map(ToString::to_string).collect();
            bail!("{}", reasons.join("; "))
        }
        Err(error) => Err(error.into()),
    }
}

/// Random secrets for `.env.prod`. Keep them: changing `JWT_SECRET` signs everyone out,
/// changing `SANTA_SEAL_KEY` makes an active Secret Santa draw unreadable.
#[allow(clippy::print_stdout)] // the output is the point
async fn ocr(file: std::path::PathBuf) -> anyhow::Result<()> {
    let config = Config::from_env()?;
    server::telemetry::init(config.log_format);
    let ocr = app::ocr::Ocr::new(&config);
    let candidates: Vec<&str> = ocr.router().candidates().collect();
    if candidates.is_empty() {
        bail!("no OCR candidates: set GEMINI_API_KEY (or VERTEX_PROJECT and VERTEX_CREDENTIALS)");
    }
    eprintln!("candidates: {}", candidates.join(", "));
    let media = app::media::Media::new(&config);
    let started = std::time::Instant::now();
    let compressed = app::ocr::compress_file(
        &media,
        file,
        config.ocr.max_side,
        config.gemini.inline_budget_bytes,
    )
    .await
    .map_err(|error| anyhow::anyhow!("{error}"))?;
    eprintln!(
        "compressed to {}x{} ({} bytes, {}) in {:.1?}",
        compressed.width,
        compressed.height,
        compressed.webp.len(),
        if compressed.lossless {
            "lossless"
        } else {
            "lossy"
        },
        started.elapsed(),
    );
    let (progress, mut attempts) = tokio::sync::mpsc::unbounded_channel::<app::ocr::Attempt>();
    let printer = tokio::spawn(async move {
        while let Some(attempt) = attempts.recv().await {
            eprintln!("request {} -> {}", attempt.number, attempt.candidate);
        }
    });
    let served = ocr.router().read(&compressed.webp, &progress).await;
    drop(progress);
    let _ = printer.await;
    let served = served?;
    eprintln!(
        "served by {} in {:.1?}",
        served.candidate,
        started.elapsed()
    );
    println!("{}", serde_json::to_string_pretty(&served.receipt)?);
    Ok(())
}

#[allow(clippy::print_stdout)] // the output is the point
fn generate_keys() {
    use base64::Engine as _;
    use rand::RngCore as _;
    let random = |len: usize| {
        let mut bytes = vec![0_u8; len];
        rand::rng().fill_bytes(&mut bytes);
        bytes
    };
    let url = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let (vapid_private, vapid_public) = app::push::generate_vapid_keys();
    println!("JWT_SECRET={}", url.encode(random(48)));
    println!("MEDIA_SIGNING_KEY={}", url.encode(random(48)));
    println!(
        "SANTA_SEAL_KEY={}",
        base64::engine::general_purpose::STANDARD.encode(random(32))
    );
    println!("VAPID_PRIVATE_KEY={vapid_private}");
    println!("VAPID_PUBLIC_KEY={vapid_public}");
    println!("VAPID_SUBJECT=mailto:you@example.com");
}

/// A dependency-free HTTP/1.1 probe: the runtime image has no curl.
async fn healthcheck(addr: Option<String>) -> anyhow::Result<()> {
    let addr = addr.unwrap_or_else(|| {
        let port = std::env::var("BIND_ADDR")
            .ok()
            .and_then(|bind| bind.parse::<std::net::SocketAddr>().ok())
            .map_or(8000, |bind| bind.port());
        format!("127.0.0.1:{port}")
    });
    let probe = async {
        let mut stream = TcpStream::connect(&addr).await?;
        stream
            .write_all(b"GET /api/health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await?;
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await?;
        anyhow::Ok(response)
    };
    let response = tokio::time::timeout(Duration::from_secs(4), probe)
        .await
        .context("timed out")?
        .with_context(|| format!("cannot reach {addr}"))?;
    let status_line = response.split(|&b| b == b'\n').next().unwrap_or_default();
    if !status_line.starts_with(b"HTTP/1.1 200") {
        bail!("unhealthy: {}", String::from_utf8_lossy(status_line).trim());
    }
    Ok(())
}
