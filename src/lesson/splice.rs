//! A lesson's lines and silences, made into one file by ffmpeg.
//!
//! Every line comes out of the audio cache as whatever its engine made - MP3
//! from `ElevenLabs`, WAV from Piper. Each is decoded once into the same raw
//! form (16-bit mono PCM at [`RATE`]), the silences between them are written
//! as zeros, and the whole is encoded once, with a chapter for every time a
//! phrase is heard or asked. Laying the lesson out sample by sample here,
//! rather than in a filter graph of hundreds of inputs, is what makes every
//! chapter start exactly where the plan says it does.

use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;

use anyhow::{Context, Result, bail};
use hilvan_core::lesson::{Line, Part};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

/// The sample rate a lesson is laid out and encoded at: plenty for speech,
/// and MP3 has a mode for it.
pub const RATE: u32 = 24_000;

/// Samples in one millisecond at [`RATE`].
const PER_MS: usize = (RATE / 1_000) as usize;

/// What a lesson is encoded at: speech in mono needs no more, and forty
/// minutes stay under fifteen megabytes for the phone to carry.
const BITRATE: &str = "48k";

/// One line, decoded: its samples, cut to a whole number of milliseconds so
/// that the plan's arithmetic in milliseconds is the file's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    /// 16-bit little-endian samples at [`RATE`], mono.
    pub pcm: Vec<u8>,
}

impl Decoded {
    /// How long it lasts.
    #[must_use]
    pub fn ms(&self) -> i64 {
        i64::try_from(self.pcm.len() / 2 / PER_MS).unwrap_or(i64::MAX)
    }
}

/// Decodes one sound, in whatever format its engine made it, to the raw
/// form a lesson is laid out in.
///
/// # Errors
///
/// Fails when ffmpeg cannot be started or cannot read the sound.
pub async fn decode(ffmpeg: &Path, sound: &[u8]) -> Result<Decoded> {
    let args = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-i",
        "pipe:0",
        "-f",
        "s16le",
        "-acodec",
        "pcm_s16le",
        "-ac",
        "1",
    ];
    let rate = RATE.to_string();
    let mut command = Command::new(ffmpeg);
    command.args(args).args(["-ar", &rate, "pipe:1"]);
    let mut pcm = run(command, sound.to_vec()).await.context("a line could not be decoded")?;
    // Whole milliseconds: what is cut is less than one, the tail of a pause.
    pcm.truncate(pcm.len() / (2 * PER_MS) * (2 * PER_MS));
    Ok(Decoded { pcm })
}

/// A chapter of the file: where it starts and ends, and what it is called.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    pub start_ms: i64,
    pub end_ms: i64,
    pub title: String,
}

/// Encodes a lesson: its parts in order, each line from `lines`, with a
/// chapter for each mark. MP3, its chapters in the ID3 tag, which players
/// skip by.
///
/// # Errors
///
/// Fails when a part names a line that was not decoded, when ffmpeg cannot
/// be started, or when it fails.
pub async fn encode(ffmpeg: &Path, parts: &[Part], lines: &HashMap<Line, Decoded>, marks: &[Mark], title: &str) -> Result<Vec<u8>> {
    for part in parts {
        if let Part::Say { line } = part
            && !lines.contains_key(line)
        {
            bail!("the lesson says {:?}, which was not decoded", line.text);
        }
    }
    let dir = tempfile::tempdir().context("failed to make a place to encode the lesson in")?;
    let metadata = dir.path().join("chapters.txt");
    tokio::fs::write(&metadata, ffmetadata(title, marks))
        .await
        .context("failed to write the lesson's chapters")?;
    let out = dir.path().join("lesson.mp3");

    let rate = RATE.to_string();
    let mut command = Command::new(ffmpeg);
    command
        .args(["-hide_banner", "-loglevel", "error", "-f", "s16le", "-ar", &rate, "-ac", "1", "-i", "pipe:0"])
        .args(["-f", "ffmetadata", "-i"])
        .arg(&metadata)
        .args(["-map", "0:a", "-map_metadata", "1", "-map_chapters", "1"])
        .args(["-c:a", "libmp3lame", "-b:a", BITRATE, "-id3v2_version", "3", "-y"])
        .arg(&out);
    // Written to a file rather than a pipe: the encoder goes back to the
    // start when it is done and records how long the stream is, which is
    // what a player seeks by.
    run(command, layout(parts, lines)).await.context("the lesson could not be encoded")?;
    tokio::fs::read(&out).await.context("failed to read the encoded lesson")
}

/// The whole lesson as raw samples, in order: what the encoder is fed.
fn layout(parts: &[Part], lines: &HashMap<Line, Decoded>) -> Vec<u8> {
    let mut pcm = Vec::new();
    for part in parts {
        match part {
            Part::Say { line } => {
                if let Some(decoded) = lines.get(line) {
                    pcm.extend_from_slice(&decoded.pcm);
                }
            }
            Part::Silence { ms } => {
                let samples = usize::try_from(*ms).unwrap_or(0) * PER_MS;
                pcm.resize(pcm.len() + samples * 2, 0);
            }
        }
    }
    pcm
}

/// The chapters as ffmpeg reads them: `;FFMETADATA1`, then one section a
/// chapter, in milliseconds.
fn ffmetadata(title: &str, marks: &[Mark]) -> String {
    use std::fmt::Write as _;
    let mut out = format!(";FFMETADATA1\ntitle={}\nartist=hilvan\n", escape(title));
    for mark in marks {
        let _ = write!(
            out,
            "[CHAPTER]\nTIMEBASE=1/1000\nSTART={}\nEND={}\ntitle={}\n",
            mark.start_ms,
            mark.end_ms,
            escape(&mark.title)
        );
    }
    out
}

/// A value as ffmetadata takes it: `=`, `;`, `#`, `\` and line ends are
/// escaped with a backslash.
fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(character, '=' | ';' | '#' | '\\' | '\n') {
            out.push('\\');
        }
        out.push(character);
    }
    out
}

/// Runs ffmpeg with `input` on its standard input, and answers what it
/// wrote on its standard output. Input is written while output is read, so
/// neither pipe can fill and stall the other.
async fn run(mut command: Command, input: Vec<u8>) -> Result<Vec<u8>> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("ffmpeg could not be started; is it installed, or HILVAN_FFMPEG set?")?;
    let mut stdin = child.stdin.take().context("ffmpeg took no input")?;
    let mut stdout = child.stdout.take().context("ffmpeg gave no output")?;
    let mut stderr = child.stderr.take().context("ffmpeg gave no errors channel")?;
    let writer = tokio::spawn(async move {
        // ffmpeg may stop reading early when it fails; its own error says
        // why, so a broken pipe here is not the story.
        let _ = stdin.write_all(&input).await;
        drop(stdin);
    });
    let mut output = Vec::new();
    let mut errors = Vec::new();
    let (read, said) = tokio::join!(stdout.read_to_end(&mut output), stderr.read_to_end(&mut errors));
    read.context("ffmpeg's output broke off")?;
    said.context("ffmpeg's errors broke off")?;
    let status = child.wait().await.context("ffmpeg did not finish")?;
    writer.await.context("the input to ffmpeg was not written")?;
    if !status.success() {
        bail!("ffmpeg failed ({status}): {}", String::from_utf8_lossy(&errors).trim());
    }
    Ok(output)
}

/// Whether ffmpeg answers at all: said once at startup, so a stand that
/// cannot make lessons says so before anyone asks for one.
pub async fn available(ffmpeg: &Path) -> bool {
    Command::new(ffmpeg)
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .is_ok_and(|status| status.success())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use hilvan_core::lesson::Voice;

    use super::*;

    /// The ffmpeg the tests run: `HILVAN_FFMPEG`, or the one on the path.
    /// CI installs it; a test that needs it fails without it rather than
    /// passing for nothing.
    fn ffmpeg() -> PathBuf {
        std::env::var_os("HILVAN_FFMPEG").map_or_else(|| PathBuf::from("ffmpeg"), PathBuf::from)
    }

    /// A WAV of a tone, as Piper answers: 16-bit mono at `rate`.
    fn tone(rate: u32, ms: u32) -> Vec<u8> {
        let samples = rate * ms / 1_000;
        let mut wav = crate::voice::tests::wav(rate, samples);
        let data = wav.len() - usize::try_from(samples * 2).unwrap();
        for (n, sample) in wav[data..].chunks_exact_mut(2).enumerate() {
            #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
            let value = ((n as f64 * 440.0 * std::f64::consts::TAU / f64::from(rate)).sin() * 8_000.0) as i16;
            sample.copy_from_slice(&value.to_le_bytes());
        }
        wav
    }

    fn line(text: &str) -> Line {
        Line {
            voice: Voice::Speaker,
            text: text.to_string(),
            slow: false,
        }
    }

    #[test]
    fn a_chapter_title_cannot_break_the_file_it_is_written_in() {
        let written = ffmetadata(
            "A lesson",
            &[Mark {
                start_ms: 0,
                end_ms: 900,
                title: "x = 1; #2 \\ ok\n[CHAPTER]".to_string(),
            }],
        );
        assert!(written.starts_with(";FFMETADATA1\n"));
        assert!(written.contains("title=x \\= 1\\; \\#2 \\\\ ok\\\n[CHAPTER]\n"), "{written}");
        assert_eq!(written.matches("\n[CHAPTER]\n").count(), 1, "a title made a chapter of its own: {written}");
    }

    #[test]
    fn silence_is_laid_out_to_the_sample() {
        let mut lines = HashMap::new();
        lines.insert(line("a"), Decoded { pcm: vec![1; 2 * PER_MS * 10] });
        let parts = [Part::Say { line: line("a") }, Part::Silence { ms: 250 }, Part::Say { line: line("a") }];
        let pcm = layout(&parts, &lines);
        assert_eq!(pcm.len(), 2 * PER_MS * (10 + 250 + 10));
        assert!(pcm[2 * PER_MS * 10..2 * PER_MS * 260].iter().all(|byte| *byte == 0));
    }

    #[tokio::test]
    async fn a_line_decodes_to_whole_milliseconds_at_the_lesson_rate() {
        let decoded = decode(&ffmpeg(), &tone(22_050, 1_234)).await.unwrap();
        assert_eq!(decoded.pcm.len() % (2 * PER_MS), 0);
        // Resampling may move the end by a few samples, never by more.
        assert!((1_230..=1_236).contains(&decoded.ms()), "decoded to {} ms", decoded.ms());
    }

    #[tokio::test]
    async fn a_lesson_is_one_mp3_with_a_chapter_for_every_phrase() {
        let ffmpeg = ffmpeg();
        let mut lines = HashMap::new();
        lines.insert(line("one"), decode(&ffmpeg, &tone(22_050, 800)).await.unwrap());
        lines.insert(line("two"), decode(&ffmpeg, &tone(16_000, 600)).await.unwrap());
        let parts = [Part::Say { line: line("one") }, Part::Silence { ms: 1_000 }, Part::Say { line: line("two") }];
        let marks = [
            Mark {
                start_ms: 0,
                end_ms: 800,
                title: "One phrase.".to_string(),
            },
            Mark {
                start_ms: 1_800,
                end_ms: 2_400,
                title: "Another phrase.".to_string(),
            },
        ];
        let mp3 = encode(&ffmpeg, &parts, &lines, &marks, "A lesson").await.unwrap();
        assert_eq!(&mp3[..3], b"ID3", "the tag comes first");
        let tag = &mp3[..tag_size(&mp3)];
        assert_eq!(count(tag, b"CHAP"), 2, "a chapter for each phrase");
        assert_eq!(count(tag, b"CTOC"), 1, "and a table of them");
        assert_eq!(count(tag, "Another phrase.".as_bytes()), 1, "chapters are named by their phrase");

        // Decoded again, it lasts as long as the plan says.
        let back = decode(&ffmpeg, &mp3).await.unwrap();
        assert!((back.ms() - 2_400).abs() <= 60, "the lesson came back {} ms long", back.ms());
    }

    #[tokio::test]
    async fn a_line_never_decoded_is_refused_before_ffmpeg_is_asked() {
        let parts = [Part::Say { line: line("missing") }];
        let error = encode(Path::new("no-such-ffmpeg"), &parts, &HashMap::new(), &[], "A lesson").await.unwrap_err();
        assert!(error.to_string().contains("missing"), "{error:#}");
    }

    /// The size of an ID3v2 tag with its header: four 7-bit bytes.
    fn tag_size(file: &[u8]) -> usize {
        10 + file[6..10].iter().fold(0usize, |size, byte| (size << 7) | usize::from(*byte & 0x7f))
    }

    fn count(haystack: &[u8], needle: &[u8]) -> usize {
        haystack.windows(needle.len()).filter(|window| *window == needle).count()
    }
}
