use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use hilvan::{app, auth, config, db, pack, voice};
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
    /// Hash a password for `HILVAN_PASSWORD_HASH`.
    ///
    /// Without a hash to put in the variable, locking a stand means finding
    /// an Argon2 tool elsewhere, and most of what turns up online is a web
    /// form asking for the password.
    Hash {
        /// The password to hash. Prompted for if omitted.
        password: Option<String>,
    },
    /// Load a formula pack into the database, or confirm it is already there.
    ///
    /// Safe to run on every start: an unchanged pack is a no-op, and a
    /// changed one replaces its formulas without touching what the learner
    /// has learnt about them.
    LoadPack {
        /// Path to the pack file, e.g. packs/en-from-ru/pack.toml.
        path: PathBuf,
    },
    /// Drop sounds nobody has asked for in a while.
    ///
    /// Explicit rather than automatic: a paid sound dropped is a sound paid
    /// for again, and that is the learner's call, not a timer's.
    PruneAudio {
        /// Sounds unused for this many days go.
        #[arg(long, default_value_t = 90)]
        unused_days: i64,
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
        Some(Command::PruneAudio { unused_days }) => prune_audio(&config, unused_days).await,
        Some(Command::Hash { password }) => {
            let password = match password {
                Some(password) => password,
                None => rpassword::prompt_password("Password: ").context("failed to read the password")?,
            };
            anyhow::ensure!(!password.trim().is_empty(), "an empty password is not a password");
            println!("{}", auth::hash(&password)?);
            Ok(())
        }
    }
}

async fn serve(config: &config::Config) -> Result<()> {
    let pool = db::connect(&config.database_url).await?;
    if config.password_hash.is_none() {
        // Said once, loudly: a stand nobody has to sign in to is a choice,
        // and it should never be one made by forgetting a variable.
        tracing::warn!("HILVAN_PASSWORD_HASH is not set: anyone who can reach this server can use the tutor");
    }

    let listener = TcpListener::bind(config.addr)
        .await
        .with_context(|| format!("failed to bind {}", config.addr))?;
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        addr = %config.addr,
        "hilvan listening"
    );

    let voices = voices(config)?;
    axum::serve(listener, app::router(pool, config.password_hash.clone(), &config.web_dir, voices))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server error")?;
    Ok(())
}

/// The engines this stand speaks with, said once at startup so a missing
/// voice is a line in the log rather than a silent drill.
fn voices(config: &config::Config) -> Result<voice::Voices> {
    let piper = config.piper_url.as_deref().map(voice::Piper::new).transpose()?;
    let elevenlabs = config
        .elevenlabs_key
        .as_deref()
        .map(|key| voice::ElevenLabs::new(key, voice::elevenlabs::API))
        .transpose()?;
    match (&piper, &elevenlabs) {
        (None, None) => tracing::warn!("no voice is configured: set HILVAN_PIPER_URL and, for the language being learnt, HILVAN_ELEVENLABS_KEY"),
        (None, Some(_)) => tracing::warn!("HILVAN_PIPER_URL is not set: the learner's own language has no voice"),
        (Some(_), None) => tracing::info!("HILVAN_ELEVENLABS_KEY is not set: Piper speaks every language"),
        (Some(_), Some(_)) => {}
    }
    Ok(voice::Voices::new(piper, elevenlabs))
}

async fn prune_audio(config: &config::Config, unused_days: i64) -> Result<()> {
    anyhow::ensure!(unused_days > 0, "--unused-days has to be at least one day");
    let pool = db::connect(&config.database_url).await?;
    let pruned = voice::cache::prune(&pool, chrono::Duration::days(unused_days), chrono::Utc::now()).await?;
    println!(
        "pruned {} sound(s) unused for {unused_days} days, {} KiB freed",
        pruned.sounds,
        pruned.bytes / 1024
    );
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
