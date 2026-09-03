use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use hilvan::{app, auth, config, db, pack};
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
    /// Load a formula pack into the database, or confirm it is already there.
    ///
    /// Safe to run on every start: an unchanged pack is a no-op, and a
    /// changed one replaces its formulas without touching what the learner
    /// has learnt about them.
    LoadPack {
        /// Path to the pack file, e.g. packs/en-from-ru/pack.toml.
        path: PathBuf,
    },
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
        Some(Command::LoadPack { path }) => load_pack(&config, &path).await,
    }
}

async fn serve(config: &config::Config) -> Result<()> {
    let pool = db::connect(&config.database_url).await?;
    let sessions = auth::Sessions::new(config.password.clone());
    if !sessions.is_required() {
        // Said once, loudly: a stand nobody has to sign in to is a choice,
        // and it should never be one made by forgetting a variable.
        tracing::warn!("HILVAN_PASSWORD is not set: anyone who can reach this server can use the tutor");
    }

    let listener = TcpListener::bind(config.addr)
        .await
        .with_context(|| format!("failed to bind {}", config.addr))?;
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        addr = %config.addr,
        "hilvan listening"
    );

    axum::serve(listener, app::router(pool, sessions, &config.web_dir))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server error")?;
    Ok(())
}

/// Loads a pack and says what it did.
///
/// A separate command rather than something the server does at startup: the
/// learner decides when their material changes, and a server that rewrites
/// the pack on every restart makes an editing mistake invisible.
async fn load_pack(config: &config::Config, path: &PathBuf) -> Result<()> {
    let pack = pack::Pack::read(path)?;
    let pool = db::connect(&config.database_url).await?;
    let loaded = pack::load(&pool, &pack).await?;

    if loaded.changed {
        println!("loaded {} v{}: {} formulas, {} new", pack.id, pack.version, loaded.formulas, loaded.new_cards);
    } else {
        println!("{} v{} is already loaded: {} formulas, nothing to do", pack.id, pack.version, loaded.formulas);
    }

    // A formula that vanished from a pack is usually an editing mistake, and
    // the learner's history for it is worth more than tidiness (see
    // `pack::orphaned_cards`).
    let orphans = pack::orphaned_cards(&pool).await?;
    if !orphans.is_empty() {
        println!(
            "note: {} card(s) belong to formulas no pack holds any more, and were kept: {}",
            orphans.len(),
            orphans.join(", ")
        );
    }
    Ok(())
}

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::error!(%error, "failed to install the shutdown signal handler");
        return;
    }
    tracing::info!("shutting down");
}
