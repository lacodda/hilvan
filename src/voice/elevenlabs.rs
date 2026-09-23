//! `ElevenLabs`: the native speaker the language being learnt is heard from.
//!
//! `POST /v1/text-to-speech/{voice}` with `eleven_flash_v2_5` - the cheapest
//! model that takes a language code, at half a credit a character - and an
//! MP3 back. See `Исследования/2026-09-02 — ElevenLabs API` in the project hub
//! for the model table and the budget on the Starter plan.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;

use super::{Engine, Speaker, Speech, Tempo, Voice};

/// Where the API lives. A field rather than a constant so the tests can
/// point the client at a stand-in.
pub const API: &str = "https://api.elevenlabs.io";

/// The model every sentence is made with.
pub const MODEL: &str = "eleven_flash_v2_5";

/// What the flash model charges per character, used when the answer does not
/// say what the sentence cost.
const CREDITS_PER_CHAR: f64 = 0.5;

/// MP3 at 44.1 kHz and 64 kbps: clear speech at half the size of the default.
const OUTPUT_FORMAT: &str = "mp3_44100_64";

/// Bytes per millisecond of a 64 kbps stream - what the length of a sentence
/// is read from, since a constant-rate MP3 carries no length of its own.
const BYTES_PER_MS: i64 = 8;

/// `voice_settings.speed` for each tempo; the API takes 0.7 to 1.2.
const SLOW_SPEED: f64 = 0.8;
const NORMAL_SPEED: f64 = 1.0;

const TIMEOUT: Duration = Duration::from_secs(30);

/// An `ElevenLabs` account.
#[derive(Clone)]
pub struct ElevenLabs {
    base: String,
    key: String,
    http: reqwest::Client,
}

impl std::fmt::Debug for ElevenLabs {
    // Written by hand so a debug print never carries the key into a log.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ElevenLabs").field("base", &self.base).finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
struct VoiceList {
    voices: Vec<Listed>,
}

#[derive(Deserialize)]
struct Listed {
    voice_id: String,
    name: String,
}

/// What `GET /v1/user/subscription` says, as far as hilvan reads it.
#[derive(Debug, Clone, Deserialize)]
pub struct Subscription {
    /// Credits used this period.
    pub character_count: i64,
    /// Credits the period holds.
    pub character_limit: i64,
    /// When the period starts over, as a Unix time.
    pub next_character_count_reset_unix: Option<i64>,
}

impl Subscription {
    /// When the period starts over.
    #[must_use]
    pub fn resets_at(&self) -> Option<DateTime<Utc>> {
        self.next_character_count_reset_unix.and_then(|unix| DateTime::from_timestamp(unix, 0))
    }
}

impl ElevenLabs {
    /// A client for the account the key belongs to.
    ///
    /// # Errors
    ///
    /// Fails when the HTTP client cannot be built.
    pub fn new(key: &str, base: &str) -> Result<Self> {
        Ok(Self {
            base: base.trim_end_matches('/').to_string(),
            key: key.to_string(),
            http: reqwest::Client::builder()
                .timeout(TIMEOUT)
                .build()
                .context("failed to build the ElevenLabs client")?,
        })
    }

    /// Credits used and left this period.
    ///
    /// # Errors
    ///
    /// Fails when the API does not answer or refuses the key.
    pub async fn subscription(&self) -> Result<Subscription> {
        let response = self
            .http
            .get(format!("{}/v1/user/subscription", self.base))
            .header("xi-api-key", &self.key)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .context("ElevenLabs did not answer")?;
        let status = response.status();
        if !status.is_success() {
            bail!("ElevenLabs answered {status} to a question about the subscription");
        }
        response
            .json()
            .await
            .context("ElevenLabs described the subscription in a shape hilvan does not read")
    }
}

impl Speaker for ElevenLabs {
    fn engine(&self) -> Engine {
        Engine::ElevenLabs
    }

    fn model(&self) -> &'static str {
        MODEL
    }

    fn settings(&self, tempo: Tempo) -> serde_json::Value {
        json!({ "speed": match tempo {
            Tempo::Slow => SLOW_SPEED,
            Tempo::Normal => NORMAL_SPEED,
        } })
    }

    async fn voices(&self) -> Result<Vec<Voice>> {
        let response = self
            .http
            .get(format!("{}/v1/voices", self.base))
            .header("xi-api-key", &self.key)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .context("ElevenLabs did not answer")?;
        let status = response.status();
        if !status.is_success() {
            bail!("ElevenLabs answered {status} to a list of voices");
        }
        let list: VoiceList = response.json().await.context("ElevenLabs listed its voices in a shape hilvan does not read")?;
        Ok(list
            .voices
            .into_iter()
            .map(|listed| Voice {
                engine: Engine::ElevenLabs,
                id: listed.voice_id,
                name: listed.name,
                // Every voice speaks every language of a multilingual model.
                languages: Vec::new(),
                preferred: false,
            })
            .collect())
    }

    async fn speak(&self, voice: &str, text: &str, language: &str, tempo: Tempo) -> Result<Speech> {
        let response = self
            .http
            .post(format!("{}/v1/text-to-speech/{voice}", self.base))
            .query(&[("output_format", OUTPUT_FORMAT)])
            .header("xi-api-key", &self.key)
            .json(&json!({
                "text": text,
                "model_id": MODEL,
                "language_code": language,
                "voice_settings": self.settings(tempo),
            }))
            .send()
            .await
            .context("ElevenLabs did not answer")?;
        let status = response.status();
        if !status.is_success() {
            // The body says why - an exhausted quota, a voice that is gone -
            // and it is short; the key is never in it.
            let why = response.text().await.unwrap_or_default();
            bail!("ElevenLabs answered {status}: {}", why.chars().take(300).collect::<String>());
        }
        // The answer says what the sentence cost when it can; the rate is the
        // fallback, and it is what the plan charges.
        let charged = response
            .headers()
            .get("character-cost")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<i64>().ok());
        let bytes = response.bytes().await.context("ElevenLabs' answer broke off")?.to_vec();
        let chars = f64::from(u32::try_from(text.chars().count()).unwrap_or(u32::MAX));
        #[allow(clippy::cast_possible_truncation)]
        let estimated = (chars * CREDITS_PER_CHAR).ceil() as i64;
        Ok(Speech {
            duration_ms: i64::try_from(bytes.len()).unwrap_or(i64::MAX) / BYTES_PER_MS,
            bytes,
            mime: "audio/mpeg".into(),
            credits: charged.unwrap_or(estimated),
        })
    }
}
