//! Taking a text apart: from what the learner brings - a paste, a file, a
//! link - to chapters of paragraphs of sentences.
//!
//! Every source ends as a [`Document`]. Books bring their chapters - an
//! `EPUB` its table of contents, an `FB2` its sections - and everything else
//! is one chapter. What is not prose stays behind: markup, headings of a
//! book (they name its chapters instead), notes, tables, the boilerplate of
//! a library and the navigation of a page.
//!
//! Nothing here touches the database: [`crate::reading`] keeps what comes
//! out.

pub(crate) mod epub;
mod fb2;
mod markup;
mod text;
pub mod web;

use encoding_rs::{Encoding, WINDOWS_1252};
use serde::{Deserialize, Serialize};

use crate::lexicon::Lexicon;

pub use text::sentences;

/// How a text came in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    /// Paragraphs apart by blank lines - or, with no blank line anywhere, one
    /// paragraph a line.
    Plain,
    /// Markdown: the text of its paragraphs, headings and list items, its
    /// marks and links taken off, its code left out.
    Markdown,
    /// `SubRip` or `WebVTT` subtitles: the lines of the cues, run together,
    /// a new paragraph where the speech pauses.
    Subtitles,
    /// A web page: the prose of its article, its navigation left out.
    Html,
    /// A Wikipedia article, as the encyclopedia gives its plain text.
    Wiki,
    /// An `EPUB` book: its chapters by its table of contents.
    Epub,
    /// A `FictionBook` (`FB2`) book: its chapters by its sections.
    Fb2,
}

impl Format {
    /// The word stored in the database.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Markdown => "markdown",
            Self::Subtitles => "subtitles",
            Self::Html => "html",
            Self::Wiki => "wiki",
            Self::Epub => "epub",
            Self::Fb2 => "fb2",
        }
    }

    /// Reads a format back from the database.
    ///
    /// # Errors
    ///
    /// Fails on a word that names no format.
    pub fn parse(word: &str) -> anyhow::Result<Self> {
        Ok(match word {
            "plain" => Self::Plain,
            "markdown" => Self::Markdown,
            "subtitles" => Self::Subtitles,
            "html" => Self::Html,
            "wiki" => Self::Wiki,
            "epub" => Self::Epub,
            "fb2" => Self::Fb2,
            other => anyhow::bail!("{other:?} is not a text format"),
        })
    }

    /// What a text pasted without saying looks like: subtitles when a line
    /// times a cue, a web page when it opens as one, markdown when a line is
    /// a heading or a mark of markdown stands in it, plain otherwise.
    #[must_use]
    pub fn guess(body: &str) -> Self {
        if text::is_subtitles(body) {
            return Self::Subtitles;
        }
        if opens_as_html(body) {
            return Self::Html;
        }
        if text::is_markdown(body) { Self::Markdown } else { Self::Plain }
    }

    /// What a file is by its name, when its name says.
    #[must_use]
    pub fn by_name(name: &str) -> Option<Self> {
        let name = name.to_lowercase();
        let extension = name.rsplit_once('.').map(|(_, extension)| extension)?;
        Some(match extension {
            "txt" | "text" => Self::Plain,
            "md" | "markdown" => Self::Markdown,
            "srt" | "vtt" => Self::Subtitles,
            "html" | "htm" | "xhtml" => Self::Html,
            "epub" => Self::Epub,
            "fb2" => Self::Fb2,
            "zip" if name.ends_with(".fb2.zip") => Self::Fb2,
            _ => return None,
        })
    }

    /// Whether the source has chapters of its own.
    #[must_use]
    pub const fn is_book(self) -> bool {
        matches!(self, Self::Epub | Self::Fb2)
    }
}

/// A chapter: a title when the source gives one, and paragraphs, each the
/// sentences in it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Chapter {
    pub title: Option<String>,
    pub paragraphs: Vec<Vec<String>>,
}

/// A text taken apart.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Document {
    /// The title the source carries: a book's, a page's, a markdown
    /// heading's.
    pub title: Option<String>,
    pub author: Option<String>,
    /// Never one without a sentence; none at all when there was nothing to
    /// read.
    pub chapters: Vec<Chapter>,
}

impl Document {
    /// The first sentence: what a text with no title is named by.
    #[must_use]
    pub fn first_sentence(&self) -> Option<&str> {
        self.chapters.first()?.paragraphs.first()?.first().map(String::as_str)
    }
}

/// A text before its paragraphs are cut into sentences: what each reader of
/// a format hands back.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Draft {
    pub title: Option<String>,
    pub author: Option<String>,
    /// Each chapter's title and its paragraphs.
    pub chapters: Vec<(Option<String>, Vec<String>)>,
}

impl Draft {
    /// One chapter of paragraphs.
    pub(crate) fn single(title: Option<String>, paragraphs: Vec<String>) -> Self {
        Self {
            title,
            author: None,
            chapters: vec![(None, paragraphs)],
        }
    }

    /// The draft cut into sentences, with every paragraph and chapter that
    /// holds none of them left out.
    pub(crate) fn finish(self, lexicon: Option<&Lexicon>) -> Document {
        let chapters = self
            .chapters
            .into_iter()
            .map(|(title, paragraphs)| Chapter {
                title: title.map(|title| squeeze(&title)).filter(|title| !title.is_empty()),
                paragraphs: paragraphs
                    .iter()
                    .map(|paragraph| sentences(paragraph, lexicon))
                    .filter(|sentences| !sentences.is_empty())
                    .collect(),
            })
            .filter(|chapter| !chapter.paragraphs.is_empty())
            .collect();
        Document {
            title: self.title.map(|title| squeeze(&title)).filter(|title| !title.is_empty()),
            author: self.author.map(|author| squeeze(&author)).filter(|author| !author.is_empty()),
            chapters,
        }
    }
}

/// The chapters of a book as it is read. A chapter started before anything
/// was read in the one before takes that one's place: a part's title
/// followed by its first chapter's, a title page, a table of contents.
#[derive(Debug, Default)]
pub(crate) struct Binder {
    chapters: Vec<Bound>,
}

/// A chapter being bound.
#[derive(Debug)]
struct Bound {
    title: Option<String>,
    /// Whether the title is the book's structure - its contents, a
    /// section's - rather than a heading found inside a file.
    structural: bool,
    paragraphs: Vec<String>,
}

impl Binder {
    /// The chapter being read, when nothing of it has been read yet.
    fn unread(&mut self) -> Option<&mut Bound> {
        self.chapters.last_mut().filter(|last| last.paragraphs.is_empty())
    }

    /// A chapter starts here, under its title when the structure gives one.
    /// One that starts on an unread chapter takes its place - keeping the
    /// title the structure gave it, when it brings none of its own.
    pub(crate) fn start(&mut self, title: Option<String>) {
        if let Some(last) = self.unread() {
            if title.is_some() || !last.structural {
                last.structural = title.is_some();
                last.title = title;
            }
            return;
        }
        self.chapters.push(Bound {
            structural: title.is_some(),
            title,
            paragraphs: Vec::new(),
        });
    }

    /// The chapter being read is called this, when nothing of it has been
    /// read yet: a section's own title.
    pub(crate) fn retitle(&mut self, title: String) {
        if self.chapters.is_empty() {
            self.start(Some(title));
        } else if let Some(last) = self.unread() {
            last.title = Some(title);
            last.structural = true;
        }
    }

    /// The chapter being read is called this when it has no title yet and
    /// nothing of it has been read: a heading under no entry of the
    /// contents.
    pub(crate) fn title_if_untitled(&mut self, title: String) {
        if self.chapters.is_empty() {
            self.start(None);
        }
        if let Some(last) = self.unread().filter(|last| last.title.is_none()) {
            last.title = Some(title);
        }
    }

    pub(crate) fn push(&mut self, paragraph: String) {
        if self.chapters.is_empty() {
            self.start(None);
        }
        if let Some(last) = self.chapters.last_mut() {
            last.paragraphs.push(paragraph);
        }
    }

    /// Whether the structure gave any chapter a title.
    pub(crate) fn any_titled(&self) -> bool {
        self.chapters.iter().any(|chapter| chapter.structural)
    }

    pub(crate) fn into_chapters(self) -> Vec<(Option<String>, Vec<String>)> {
        self.chapters.into_iter().map(|chapter| (chapter.title, chapter.paragraphs)).collect()
    }
}

/// Runs of whitespace as one space, the ends trimmed.
pub(crate) fn squeeze(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A text written out in one of the formats that are words: pasted, or the
/// body of a file of them. A book's formats are not words and come in
/// through [`file`].
#[must_use]
pub fn written(format: Format, body: &str, lexicon: Option<&Lexicon>) -> Document {
    let body = body.replace("\r\n", "\n").replace('\r', "\n");
    let draft = match format {
        Format::Plain | Format::Epub | Format::Fb2 => Draft::single(None, text::plain(&body)),
        Format::Markdown => text::markdown(&body),
        Format::Subtitles => Draft::single(None, text::subtitles(&body)),
        Format::Html => markup::page(&body),
        Format::Wiki => Draft::single(None, text::wiki(&body)),
    };
    draft.finish(lexicon)
}

/// Why a file or a link gave nothing to read.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Unreadable {
    #[error("the file is not a book or a text hilvan reads: {0}")]
    Format(String),
    #[error("the book is damaged: {0}")]
    Damaged(String),
}

/// What came of a file: how it was written, and what it says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
    pub format: Format,
    pub document: Document,
}

/// What a file is and what it says. Its name, when there is one, says what
/// it is; otherwise its first bytes do.
///
/// # Errors
///
/// [`Unreadable`] for a zip with no book in it, a book whose parts are
/// missing, and a file of bytes no format of text reads.
pub fn file(name: Option<&str>, bytes: &[u8], lexicon: Option<&Lexicon>) -> Result<Imported, Unreadable> {
    file_declared(name, bytes, None, lexicon)
}

/// [`file`], with the encoding its sender declared for it, when it is text.
pub(crate) fn file_declared(name: Option<&str>, bytes: &[u8], charset: Option<&str>, lexicon: Option<&Lexicon>) -> Result<Imported, Unreadable> {
    let named = name.and_then(Format::by_name);
    let format = if bytes.starts_with(b"PK\x03\x04") {
        if epub::is_epub(bytes) {
            Format::Epub
        } else if fb2::zipped(bytes).is_some() {
            Format::Fb2
        } else {
            return Err(Unreadable::Format("a zip archive with no EPUB or FB2 book in it".to_string()));
        }
    } else if named == Some(Format::Epub) {
        return Err(Unreadable::Damaged("an EPUB is a zip archive, and this file is not one".to_string()));
    } else if named == Some(Format::Fb2) || fb2::is_fb2(bytes) {
        Format::Fb2
    } else if let Some(format) = named {
        format
    } else if bytes.contains(&0) && !has_bom(bytes) {
        return Err(Unreadable::Format("the file holds bytes, not text".to_string()));
    } else {
        Format::guess(&decode(bytes, charset))
    };
    let document = match format {
        Format::Epub => epub::read(bytes, lexicon)?,
        Format::Fb2 => fb2::read(bytes, lexicon)?,
        format => written(format, &decode(bytes, charset), lexicon),
    };
    Ok(Imported { format, document })
}

/// Whether a text opens as a web page.
fn opens_as_html(body: &str) -> bool {
    let start: String = body.trim_start().chars().take(15).collect::<String>().to_lowercase();
    start.starts_with("<!doctype html") || start.starts_with("<html")
}

fn has_bom(bytes: &[u8]) -> bool {
    Encoding::for_bom(bytes).is_some()
}

/// Bytes as text: by the byte order mark when there is one, by what the
/// sender says (`declared`, as an HTTP header names it), by what the text
/// says of itself - an XML declaration, an HTML `meta` - and otherwise as
/// UTF-8, or as Windows-1252 when the bytes are not UTF-8: what an old text
/// file in a Western language usually is.
#[must_use]
pub fn decode(bytes: &[u8], declared: Option<&str>) -> String {
    if let Some((encoding, bom)) = Encoding::for_bom(bytes) {
        return encoding.decode_without_bom_handling(&bytes[bom..]).0.into_owned();
    }
    let label = declared.map(str::to_string).or_else(|| declared_inside(bytes));
    if let Some(encoding) = label.and_then(|label| Encoding::for_label(label.trim().as_bytes())) {
        // A text that can say its own encoding in ASCII is not UTF-16.
        let encoding = if encoding.is_single_byte() || encoding == encoding_rs::UTF_8 {
            encoding
        } else if encoding.name().starts_with("UTF-16") {
            encoding_rs::UTF_8
        } else {
            encoding
        };
        return encoding.decode_without_bom_handling(bytes).0.into_owned();
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => WINDOWS_1252.decode_without_bom_handling(bytes).0.into_owned(),
    }
}

/// The encoding a text names in its first kilobyte: `encoding="..."` of an
/// XML declaration, `charset=...` of an HTML `meta`.
fn declared_inside(bytes: &[u8]) -> Option<String> {
    let head: String = bytes.iter().take(1024).map(|&byte| char::from(byte).to_ascii_lowercase()).collect();
    let after = |key: &str| -> Option<String> {
        let at = head.find(key)? + key.len();
        let rest = head[at..].trim_start_matches(['"', '\'', ' ']);
        let label: String = rest
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ':' | '.'))
            .collect();
        (!label.is_empty()).then_some(label)
    };
    if head.contains("<?xml")
        && let Some(label) = after("encoding=")
    {
        return Some(label);
    }
    after("charset=")
}

/// `%20` and the like back into the bytes they stand for: a reference
/// inside a book, a title in a link.
pub(crate) fn percent_decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let hex = |byte: u8| char::from(byte).to_digit(16).and_then(|digit| u8::try_from(digit).ok());
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%'
            && at + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex(bytes[at + 1]), hex(bytes[at + 2]))
        {
            out.push(high * 16 + low);
            at += 3;
            continue;
        }
        out.push(bytes[at]);
        at += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexicon;

    fn english() -> Option<&'static Lexicon> {
        lexicon::of("en")
    }

    #[test]
    fn a_name_says_what_a_file_is() {
        assert_eq!(Format::by_name("Book.EPUB"), Some(Format::Epub));
        assert_eq!(Format::by_name("tale.fb2.zip"), Some(Format::Fb2));
        assert_eq!(Format::by_name("notes.zip"), None);
        assert_eq!(Format::by_name("episode.srt"), Some(Format::Subtitles));
        assert_eq!(Format::by_name("page.htm"), Some(Format::Html));
        assert_eq!(Format::by_name("README"), None);
    }

    #[test]
    fn bytes_are_read_in_the_encoding_they_say_or_show() {
        // "café" in Windows-1252, saying nothing.
        assert_eq!(decode(b"caf\xe9", None), "café");
        // UTF-8 with a mark, and without.
        assert_eq!(decode("\u{feff}café".as_bytes(), None), "café");
        assert_eq!(decode("café".as_bytes(), None), "café");
        // Windows-1251 said by an XML declaration: a quote and a dash.
        assert_eq!(
            decode(b"<?xml version=\"1.0\" encoding=\"windows-1251\"?><p>\x93Yes\x94 \x97 no</p>", None),
            "<?xml version=\"1.0\" encoding=\"windows-1251\"?><p>\u{201c}Yes\u{201d} \u{2014} no</p>"
        );
        // A charset the sender names wins over guessing.
        assert_eq!(decode(b"caf\xe9", Some("iso-8859-1")), "café");
        assert_eq!(decode(b"<meta charset=\"utf-16\"><p>Hi</p>", None), "<meta charset=\"utf-16\"><p>Hi</p>");
    }

    #[test]
    fn a_file_without_a_name_is_told_by_what_it_holds() {
        let page = file(None, b"<!DOCTYPE html><html><body><p>The cat sat on the mat.</p></body></html>", english()).unwrap();
        assert_eq!(page.format, Format::Html);
        assert_eq!(page.document.first_sentence(), Some("The cat sat on the mat."));
        let plain = file(Some("notes.txt"), b"One line.\n\nTwo.", english()).unwrap();
        assert_eq!((plain.format, plain.document.chapters[0].paragraphs.len()), (Format::Plain, 2));
        assert!(matches!(file(None, b"\x00\x01\x02binary", english()), Err(Unreadable::Format(_))));
        assert!(matches!(file(Some("book.epub"), b"not a zip", english()), Err(Unreadable::Damaged(_))));
    }

    #[test]
    fn a_draft_keeps_only_what_has_a_sentence() {
        let draft = Draft {
            title: Some("  A   book ".to_string()),
            author: Some(" ".to_string()),
            chapters: vec![
                (Some("Contents".to_string()), vec!["  ".to_string(), "42".to_string()]),
                (Some(" Chapter  1 ".to_string()), vec!["It rained. We stayed in.".to_string(), String::new()]),
            ],
        };
        let document = draft.finish(english());
        assert_eq!(document.title.as_deref(), Some("A book"));
        assert_eq!(document.author, None);
        assert_eq!(
            document.chapters,
            vec![Chapter {
                title: Some("Chapter 1".to_string()),
                paragraphs: vec![vec!["It rained.".to_string(), "We stayed in.".to_string()]],
            }]
        );
    }
}
