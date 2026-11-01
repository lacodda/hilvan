//! The words taken from texts as a file Anki imports: a note a word, the
//! sentence it was taken from on the front with the word in bold, the word,
//! its sound and its meaning on the back.
//!
//! Anki reads plain text with a header that says how (Anki 2.1.55 and
//! later, `AnkiDroid` 2.17): tab-separated, HTML in the fields, the note type,
//! the deck, and a column of ids so a file imported again updates its notes
//! instead of adding them twice. The id is the word and the sentence's text,
//! which outlive a move to another stand.

use std::fmt::Write as _;

use anyhow::{Context as _, Result};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::words;

/// One word taken from a text, with what the note says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub id: String,
    /// The sentence, the word in it in bold.
    pub front: String,
    /// The word, its sound, its meaning, and where it was read.
    pub back: String,
}

/// Text made safe to stand in a field of HTML on one line.
fn escaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' | '\n' | '\r' => out.push(' '),
            ch => out.push(ch),
        }
    }
    out
}

/// A sentence with the stretch from `start` - in characters - `length` long
/// in bold. A stretch that does not fit leaves the sentence plain.
fn marked(sentence: &str, start: usize, length: usize) -> String {
    let chars: Vec<char> = sentence.chars().collect();
    if start + length > chars.len() || length == 0 {
        return escaped(sentence);
    }
    let part = |range: std::ops::Range<usize>| escaped(&chars[range].iter().collect::<String>());
    format!("{}<b>{}</b>{}", part(0..start), part(start..start + length), part(start + length..chars.len()))
}

/// The id of a note: the word and the sentence it was taken from.
fn note_id(word: &str, sentence: &str) -> String {
    let hash = Sha256::digest(sentence.as_bytes());
    let short = hash.iter().take(6).fold(String::new(), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    });
    format!("hilvan:{word}:{short}")
}

/// Every word taken from a text, the earliest first, as notes.
///
/// # Errors
///
/// Fails when the database rejects a query.
pub async fn notes(pool: &SqlitePool) -> Result<Vec<Note>> {
    let picks: Vec<(String, i64, String, String, i64)> = sqlx::query_as(
        "SELECT p.word_id, s.id, s.text, p.form, p.start FROM pick p JOIN sentence s ON s.id = p.sentence_id
         ORDER BY p.picked_at, p.word_id, s.id",
    )
    .fetch_all(pool)
    .await
    .context("failed to read the words taken from texts")?;
    let mut notes = Vec::with_capacity(picks.len());
    for (id, sentence_id, sentence, form, start) in picks {
        let word = words::word(pool, &id).await?;
        // The text it was read in, and the chapter of a book, while the
        // text is still there.
        let read: Option<(String, Option<String>, i64)> = sqlx::query_as(
            "SELECT t.title, c.title, (SELECT count(*) FROM chapter WHERE text_id = t.id)
             FROM text_sentence ts JOIN text t ON t.id = ts.text_id
             JOIN chapter c ON c.text_id = t.id
                AND c.paragraph = (SELECT max(paragraph) FROM chapter WHERE text_id = t.id AND paragraph <= ts.paragraph)
             WHERE ts.sentence_id = ? ORDER BY t.id LIMIT 1",
        )
        .bind(sentence_id)
        .fetch_optional(pool)
        .await
        .context("failed to read where a word was taken from")?;
        let start = usize::try_from(start).unwrap_or(usize::MAX);
        let front = marked(&sentence, start, form.chars().count());
        let mut back = format!("<b>{}</b>", escaped(&word.lemma));
        if let Some(ipa) = &word.ipa {
            let _ = write!(back, " /{}/", escaped(ipa));
        }
        if let Some(gloss) = &word.gloss {
            let _ = write!(back, " - {}", escaped(gloss));
        }
        if let Some((title, chapter, chapters)) = read {
            let place = match chapter.filter(|_| chapters > 1) {
                Some(chapter) => format!("{title} · {chapter}"),
                None => title,
            };
            let _ = write!(back, "<br><i>{}</i>", escaped(&place));
        }
        notes.push(Note {
            id: note_id(&id, &sentence),
            front,
            back,
        });
    }
    Ok(notes)
}

/// The notes as a file Anki imports.
#[must_use]
pub fn file(notes: &[Note]) -> String {
    let mut text = String::from("#separator:tab\n#html:true\n#notetype:Basic\n#deck:hilvan\n#guid column:1\n#tags column:4\n");
    for note in notes {
        let _ = writeln!(text, "{}\t{}\t{}\thilvan", escaped(&note.id), note.front, note.back);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reading::tests::{at, book, plain, tutor};
    use crate::reading::{self, Library};

    #[test]
    fn the_word_is_bold_in_its_sentence_and_nothing_breaks_a_field() {
        assert_eq!(marked("She went <home>.", 4, 4), "She <b>went</b> &lt;home&gt;.");
        assert_eq!(marked("Écoute, il est là.", 8, 2), "Écoute, <b>il</b> est là.", "characters, not bytes");
        assert_eq!(marked("Short.", 4, 9), "Short.", "a stretch past the end leaves it plain");
        assert_eq!(escaped("a\tb\nc & d"), "a b c &amp; d");
    }

    #[test]
    fn a_word_taken_from_two_sentences_is_two_notes() {
        assert_ne!(note_id("en:late", "It was late."), note_id("en:late", "Too late."));
        assert_eq!(note_id("en:late", "It was late."), note_id("en:late", "It was late."));
        assert!(note_id("en:late", "It was late.").starts_with("hilvan:en:late:"));
    }

    #[tokio::test]
    async fn every_word_taken_is_a_note_with_where_it_was_read() {
        let pool = tutor().await;
        let library = Library::default();
        let pasted = reading::add(&pool, &library, &plain("Rain", "Suddenly it rained."), at(0)).await.unwrap();
        let sentence = reading::read(&pool, &library, pasted.id, None).await.unwrap().lines[0][0].sentence;
        crate::words::pick(&pool, "en:suddenly", sentence, 0, at(1)).await.unwrap();
        let kept = reading::keep(&pool, &library, book(), at(2)).await.unwrap();
        let house = reading::read(&pool, &library, kept.id, Some(0)).await.unwrap().lines[0][0].sentence;
        crate::words::pick(&pool, "en:house", house, 23, at(3)).await.unwrap();

        let notes = notes(&pool).await.unwrap();
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].front, "<b>Suddenly</b> it rained.");
        assert!(notes[0].back.starts_with("<b>suddenly</b> /"), "{}", notes[0].back);
        assert!(notes[0].back.contains(" - вдруг<br><i>Rain</i>"), "{}", notes[0].back);
        assert_eq!(notes[1].front, "Then Harry went to the <b>house</b>.");
        assert!(notes[1].back.ends_with("<i>The Book · One</i>"), "a book's chapter: {}", notes[1].back);
        assert_eq!(notes[0].id, note_id("en:suddenly", "Suddenly it rained."));
        assert_ne!(notes[0].id, notes[1].id);

        let file = file(&notes);
        let lines: Vec<&str> = file.lines().collect();
        assert_eq!(
            lines[..6],
            [
                "#separator:tab",
                "#html:true",
                "#notetype:Basic",
                "#deck:hilvan",
                "#guid column:1",
                "#tags column:4"
            ]
        );
        assert_eq!(lines.len(), 8);
        assert!(lines[6..].iter().all(|line| line.split('\t').count() == 4), "{file}");
    }
}
