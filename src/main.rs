use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use clap::{Parser, Subcommand};
use hilvan::{app, auth, config, db, model, pack, voice};
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
    /// Write the learner model to a file: everything the tutor knows about
    /// the learner, in the format the docs publish a schema for.
    Export {
        /// The model as it stood at this moment instead of as it stands: a
        /// day (YYYY-MM-DD, its end, UTC) or an RFC 3339 date and time.
        #[arg(long)]
        at: Option<String>,
        /// Where to write it; standard output when left out.
        #[arg(long, short)]
        out: Option<PathBuf>,
    },
    /// Replace the learner model with one from a file.
    ///
    /// The model it replaces is written next to the database first, so an
    /// import is never the end of anything. The packs stay; load the ones
    /// the model was learnt on before importing it.
    Import {
        /// A file `hilvan export` wrote.
        path: PathBuf,
    },
    /// Load a fictional learner, three weeks into English, onto a tutor that
    /// holds no learner: for a look around, the docs, a check of a new stand.
    ///
    /// Refuses a tutor that holds a learner already. Load the pack first.
    LoadDemo,
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

    // The log goes to standard error: standard output is what `export`
    // writes the learner model to, and a line of log in it is a broken file.
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("hilvan=info,tower_http=info")))
        .with_writer(std::io::stderr)
        .init();

    let config = config::Config::from_env()?;
    let cli = Cli::parse();

    match cli.command {
        // Running the binary with no arguments serves, which is what a
        // container image or a systemd unit expects.
        None | Some(Command::Serve) => serve(&config).await,
        Some(Command::LoadPack { path }) => load_pack(&config, &path).await,
        Some(Command::PruneAudio { unused_days }) => prune_audio(&config, unused_days).await,
        Some(Command::Export { at, out }) => export(&config, at.as_deref(), out.as_deref()).await,
        Some(Command::Import { path }) => import(&config, &path).await,
        Some(Command::LoadDemo) => load_demo(&config).await,
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
        println!(
            "loaded {} v{}: {} formulas, {} words, {} new cards",
            pack.id, pack.version, loaded.formulas, loaded.words, loaded.new_cards
        );
    } else {
        println!(
            "{} v{} is already loaded: {} formulas, {} words, nothing to do",
            pack.id, pack.version, loaded.formulas, loaded.words
        );
    }

    // A formula or a word that vanished from a pack is usually an editing
    // mistake, and the learner's history for it is worth more than tidiness
    // (see `pack::orphaned_cards`).
    let orphans = pack::orphaned_cards(&pool).await?;
    if !orphans.is_empty() {
        println!(
            "note: {} card(s) belong to formulas or words no pack holds any more, and were kept: {}",
            orphans.len(),
            orphans.join(", ")
        );
    }
    Ok(())
}

/// A moment as the command line takes it: a day, meaning its end in UTC -
/// "how I knew it on 1 March" is the evening of the first - or an RFC 3339
/// date and time.
fn moment(text: &str) -> Result<DateTime<Utc>> {
    if let Ok(at) = DateTime::parse_from_rfc3339(text) {
        return Ok(at.with_timezone(&Utc));
    }
    let day = NaiveDate::parse_from_str(text, "%Y-%m-%d").with_context(|| format!("{text:?} is neither a day (YYYY-MM-DD) nor an RFC 3339 moment"))?;
    day.and_hms_milli_opt(23, 59, 59, 999)
        .map(|end| end.and_utc())
        .with_context(|| format!("{text:?} has no end"))
}

async fn export(config: &config::Config, at: Option<&str>, out: Option<&std::path::Path>) -> Result<()> {
    let pool = db::connect(&config.database_url).await?;
    let now = Utc::now();
    let model = match at.map(moment).transpose()? {
        Some(at) => model::as_of(&pool, at.min(now), now).await?,
        None => model::export(&pool, now).await?,
    };
    let text = serde_json::to_string_pretty(&model).context("failed to write the learner model")? + "\n";
    let Some(path) = out else {
        use std::io::Write as _;
        // A reader that stops early - `| head` - closes the pipe: it has what
        // it wanted, and that is not a failure of the export.
        if let Err(error) = std::io::stdout().lock().write_all(text.as_bytes())
            && error.kind() != std::io::ErrorKind::BrokenPipe
        {
            return Err(error).context("failed to write the learner model");
        }
        return Ok(());
    };
    std::fs::write(path, text).with_context(|| format!("failed to write {}", path.display()))?;
    eprintln!(
        "wrote the learner model as of {} to {}: {} cards, {} own words, {} inventory answers",
        model.as_of.format("%Y-%m-%d %H:%M UTC"),
        path.display(),
        model.cards.len(),
        model.own_words.len(),
        model.inventory.len()
    );
    Ok(())
}

async fn import(config: &config::Config, path: &std::path::Path) -> Result<()> {
    let text = std::fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let incoming = model::Model::parse(&text).with_context(|| format!("{} is not a learner model hilvan can read", path.display()))?;
    let pool = db::connect(&config.database_url).await?;
    let now = Utc::now();

    // The model being replaced goes to a file first, beside the database:
    // in the data volume on a stand, where a backup already looks.
    if model::holds_a_learner(&pool).await? {
        let before = model::export(&pool, now).await?;
        let aside = beside_the_database(
            &config.database_url,
            &format!("hilvan-model-before-import-{}.json", now.format("%Y%m%dT%H%M%SZ")),
        )?;
        std::fs::write(&aside, serde_json::to_string_pretty(&before)? + "\n")
            .with_context(|| format!("failed to keep the model being replaced in {}", aside.display()))?;
        println!("the learner model this replaces is kept in {}", aside.display());
    }

    let imported = model::import(&pool, &incoming, now).await?;
    println!(
        "imported the learner model as of {}: {} cards with {} answers, {} own words, {} inventory answers",
        incoming.as_of.format("%Y-%m-%d %H:%M UTC"),
        imported.cards,
        imported.reviews,
        incoming.own_words.len(),
        incoming.inventory.len()
    );
    report(&imported);
    Ok(())
}

async fn load_demo(config: &config::Config) -> Result<()> {
    let pool = db::connect(&config.database_url).await?;
    let imported = model::load_demo(&pool, Utc::now()).await?;
    println!(
        "loaded the demo learner: three weeks into English, last studied yesterday evening - {} cards with {} answers",
        imported.cards, imported.reviews
    );
    report(&imported);
    Ok(())
}

/// What an import could not carry over, said rather than lost quietly.
fn report(imported: &model::Imported) {
    if imported.anchors_dropped > 0 {
        println!(
            "note: {} word(s) were heard in sentences no loaded pack shows; each is heard in the first sentence it is met in until it is answered again",
            imported.anchors_dropped
        );
    }
    if !imported.packs_missing.is_empty() {
        println!(
            "note: the model was learnt on {}, not loaded here; cards of formulas and words this stand does not have are kept and wait for the pack",
            imported.packs_missing.join(", ")
        );
    }
}

/// A file in the directory the database lives in.
fn beside_the_database(url: &str, name: &str) -> Result<PathBuf> {
    use std::str::FromStr as _;
    let options = sqlx::sqlite::SqliteConnectOptions::from_str(url).with_context(|| format!("HILVAN_DATABASE_URL is not a valid SQLite URL: {url}"))?;
    let directory = options.get_filename().parent().map(std::path::Path::to_path_buf).unwrap_or_default();
    Ok(directory.join(name))
}

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        tracing::error!(%error, "failed to install the shutdown signal handler");
        return;
    }
    tracing::info!("shutting down");
}
