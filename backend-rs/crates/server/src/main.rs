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
        Command::Healthcheck { addr } => healthcheck(addr).await,
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
    Ok(Arc::new(Context::new(config, pool, Arc::new(SystemClock))))
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
