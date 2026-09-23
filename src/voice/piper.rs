//! Piper: the local voice, running as a service beside the tutor.
//!
//! Piper's HTTP server takes `POST /synthesize` with the text and a voice
//! name and answers with a WAV; `GET /voices` lists the voices it holds.
//! It speaks the learner's own language - the teacher - and costs nothing
//! but a second or two of the Pi's time per sentence.

use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::json;

use super::{Engine, Speaker, Speech, Tempo, Voice, wav_duration_ms};

/// How long a sentence may take. A cold voice loads in about six seconds on
/// a Pi 4 and a long explanation takes a few more; past this the service is
/// stuck rather than slow.
const TIMEOUT: Duration = Duration::from_secs(60);

/// Phoneme length for each tempo: above 1 is slower.
const SLOW_LENGTH_SCALE: f64 = 1.35;
const NORMAL_LENGTH_SCALE: f64 = 1.0;

/// A Piper service.
#[derive(Debug, Clone)]
pub struct Piper {
    url: String,
    http: reqwest::Client,
}

/// One voice as `GET /voices` describes it: only what hilvan reads.
#[derive(Deserialize)]
struct Described {
    language: Option<Language>,
}

/// What `GET /info` says, as far as hilvan reads it: the voice the service
/// loaded at start.
#[derive(Deserialize)]
struct Info {
    voice: InfoVoice,
}

#[derive(Deserialize)]
struct InfoVoice {
    name: String,
}

#[derive(Deserialize)]
struct Language {
    /// `ru_RU`, `en_US`.
    code: Option<String>,
    family: Option<String>,
}

impl Piper {
    /// A client for the service at `url`, e.g. `http://piper:5000`.
    ///
    /// # Errors
    ///
    /// Fails when the HTTP client cannot be built.
    pub fn new(url: &str) -> Result<Self> {
        Ok(Self {
            url: url.trim_end_matches('/').to_string(),
            http: reqwest::Client::builder()
                .timeout(TIMEOUT)
                .build()
                .context("failed to build the Piper client")?,
        })
    }
}

/// The name a learner sees: `ru_RU-irina-medium` is "Irina".
fn display_name(id: &str) -> String {
    let speaker = id.split('-').nth(1).unwrap_or(id);
    let mut chars = speaker.chars();
    chars.next().map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
}

impl Piper {
    /// The voice the service loaded at start: its operator's pick, and the
    /// default for a language nobody has chosen a voice for. `None` when the
    /// service does not say - the voices are then in name order.
    async fn default_voice(&self) -> Option<String> {
        let response = self.http.get(format!("{}/info", self.url)).timeout(Duration::from_secs(5)).send().await.ok()?;
        let info: Info = response.error_for_status().ok()?.json().await.ok()?;
        // The service names it the way it trims the file name, which can
        // leave a stray character; the list of voices is the authority.
        Some(info.voice.name)
    }
}

impl Speaker for Piper {
    fn engine(&self) -> Engine {
        Engine::Piper
    }

    fn model(&self) -> &'static str {
        // Each Piper voice is its own model, and the voice is already in the
        // key; the engine version is what would change the sound under it.
        "piper-1"
    }

    fn settings(&self, tempo: Tempo) -> serde_json::Value {
        json!({ "length_scale": match tempo {
            Tempo::Slow => SLOW_LENGTH_SCALE,
            Tempo::Normal => NORMAL_LENGTH_SCALE,
        } })
    }

    async fn voices(&self) -> Result<Vec<Voice>> {
        let response = self
            .http
            .get(format!("{}/voices", self.url))
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .with_context(|| format!("Piper at {} did not answer", self.url))?;
        if !response.status().is_success() {
            bail!("Piper at {} answered {} to a list of voices", self.url, response.status());
        }
        let described: BTreeMap<String, Described> = response.json().await.context("Piper listed its voices in a shape hilvan does not read")?;
        let default = self.default_voice().await;
        Ok(described
            .into_iter()
            .map(|(id, described)| {
                // `family` is the ISO 639-1 code; `code` is the locale it
                // starts with. Either will do, the first is exact.
                let language = described
                    .language
                    .and_then(|language| {
                        language
                            .family
                            .or_else(|| language.code.map(|code| code.split('_').next().unwrap_or_default().to_string()))
                    })
                    .unwrap_or_default();
                Voice {
                    engine: Engine::Piper,
                    name: display_name(&id),
                    preferred: default.as_deref() == Some(id.as_str()),
                    id,
                    languages: vec![language],
                }
            })
            .collect())
    }

    async fn speak(&self, voice: &str, text: &str, _language: &str, tempo: Tempo) -> Result<Speech> {
        // Piper answers a voice it does not have with its default voice and a
        // warning in its own log: asking for Dmitri and hearing Irina, with
        // nothing to say why. So the voice is checked here first.
        let voices = self.voices().await?;
        if !voices.iter().any(|known| known.id == voice) {
            bail!("Piper at {} has no voice {voice:?}", self.url);
        }

        let mut body = self.settings(tempo);
        body["text"] = json!(text);
        body["voice"] = json!(voice);
        let response = self
            .http
            .post(format!("{}/synthesize", self.url))
            .json(&body)
            .send()
            .await
            .with_context(|| format!("Piper at {} did not answer", self.url))?;
        let status = response.status();
        if !status.is_success() {
            bail!("Piper at {} answered {status} to a sentence", self.url);
        }
        let bytes = response.bytes().await.context("Piper's answer broke off")?.to_vec();
        let duration_ms = wav_duration_ms(&bytes).context("Piper answered with something that is not a WAV")?;
        Ok(Speech {
            bytes,
            mime: "audio/wav".into(),
            duration_ms,
            credits: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_voice_is_named_after_its_speaker() {
        assert_eq!(display_name("ru_RU-irina-medium"), "Irina");
        assert_eq!(display_name("en_US-lessac-medium"), "Lessac");
        assert_eq!(display_name("plain"), "Plain");
    }

    #[test]
    fn a_slow_tempo_is_a_longer_phoneme() {
        let piper = Piper::new("http://piper:5000/").unwrap();
        assert!(piper.settings(Tempo::Slow)["length_scale"].as_f64() > piper.settings(Tempo::Normal)["length_scale"].as_f64());
        assert_eq!(piper.url, "http://piper:5000", "a trailing slash would double up in every path");
    }
}
