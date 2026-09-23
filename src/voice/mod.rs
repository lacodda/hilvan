//! The voice: who says a sentence, and the sound of it.
//!
//! Two engines behind one trait (owner's decision, 2026-09-11): the learner's
//! own language - the teacher, translations, prompts - is spoken by Piper, a
//! local service on the stand; the language being learnt is spoken by
//! `ElevenLabs`, because a native speaker is what pronunciation is learnt from,
//! and its budget goes on nothing else. The learner never sees which engine
//! is talking, only a voice with a name.
//!
//! Every sound is made once: [`cache`] keeps it in the database under a hash
//! of everything that changes it.

pub mod budget;
pub mod cache;
pub mod elevenlabs;
pub mod material;
pub mod piper;

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::sync::Mutex;

pub use elevenlabs::ElevenLabs;
pub use piper::Piper;

/// How long a list of voices is trusted before it is asked for again.
///
/// Voices change when someone installs one, not between two turns of a
/// drill, and asking an engine on every sentence would put a round trip in
/// front of every sound.
const VOICES_TTL: Duration = Duration::from_secs(600);

/// Which engine makes a voice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    Piper,
    #[serde(rename = "elevenlabs")]
    ElevenLabs,
}

impl Engine {
    /// The word stored in the database.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Piper => "piper",
            Self::ElevenLabs => "elevenlabs",
        }
    }

    /// Reads an engine back from the database.
    ///
    /// # Errors
    ///
    /// Fails on a word that names no engine.
    pub fn parse(word: &str) -> Result<Self> {
        match word {
            "piper" => Ok(Self::Piper),
            "elevenlabs" => Ok(Self::ElevenLabs),
            other => anyhow::bail!("{other:?} is not a voice engine"),
        }
    }
}

/// How fast a sentence is said.
///
/// Two speeds, chosen by where the learner stands with the sentence: new
/// material slowly, so every sound is heard; anything already basted at the
/// speed people talk, because that is the speed it will be heard at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tempo {
    Slow,
    #[default]
    Normal,
}

/// A voice the learner can choose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Voice {
    pub engine: Engine,
    /// What the engine calls it.
    pub id: String,
    /// What the learner sees.
    pub name: String,
    /// The languages it speaks, ISO 639-1. Empty for a voice that speaks any
    /// language its model does - every `ElevenLabs` voice on a multilingual
    /// model.
    pub languages: Vec<String>,
}

impl Voice {
    fn speaks(&self, language: &str) -> bool {
        self.languages.is_empty() || self.languages.iter().any(|spoken| spoken == language)
    }
}

/// A piece of speech, as an engine made it.
#[derive(Debug, Clone)]
pub struct Speech {
    pub bytes: Vec<u8>,
    pub mime: String,
    /// How much speech this is.
    pub duration_ms: i64,
    /// What making it cost; 0 for a local engine.
    pub credits: i64,
}

/// One engine: a list of voices and a way to make them talk.
pub trait Speaker: Send + Sync {
    /// Which engine this is.
    fn engine(&self) -> Engine;

    /// The model the sound comes from, part of what a cached sound is keyed on.
    fn model(&self) -> &str;

    /// The settings a tempo is sent as, exactly as the engine receives them:
    /// a different tempo is a different sound, and the cache has to know.
    fn settings(&self, tempo: Tempo) -> serde_json::Value;

    /// The voices this engine offers now.
    fn voices(&self) -> impl Future<Output = Result<Vec<Voice>>> + Send;

    /// Says one sentence.
    fn speak(&self, voice: &str, text: &str, language: &str, tempo: Tempo) -> impl Future<Output = Result<Speech>> + Send;
}

/// Whether an engine can be used, as health and the voices screen say it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    /// Configured and answering.
    Ok,
    /// Configured and not answering.
    Unreachable,
    /// Not configured: no URL, or no key.
    Off,
}

/// A voice list remembered for a while.
#[derive(Default)]
struct Remembered {
    at: Option<Instant>,
    voices: Vec<Voice>,
}

/// Every engine the stand has, and the voice each language is spoken in.
#[derive(Clone, Default)]
pub struct Voices {
    piper: Option<Arc<Piper>>,
    elevenlabs: Option<Arc<ElevenLabs>>,
    remembered: Arc<Mutex<[Remembered; 2]>>,
    accountant: budget::Accountant,
}

/// The voice a language is spoken in now, and why.
#[derive(Debug, Clone, Serialize)]
pub struct Spoken {
    pub voice: Voice,
    /// True when the learner chose it; false when it is the default for the
    /// language's role.
    pub chosen: bool,
}

/// What went wrong asking for a sentence, in terms the client can act on.
#[derive(Debug, thiserror::Error)]
pub enum SpeakError {
    #[error("{0:?} is not a sentence the material says")]
    NotInMaterial(String),
    #[error("no voice speaks {0}")]
    NoVoice(String),
    #[error("the voice could not be reached: {0:#}")]
    Engine(anyhow::Error),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl Voices {
    /// The engines configured for this stand.
    #[must_use]
    pub fn new(piper: Option<Piper>, elevenlabs: Option<ElevenLabs>) -> Self {
        Self {
            piper: piper.map(Arc::new),
            elevenlabs: elevenlabs.map(Arc::new),
            remembered: Arc::default(),
            accountant: budget::Accountant::default(),
        }
    }

    /// A stand with no voice at all: what the tests of everything else run.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// The `ElevenLabs` client, when there is a key for one.
    #[must_use]
    pub fn elevenlabs(&self) -> Option<&ElevenLabs> {
        self.elevenlabs.as_deref()
    }

    /// The `ElevenLabs` budget, when there is an account to ask. A failure to
    /// ask is logged and reads as no budget, so health and the voices screen
    /// still answer when the account does not.
    pub async fn budget(&self, pool: &SqlitePool) -> Option<budget::Budget> {
        let elevenlabs = self.elevenlabs.as_deref()?;
        match self.accountant.budget(elevenlabs, pool, Utc::now()).await {
            Ok(budget) => Some(budget),
            Err(error) => {
                tracing::warn!(error = format!("{error:#}"), "the ElevenLabs budget could not be read");
                None
            }
        }
    }

    /// Whether each engine is usable right now.
    pub async fn states(&self) -> (State, State) {
        let piper = match &self.piper {
            None => State::Off,
            Some(_) => match self.listed(Engine::Piper).await {
                Ok(_) => State::Ok,
                Err(_) => State::Unreachable,
            },
        };
        let elevenlabs = match &self.elevenlabs {
            None => State::Off,
            Some(_) => match self.listed(Engine::ElevenLabs).await {
                Ok(_) => State::Ok,
                Err(_) => State::Unreachable,
            },
        };
        (piper, elevenlabs)
    }

    /// The voices of one engine, remembered for [`VOICES_TTL`].
    ///
    /// A failure is not remembered: an engine that was down a second ago may
    /// be up now, and the learner should not wait ten minutes to find out.
    async fn listed(&self, engine: Engine) -> Result<Vec<Voice>> {
        let slot = match engine {
            Engine::Piper => 0,
            Engine::ElevenLabs => 1,
        };
        {
            let remembered = self.remembered.lock().await;
            if let Some(at) = remembered[slot].at
                && at.elapsed() < VOICES_TTL
            {
                return Ok(remembered[slot].voices.clone());
            }
        }
        let mut voices = match engine {
            Engine::Piper => match &self.piper {
                Some(piper) => piper.voices().await?,
                None => Vec::new(),
            },
            Engine::ElevenLabs => match &self.elevenlabs {
                Some(elevenlabs) => elevenlabs.voices().await?,
                None => Vec::new(),
            },
        };
        voices.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
        let mut remembered = self.remembered.lock().await;
        remembered[slot] = Remembered {
            at: Some(Instant::now()),
            voices: voices.clone(),
        };
        Ok(voices)
    }

    /// Every voice that may speak a language, the preferred engine first.
    ///
    /// The language being learnt is spoken by `ElevenLabs` first and Piper
    /// after; the learner's own language by Piper only - the paid budget goes
    /// on the language being learnt and on nothing else (owner's decision,
    /// 2026-09-11). An engine that is down contributes nothing rather than
    /// failing the list: the other one may still speak.
    pub async fn options(&self, language: &str, native: bool) -> Vec<Voice> {
        let engines: &[Engine] = if native { &[Engine::Piper] } else { &[Engine::ElevenLabs, Engine::Piper] };
        let mut options = Vec::new();
        for engine in engines {
            match self.listed(*engine).await {
                Ok(voices) => options.extend(voices.into_iter().filter(|voice| voice.speaks(language))),
                Err(error) => tracing::warn!(engine = engine.as_str(), error = format!("{error:#}"), "an engine did not list its voices"),
            }
        }
        options
    }

    /// The voice a language is spoken in: the learner's choice while it is
    /// still offered, otherwise the first option for the language's role.
    ///
    /// # Errors
    ///
    /// Fails when the database rejects a query.
    pub async fn spoken(&self, pool: &SqlitePool, language: &str) -> Result<Option<Spoken>> {
        let native = is_native(pool, language).await?;
        let options = self.options(language, native).await;
        let choice: Option<(String, String)> = sqlx::query_as("SELECT engine, voice FROM voice_choice WHERE language = ?")
            .bind(language)
            .fetch_optional(pool)
            .await
            .context("failed to read the chosen voice")?;

        if let Some((engine, id)) = choice {
            let engine = Engine::parse(&engine)?;
            if let Some(voice) = options.iter().find(|voice| voice.engine == engine && voice.id == id) {
                return Ok(Some(Spoken {
                    voice: voice.clone(),
                    chosen: true,
                }));
            }
            // The choice is kept, not deleted: a key that lapsed for a day
            // should not cost the learner the voice they picked. Until it is
            // back, the default speaks, and the voices screen shows which.
            tracing::info!(
                language,
                engine = engine.as_str(),
                voice = id,
                "the chosen voice is not offered now; the default speaks"
            );
        }
        Ok(options.into_iter().next().map(|voice| Spoken { voice, chosen: false }))
    }

    /// Records the learner's choice of voice for a language.
    ///
    /// # Errors
    ///
    /// Fails when the voice is not one offered for that language - which
    /// includes an `ElevenLabs` voice for the learner's own language - or when
    /// the database rejects the write.
    pub async fn choose(&self, pool: &SqlitePool, language: &str, engine: Engine, id: &str) -> Result<Voice, SpeakError> {
        let native = is_native(pool, language).await?;
        let voice = self
            .options(language, native)
            .await
            .into_iter()
            .find(|voice| voice.engine == engine && voice.id == id)
            .ok_or_else(|| SpeakError::NoVoice(format!("{language} in {} voice {id:?}", engine.as_str())))?;
        sqlx::query(
            "INSERT INTO voice_choice (language, engine, voice) VALUES (?, ?, ?)
             ON CONFLICT (language) DO UPDATE SET engine = excluded.engine, voice = excluded.voice",
        )
        .bind(language)
        .bind(engine.as_str())
        .bind(id)
        .execute(pool)
        .await
        .context("failed to save the chosen voice")?;
        Ok(voice)
    }

    /// Says a sentence of the material in the voice of its language, from
    /// the cache when it has been said before.
    ///
    /// # Errors
    ///
    /// [`SpeakError::NotInMaterial`] for text the material does not hold -
    /// the endpoint is not a free text-to-speech service, and a paid engine
    /// sits behind it. [`SpeakError::NoVoice`] when nothing speaks the
    /// language, [`SpeakError::Engine`] when the engine fails.
    pub async fn say(&self, pool: &SqlitePool, language: &str, text: &str, tempo: Tempo) -> Result<cache::Sound, SpeakError> {
        let text = text.trim();
        if !material::speakable(pool, language, text).await? {
            return Err(SpeakError::NotInMaterial(text.to_string()));
        }
        let spoken = self.spoken(pool, language).await?.ok_or_else(|| SpeakError::NoVoice(language.to_string()))?;
        match spoken.voice.engine {
            Engine::Piper => {
                let piper = self.piper.as_deref().ok_or_else(|| SpeakError::NoVoice(language.to_string()))?;
                speak_cached(pool, piper, &spoken.voice.id, language, text, tempo).await
            }
            Engine::ElevenLabs => {
                let elevenlabs = self.elevenlabs.as_deref().ok_or_else(|| SpeakError::NoVoice(language.to_string()))?;
                speak_cached(pool, elevenlabs, &spoken.voice.id, language, text, tempo).await
            }
        }
    }
}

/// One engine behind the cache: served if it was made before, made and kept
/// otherwise.
async fn speak_cached<S: Speaker>(pool: &SqlitePool, speaker: &S, voice: &str, language: &str, text: &str, tempo: Tempo) -> Result<cache::Sound, SpeakError> {
    let settings = speaker.settings(tempo);
    let key = cache::Key {
        engine: speaker.engine(),
        voice,
        model: speaker.model(),
        settings: &settings,
        language,
        text,
    };
    let now = Utc::now();
    if let Some(sound) = cache::get(pool, &key, now).await? {
        return Ok(sound);
    }
    let speech = speaker.speak(voice, text, language, tempo).await.map_err(SpeakError::Engine)?;
    Ok(cache::put(pool, &key, speech, now).await?)
}

async fn is_native(pool: &SqlitePool, language: &str) -> Result<bool> {
    let native: Option<i64> = sqlx::query_scalar("SELECT is_native FROM language WHERE code = ?")
        .bind(language)
        .fetch_optional(pool)
        .await
        .context("failed to read a language")?;
    Ok(native == Some(1))
}

/// Reads the duration of a WAV file from its header, in milliseconds.
///
/// Piper answers with 16-bit PCM WAV; the length of speech is the size of the
/// `data` chunk over the byte rate in `fmt `. `None` for anything that is not
/// a RIFF WAVE with both chunks.
#[must_use]
pub fn wav_duration_ms(bytes: &[u8]) -> Option<i64> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut at = 12;
    let mut byte_rate: Option<u32> = None;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().ok()?);
        let body = at + 8;
        if id == b"fmt " && body + 12 <= bytes.len() {
            byte_rate = Some(u32::from_le_bytes(bytes[body + 8..body + 12].try_into().ok()?));
        }
        if id == b"data" {
            let rate = byte_rate.filter(|rate| *rate > 0)?;
            // A streamed WAV may leave the size at its maximum: the bytes that
            // are actually there are the truth.
            let data = u64::from(size).min(u64::try_from(bytes.len() - body).ok()?);
            return i64::try_from(data * 1000 / u64::from(rate)).ok();
        }
        at = body + usize::try_from(size).ok()? + usize::try_from(size % 2).ok()?;
    }
    None
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A WAV header and `samples` of 16-bit mono silence at `rate` Hz.
    pub fn wav(rate: u32, samples: u32) -> Vec<u8> {
        let data = samples * 2;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&rate.to_le_bytes());
        bytes.extend_from_slice(&(rate * 2).to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data.to_le_bytes());
        bytes.resize(bytes.len() + data as usize, 0);
        bytes
    }

    #[test]
    fn a_wav_says_how_long_it_is() {
        assert_eq!(wav_duration_ms(&wav(22_050, 22_050 * 3)), Some(3000));
        assert_eq!(wav_duration_ms(&wav(16_000, 8000)), Some(500));
    }

    #[test]
    fn something_that_is_not_a_wav_has_no_duration() {
        assert_eq!(wav_duration_ms(b"ID3 not a wav at all"), None);
        assert_eq!(wav_duration_ms(&[]), None);
    }
}
