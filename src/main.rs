use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use hilvan::{app, config, db};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

/// A self-hosted language tutor: grammar formulas, spaced repetition and
/// audio lessons compiled from what you already know.
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run the HTTP server: the API and the tutor app.
    Serve,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Loaded before the logger so RUST_LOG can live in .env too. A missing
    // file is normal: in production the environment is set by the deployment.
    match dotenvy::dotenv() {
        Ok(_) | Err(dotenvy::Error::Io(_)) => {}
        Err(error) => return Err(error).context("failed to read .env"),
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("hilvan=info,tower_http=info")))
        .init();

    let config = config::Config::from_env()?;
    let cli = Cli::parse();

    match cli.command {
        // Running the binary with no arguments serves, which is what a
        // container image or a systemd unit expects.
        None | Some(Command::Serve) => serve(&config).await,
    }
}

async fn serve(config: &config::Config) -> Result<()> {
    let pool = db::connect(&config.database_url).await?;

    let listener = TcpListener::bind(config.addr)
        .await
        .with_context(|| format!("failed to bind {}", config.addr))?;
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        addr = %config.addr,
        "hilvan listening"
    );

    axum::serve(listener, app::router(pool, &config.web_dir))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server error")?;
    Ok(())
}

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::error!(%error, "failed to install the shutdown signal handler");
        return;
    }
    tracing::info!("shutting down");
}
