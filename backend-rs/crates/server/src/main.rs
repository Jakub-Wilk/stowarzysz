use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, bail};
use app::config::Config;
use clap::{Parser, Subcommand};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[derive(Debug, Parser)]
#[command(name = "stowarzysz", version, about = "The stowarzysz backend")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Serve the HTTP API (the default).
    Serve,
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

async fn serve() -> anyhow::Result<()> {
    let config = Config::from_env()?;
    server::telemetry::init(config.log_format);
    tracing::info!(
        env = ?config.env,
        bind = %config.bind_addr,
        push = config.vapid.is_some(),
        gemini = config.gemini.api_key.is_some(),
        vertex = config.vertex.is_some(),
        "starting stowarzysz",
    );

    let listener = TcpListener::bind(config.bind_addr)
        .await
        .with_context(|| format!("cannot bind {}", config.bind_addr))?;
    let app = server::http::router(server::AppState::new(config));
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(server::shutdown::signal())
    .await
    .context("server error")?;
    tracing::info!("stopped");
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
