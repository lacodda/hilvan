//! `FictionBook` (`FB2`): one XML file, the book's description and its
//! bodies of sections, each section titled. Often zipped one to an archive.
//!
//! A section is a chapter. A part whose sections are its chapters reads
//! nothing of its own, so it gives its place to its first chapter. The
//! bodies of notes and comments are not the book; nor is a note's mark in
//! the text.

use std::collections::BTreeMap;
use std::io::{Cursor, Read as _};

use zip::ZipArchive;

use super::markup::{self, Skip, Tag};
use super::{Binder, Document, Draft, Unreadable, decode, squeeze};
use crate::lexicon::Lexicon;

/// How large a book may be once unpacked from its zip.
const UNPACKED_LIMIT: u64 = 64 << 20;

/// Whether bytes are a `FictionBook`: it names itself in its first lines.
pub(super) fn is_fb2(bytes: &[u8]) -> bool {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(2048)]).to_lowercase();
    head.contains("<fictionbook")
}

/// The book inside a zip, when the zip holds one.
pub(super) fn zipped(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).ok()?;
    let name = archive
        .file_names()
        .filter_map(Result::ok)
        .find(|name| name.to_lowercase().ends_with(".fb2"))?
        .into_owned();
    let file = archive.by_name(&name).ok()?;
    let mut book = Vec::new();
    file.take(UNPACKED_LIMIT + 1).read_to_end(&mut book).ok()?;
    (u64::try_from(book.len()).unwrap_or(u64::MAX) <= UNPACKED_LIMIT).then_some(book)
}

/// What is being collected: text inside a paragraph, a verse, a title.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Collecting {
    /// A paragraph or a verse: prose of the chapter.
    Prose,
    /// The title of a section.
    Title,
    /// A subtitle, an epigraph's author: read, and left out.
    Aside,
}

/// Elements whose content is never the text: pictures, notes' marks,
/// embedded files.
const NEVER_THE_TEXT: [&str; 3] = ["binary", "image", "sup"];

/// The part of a `FictionBook` being read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Part {
    #[default]
    Elsewhere,
    /// The description of the book, outside what names it.
    Description,
    /// What names the book: its title, its authors.
    TitleInfo,
    /// The first author named.
    Author,
    /// A body of the book itself.
    Body,
}

impl Part {
    const fn describes(self) -> bool {
        matches!(self, Self::Description | Self::TitleInfo | Self::Author)
    }

    const fn names(self) -> bool {
        matches!(self, Self::TitleInfo | Self::Author)
    }
}

/// The reading of a `FictionBook`, step by step.
#[derive(Debug, Default)]
struct Walker {
    draft: Draft,
    binder: Binder,
    skip: Skip,
    part: Part,
    /// How deep the reading is in sections.
    sections: usize,
    /// The field of the description being read, and the author's names so
    /// far.
    field: Option<String>,
    names: Vec<String>,
    collecting: Option<Collecting>,
    text: String,
    title: String,
    in_title: bool,
    /// The lines of the stanza being read.
    stanza: Option<Vec<String>>,
}

/// The fields of the description that name the book and its author.
const FIELDS: [&str; 5] = ["book-title", "first-name", "middle-name", "last-name", "nickname"];

impl Walker {
    fn open(&mut self, name: String, attributes: &BTreeMap<String, String>, closed: bool) {
        let note = name == "a" && attributes.get("type").is_some_and(|kind| kind == "note");
        let notes = name == "body" && attributes.contains_key("name");
        let annotation = name == "annotation" && self.part.describes();
        if !closed && (NEVER_THE_TEXT.contains(&name.as_str()) || note || notes || annotation) {
            self.skip.start(name);
            return;
        }
        if closed {
            return;
        }
        match name.as_str() {
            "description" => self.part = Part::Description,
            "title-info" if self.part == Part::Description => self.part = Part::TitleInfo,
            "author" if self.part == Part::TitleInfo && self.draft.author.is_none() => self.part = Part::Author,
            field if self.part.names() && FIELDS.contains(&field) => {
                self.field = Some(name);
                self.text.clear();
            }
            "body" => self.part = Part::Body,
            "section" if self.part == Part::Body => {
                self.sections += 1;
                self.binder.start(None);
            }
            "title" if self.part == Part::Body => {
                self.in_title = true;
                self.title.clear();
            }
            "p" | "v" if self.part == Part::Body => {
                if self.in_title {
                    // The lines of a title - "Chapter 1", "The Storm" - read
                    // as one: "Chapter 1. The Storm".
                    let separator = if self.title.trim_end().chars().last().is_some_and(char::is_alphanumeric) {
                        ". "
                    } else {
                        " "
                    };
                    self.title.push_str(separator);
                    self.collecting = Some(Collecting::Title);
                } else {
                    self.text.clear();
                    self.collecting = Some(Collecting::Prose);
                }
            }
            "subtitle" | "text-author" if self.part == Part::Body => self.collecting = Some(Collecting::Aside),
            "stanza" if self.part == Part::Body => self.stanza = Some(Vec::new()),
            _ => {}
        }
    }

    fn close(&mut self, name: &str) {
        match name {
            "description" | "body" => self.part = Part::Elsewhere,
            "title-info" if self.part.names() => self.part = Part::Description,
            "author" if self.part == Part::Author => {
                self.part = Part::TitleInfo;
                let author = squeeze(&self.names.join(" "));
                if !author.is_empty() {
                    self.draft.author = Some(author);
                }
            }
            field if FIELDS.contains(&field) && self.field.as_deref() == Some(field) => {
                self.field = None;
                let value = squeeze(&self.text);
                if field == "book-title" {
                    self.draft.title = self.draft.title.take().or(Some(value));
                } else if self.part == Part::Author && field != "nickname" {
                    self.names.push(value);
                }
            }
            "section" if self.sections > 0 => self.sections -= 1,
            "title" if self.in_title => {
                self.in_title = false;
                let found = squeeze(&self.title);
                // A body's own title is the book's, not a chapter's.
                if self.sections > 0 && !found.is_empty() {
                    self.binder.retitle(found);
                }
            }
            "p" | "v" => {
                if self.collecting == Some(Collecting::Prose) {
                    let paragraph = squeeze(&self.text);
                    match self.stanza.as_mut() {
                        Some(lines) => lines.push(paragraph),
                        None if !paragraph.is_empty() => self.binder.push(paragraph),
                        None => {}
                    }
                }
                self.collecting = None;
            }
            "subtitle" | "text-author" => self.collecting = None,
            "stanza" => {
                if let Some(lines) = self.stanza.take() {
                    let verse = squeeze(&lines.join(" "));
                    if !verse.is_empty() {
                        self.binder.push(verse);
                    }
                }
            }
            _ => {}
        }
    }

    fn text(&mut self, more: &str) {
        if self.field.is_some() {
            self.text.push_str(more);
            return;
        }
        match self.collecting {
            Some(Collecting::Prose) => self.text.push_str(more),
            Some(Collecting::Title) => self.title.push_str(more),
            Some(Collecting::Aside) | None => {}
        }
    }
}

/// Reads a `FictionBook`, zipped or not, into chapters.
pub(super) fn read(bytes: &[u8], lexicon: Option<&Lexicon>) -> Result<Document, Unreadable> {
    let unzipped;
    let bytes = if bytes.starts_with(b"PK\x03\x04") {
        unzipped = zipped(bytes).ok_or_else(|| Unreadable::Damaged("the zip holds no FB2 book".to_string()))?;
        unzipped.as_slice()
    } else {
        bytes
    };
    if !is_fb2(bytes) {
        return Err(Unreadable::Damaged("it is not a FictionBook".to_string()));
    }
    let mut walker = Walker::default();
    for tag in markup::tags(&decode(bytes, None), false) {
        if walker.skip.swallows(&tag) {
            continue;
        }
        match tag {
            Tag::Open { name, attributes, closed } => walker.open(name, &attributes, closed),
            Tag::Close(name) => walker.close(&name),
            Tag::Text(more) => walker.text(&more),
        }
    }
    let mut draft = walker.draft;
    draft.chapters = walker.binder.into_chapters();
    if draft.chapters.is_empty() {
        return Err(Unreadable::Damaged("the book has no body to read".to_string()));
    }
    Ok(draft.finish(lexicon))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::import::epub::tests::zip;
    use crate::lexicon;

    pub(crate) const BOOK: &str = r##"<?xml version="1.0" encoding="utf-8"?>
<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0" xmlns:l="http://www.w3.org/1999/xlink">
<description><title-info><genre>prose</genre><author><first-name>Ann</first-name><last-name>Writer</last-name></author>
<book-title>A Small Book</book-title><annotation><p>What the book is about, not the book.</p></annotation><lang>en</lang></title-info></description>
<body><title><p>A Small Book</p></title>
<section><title><p>Part One</p></title>
  <section><title><p>Chapter 1</p><p>The Cold Day</p></title><epigraph><p>Winter is long.</p><text-author>Someone</text-author></epigraph>
    <p>The first day was <emphasis>cold</emphasis>.<a l:href="#n1" type="note">[1]</a> We stayed in.</p><empty-line/>
    <subtitle>* * *</subtitle><p>Then it snowed.</p></section>
  <section><title><p>Chapter 2</p></title><poem><stanza><v>The snow is white,</v><v>the night is long.</v></stanza></poem><p>Morning came.</p></section>
</section>
</body>
<body name="notes"><section id="n1"><title><p>1</p></title><p>A note, not the book.</p></section></body>
<binary id="cover.jpg" content-type="image/jpeg">/9j/4AAQSkZJRgABAQ==</binary>
</FictionBook>"##;

    fn chapters(document: &Document) -> Vec<(Option<&str>, Vec<Vec<&str>>)> {
        document
            .chapters
            .iter()
            .map(|chapter| {
                (
                    chapter.title.as_deref(),
                    chapter
                        .paragraphs
                        .iter()
                        .map(|paragraph| paragraph.iter().map(String::as_str).collect())
                        .collect(),
                )
            })
            .collect()
    }

    #[test]
    fn a_fictionbook_is_read_into_its_sections() {
        let document = read(BOOK.as_bytes(), lexicon::of("en")).unwrap();
        assert_eq!(document.title.as_deref(), Some("A Small Book"));
        assert_eq!(document.author.as_deref(), Some("Ann Writer"));
        assert_eq!(
            chapters(&document),
            vec![
                (
                    Some("Chapter 1. The Cold Day"),
                    vec![
                        vec!["Winter is long."],
                        vec!["The first day was cold.", "We stayed in."],
                        vec!["Then it snowed."]
                    ]
                ),
                (Some("Chapter 2"), vec![vec!["The snow is white, the night is long."], vec!["Morning came."]]),
            ]
        );
    }

    #[test]
    fn a_part_names_its_first_chapter_when_that_one_has_no_title_of_its_own() {
        let book = r#"<?xml version="1.0" encoding="utf-8"?><FictionBook><body><section><title><p>Part Two</p></title><section><p>It was spring again.</p></section><section><title><p>Later</p></title><p>It was summer.</p></section></section></body></FictionBook>"#;
        let document = read(book.as_bytes(), lexicon::of("en")).unwrap();
        let titles: Vec<Option<&str>> = document.chapters.iter().map(|chapter| chapter.title.as_deref()).collect();
        assert_eq!(titles, vec![Some("Part Two"), Some("Later")]);
    }

    #[test]
    fn a_zipped_fictionbook_is_read_too() {
        let bytes = zip(&[("book.fb2", BOOK)]);
        assert!(zipped(&bytes).is_some());
        assert_eq!(read(&bytes, lexicon::of("en")).unwrap().chapters.len(), 2);
        assert!(matches!(read(&zip(&[("readme.txt", "hi")]), None), Err(Unreadable::Damaged(_))));
    }

    #[test]
    fn a_fictionbook_in_windows_1251_is_read_as_it_says() {
        let mut bytes = br#"<?xml version="1.0" encoding="windows-1251"?><FictionBook><body><section><p>"#.to_vec();
        bytes.extend_from_slice(b"\x93Yes,\x94 she said \x97 and left.");
        bytes.extend_from_slice(b"</p></section></body></FictionBook>");
        let document = read(&bytes, lexicon::of("en")).unwrap();
        assert_eq!(
            document.chapters[0].paragraphs[0],
            vec!["\u{201c}Yes,\u{201d} she said \u{2014} and left.".to_string()]
        );
    }
}
