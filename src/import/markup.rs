//! Markup read as prose: the chapters of an `EPUB` are XHTML, a link is
//! usually a web page, and the files that describe a book are XML.
//!
//! One tokenizer reads all of them - html5gum, which knows every entity of
//! HTML (an `EPUB` writes `&nbsp;` without declaring it) and recovers from
//! markup no XML parser would take. What a tree builder would add is not
//! needed: the prose is the text inside block elements, and what is not
//! prose is a subtree to step over.

use std::collections::BTreeMap;

use html5gum::{DefaultEmitter, Token, Tokenizer};

use super::text::is_prose;
use super::{Draft, squeeze};

/// One step through markup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Tag {
    /// An element opens; `closed` when it closes itself (`<br/>`).
    Open {
        name: String,
        attributes: BTreeMap<String, String>,
        closed: bool,
    },
    Close(String),
    Text(String),
}

/// The steps of a document. `raw_text` reads the content of `script`,
/// `style` and `title` as HTML does - as text, whatever `<` stands in it -
/// which a web page needs and XML must not get: an `FB2` title holds
/// paragraphs.
pub(super) fn tags(source: &str, raw_text: bool) -> Vec<Tag> {
    let mut emitter = DefaultEmitter::default();
    emitter.naively_switch_states(raw_text);
    let lossy = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
    Tokenizer::new_with_emitter(source, emitter)
        .filter_map(Result::ok)
        .filter_map(|token| match token {
            Token::StartTag(tag) => Some(Tag::Open {
                name: lossy(&tag.name),
                attributes: tag.attributes.iter().map(|(name, value)| (lossy(name), lossy(&value.value))).collect(),
                closed: tag.self_closing,
            }),
            Token::EndTag(tag) => Some(Tag::Close(lossy(&tag.name))),
            Token::String(text) => Some(Tag::Text(lossy(&text.value))),
            Token::Comment(_) | Token::Doctype(_) | Token::Error(_) => None,
        })
        .collect()
}

/// Elements HTML never closes.
const VOID: [&str; 14] = [
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr",
];

/// Elements whose content is never prose: code, pictures, tables, notes'
/// marks, controls.
const NEVER_PROSE: [&str; 17] = [
    "script", "style", "noscript", "template", "svg", "math", "table", "figure", "sup", "button", "select", "textarea", "iframe", "object", "video", "audio",
    "canvas",
];

/// What a page wraps around its article.
const PAGE_FURNITURE: [&str; 7] = ["nav", "header", "footer", "aside", "form", "dialog", "menu"];

/// Elements that start and end a block of text.
const BLOCKS: [&str; 20] = [
    "p",
    "div",
    "li",
    "dd",
    "dt",
    "blockquote",
    "section",
    "article",
    "main",
    "body",
    "pre",
    "address",
    "center",
    "ul",
    "ol",
    "dl",
    "caption",
    "summary",
    "details",
    "hgroup",
];

/// Classes, roles and types that mark what is not the text: a library's
/// boilerplate, page numbers, notes, navigation.
const NOT_THE_TEXT: [&str; 13] = [
    "pg-boilerplate",
    "pagenum",
    "footnote",
    "endnote",
    "rearnote",
    "noteref",
    "pagebreak",
    "toc",
    "navigation",
    "banner",
    "contentinfo",
    "complementary",
    "doc-noteref",
];

/// A piece of a document read as blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Block {
    /// A heading, by its level: 1 for `h1`.
    Heading(u8, String),
    Paragraph(String),
    /// An element with an `id` starts here: where a table of contents may
    /// point.
    Anchor(String),
}

/// How a document is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    /// A chapter of a book: every block of its body.
    Book,
    /// A web page: the blocks of its article, without the furniture around
    /// it.
    Page,
}

/// A document read as blocks, with what its head says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Read {
    /// What its `title` element says.
    pub title: Option<String>,
    /// What the page says its title is to whoever shares it: `og:title`.
    pub og_title: Option<String>,
    pub author: Option<String>,
    pub blocks: Vec<Block>,
}

fn heading_level(name: &str) -> Option<u8> {
    let digit = name.strip_prefix('h')?;
    let level: u8 = digit.parse().ok()?;
    (1..=6).contains(&level).then_some(level)
}

/// Whether an element's attributes mark it as not the text.
fn marked_out(attributes: &BTreeMap<String, String>) -> bool {
    if attributes.contains_key("hidden") || attributes.get("aria-hidden").is_some_and(|value| value == "true") {
        return true;
    }
    ["class", "role", "epub:type"].iter().any(|key| {
        attributes
            .get(*key)
            .is_some_and(|value| value.split_whitespace().any(|word| NOT_THE_TEXT.contains(&word.to_lowercase().as_str())))
    })
}

/// An element being stepped over: its name, and how deep inside it the
/// reading is. Only the element's own name opens and closes it, so markup
/// that nests wrongly inside it changes nothing.
#[derive(Debug, Default)]
pub(super) struct Skip(Option<(String, usize)>);

impl Skip {
    /// Steps over an element from here to where it closes.
    pub(super) fn start(&mut self, name: String) {
        self.0 = Some((name, 1));
    }

    /// Whether a step falls inside the element being stepped over - its
    /// close included.
    pub(super) fn swallows(&mut self, tag: &Tag) -> bool {
        let Some((name, depth)) = self.0.as_mut() else {
            return false;
        };
        match tag {
            Tag::Open {
                name: opened, closed: false, ..
            } if opened == name => *depth += 1,
            Tag::Close(closed) if closed == name => {
                *depth -= 1;
                if *depth == 0 {
                    self.0 = None;
                }
            }
            _ => {}
        }
        true
    }
}

/// A block as it is read, and where in the page it stood.
struct Found {
    block: Block,
    in_article: bool,
    in_main: bool,
}

/// The reading of one document, step by step.
struct Walker {
    mode: Mode,
    read: Read,
    found: Vec<Found>,
    skip: Skip,
    /// The text of the block being read.
    current: String,
    /// The level of the heading being read, when it is one.
    heading: Option<u8>,
    in_head: bool,
    in_body: bool,
    in_title: bool,
    title: String,
    /// How deep the reading is in an `article`, and in a `main`.
    article: usize,
    main: usize,
    /// Anchors met inside a block, put after it.
    pending: Vec<String>,
}

impl Walker {
    fn new(mode: Mode) -> Self {
        Self {
            mode,
            read: Read::default(),
            found: Vec::new(),
            skip: Skip::default(),
            current: String::new(),
            heading: None,
            in_head: false,
            in_body: false,
            in_title: false,
            title: String::new(),
            article: 0,
            main: 0,
            pending: Vec::new(),
        }
    }

    fn place(&self, block: Block) -> Found {
        Found {
            block,
            in_article: self.article > 0,
            in_main: self.main > 0,
        }
    }

    /// The block being read is done: kept when it says anything, and the
    /// anchors met inside it after it.
    fn flush(&mut self) {
        let text = squeeze(&self.current);
        self.current.clear();
        if !text.is_empty() {
            let block = match self.heading.take() {
                Some(level) => Block::Heading(level, text),
                None => Block::Paragraph(text),
            };
            let found = self.place(block);
            self.found.push(found);
        }
        for id in std::mem::take(&mut self.pending) {
            let found = self.place(Block::Anchor(id));
            self.found.push(found);
        }
    }

    /// What the head says: the title, the author.
    fn head(&mut self, name: &str, attributes: &BTreeMap<String, String>, void: bool) {
        match name {
            "head" => self.in_head = true,
            "body" => self.in_body = true,
            "title" if !self.in_body => self.in_title = !void,
            "meta" => {
                let content = attributes.get("content").cloned();
                match (attributes.get("name").map(String::as_str), attributes.get("property").map(String::as_str)) {
                    (Some("author"), _) => self.read.author = self.read.author.take().or(content),
                    (_, Some("og:title")) => self.read.og_title = self.read.og_title.take().or(content),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn open(&mut self, name: String, attributes: &BTreeMap<String, String>, closed: bool) {
        let void = closed || VOID.contains(&name.as_str());
        self.head(&name, attributes, void);
        if name == "br" {
            self.current.push(' ');
        }
        let furniture = self.mode == Mode::Page && PAGE_FURNITURE.contains(&name.as_str());
        if !void && (NEVER_PROSE.contains(&name.as_str()) || furniture || marked_out(attributes)) {
            self.skip.start(name);
            return;
        }
        if self.in_head {
            return;
        }
        if let Some(id) = attributes.get("id").or_else(|| attributes.get("name").filter(|_| name == "a")) {
            if self.current.trim().is_empty() {
                let found = self.place(Block::Anchor(id.clone()));
                self.found.push(found);
            } else {
                self.pending.push(id.clone());
            }
        }
        if void {
            return;
        }
        if let Some(level) = heading_level(&name) {
            self.flush();
            self.heading = Some(level);
        } else if BLOCKS.contains(&name.as_str()) {
            self.flush();
        }
        match name.as_str() {
            "article" => self.article += 1,
            "main" => self.main += 1,
            _ => {}
        }
    }

    fn close(&mut self, name: &str) {
        match name {
            "head" => self.in_head = false,
            "title" => self.in_title = false,
            _ => {}
        }
        if self.in_head {
            return;
        }
        if heading_level(name).is_some() || BLOCKS.contains(&name) {
            self.flush();
        }
        match name {
            "article" => self.article = self.article.saturating_sub(1),
            "main" => self.main = self.main.saturating_sub(1),
            _ => {}
        }
    }

    fn text(&mut self, text: &str) {
        if self.in_title {
            self.title.push_str(text);
        } else if !self.in_head {
            self.current.push_str(text);
        }
    }

    fn finish(mut self) -> Read {
        self.flush();
        let title = squeeze(&self.title);
        let mut read = self.read;
        read.title = (!title.is_empty()).then_some(title);
        read.og_title = read.og_title.map(|title| squeeze(&title)).filter(|title| !title.is_empty());
        read.author = read.author.map(|author| squeeze(&author)).filter(|author| !author.is_empty());
        // A page's article is where its text is; a page without one keeps
        // its main, and a page without that all it has.
        let page = self.mode == Mode::Page;
        let holds = |inside: fn(&Found) -> bool| page && self.found.iter().any(|found| inside(found) && !matches!(found.block, Block::Anchor(_)));
        let keep: fn(&Found) -> bool = if holds(|found| found.in_article) {
            |found| found.in_article
        } else if holds(|found| found.in_main) {
            |found| found.in_main
        } else {
            |_| true
        };
        read.blocks = self.found.into_iter().filter(keep).map(|found| found.block).collect();
        read
    }
}

/// Reads markup into blocks.
pub(super) fn read(source: &str, mode: Mode) -> Read {
    let mut walker = Walker::new(mode);
    for tag in tags(source, mode == Mode::Page) {
        if walker.skip.swallows(&tag) {
            continue;
        }
        match tag {
            Tag::Open { name, attributes, closed } => walker.open(name, &attributes, closed),
            Tag::Close(name) => walker.close(&name),
            Tag::Text(text) => walker.text(&text),
        }
    }
    walker.finish()
}

/// A web page as a text: the prose of its article, a heading kept where
/// prose follows it. The title is what the page says it is to whoever
/// shares it, or its article's first top heading, or its `title` element -
/// last, as that one usually carries the site's name too.
pub(super) fn page(source: &str) -> Draft {
    let read = read(source, Mode::Page);
    let heading_one = read.blocks.iter().find_map(|block| match block {
        Block::Heading(1, text) => Some(text.clone()),
        _ => None,
    });
    let title = read.og_title.or(heading_one).or(read.title);
    let mut paragraphs = Vec::new();
    let mut heading: Option<String> = None;
    for block in read.blocks {
        match block {
            Block::Heading(_, text) => {
                if title.as_deref() != Some(text.as_str()) {
                    heading = Some(text);
                }
            }
            Block::Paragraph(text) if is_prose(&text) => {
                if let Some(heading) = heading.take() {
                    paragraphs.push(heading);
                }
                paragraphs.push(text);
            }
            Block::Paragraph(_) | Block::Anchor(_) => {}
        }
    }
    Draft {
        title,
        author: read.author,
        chapters: vec![(None, paragraphs)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_gives_its_article_and_leaves_its_furniture() {
        let html = r#"<!DOCTYPE html><html><head><title>Cats | The Site</title><meta property="og:title" content="All about cats"><meta name="author" content="Ann Lee"><script>var a = "<p>no</p>";</script></head>
        <body><nav><a href="/">Home</a> <a href="/news">News</a></nav><header><h1>The Site</h1></header>
        <article><h1>All about cats</h1><p>Cats are small&nbsp;animals.<sup>[1]</sup> People keep them.</p>
        <h2>Food</h2><p>Cats eat meat, and they <em>love</em> fish.</p><table><tr><td>Not prose.</td></tr></table>
        <p>Share</p><figure><img src="c.png"><figcaption>A cat.</figcaption></figure><h2>Empty</h2></article>
        <aside><p>Read this other thing today.</p></aside><div class="promo"><p>Subscribe to our letter for more news every week.</p></div><footer><p>© The Site, all rights reserved.</p></footer></body></html>"#;
        let draft = page(html);
        assert_eq!(draft.title.as_deref(), Some("All about cats"));
        assert_eq!(draft.author.as_deref(), Some("Ann Lee"));
        assert_eq!(
            draft.chapters,
            vec![(
                None,
                vec![
                    // A space that does not break is a space, as every run
                    // of whitespace is one.
                    "Cats are small animals. People keep them.".to_string(),
                    "Food".to_string(),
                    "Cats eat meat, and they love fish.".to_string(),
                ]
            )]
        );
    }

    #[test]
    fn a_page_without_an_article_keeps_its_main_or_its_body() {
        let main = page("<html><body><div><p>Outside the main text here.</p></div><main><p>Inside it.</p></main></body></html>");
        assert_eq!(main.chapters[0].1, vec!["Inside it.".to_string()]);
        let body = page("<html><head><title>T | Site</title></head><body><p>Just a body.</p><div>And a div of text.</div></body></html>");
        assert_eq!(body.title.as_deref(), Some("T | Site"));
        let headed = page("<title>T | Site</title><h1>The real title</h1><p>Some text here.</p>");
        assert_eq!(headed.title.as_deref(), Some("The real title"), "the article's heading over the site's title");
        assert_eq!(headed.chapters[0].1, vec!["Some text here.".to_string()]);
        assert_eq!(body.chapters[0].1, vec!["Just a body.".to_string(), "And a div of text.".to_string()]);
    }

    #[test]
    fn a_book_chapter_reads_into_headings_paragraphs_and_anchors() {
        let xhtml = r#"<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>Chapter
        I</title></head><body><div class="pg-boilerplate" id="pg-header"><p>Project Gutenberg licence text.</p></div>
        <div class="chapter" id="ch1"><h2><a id="c1"/>CHAPTER I.<br/>Down the Hole</h2><p>Alice was
        <i>very</i> tired.<span class="pagenum"><a id="Page_2">[2]</a></span> She sat<a id="mid"/> down.</p>
        <p>It was hot&mdash;very hot.</p></div></body></html>"#;
        let read = read(xhtml, Mode::Book);
        assert_eq!(read.title.as_deref(), Some("Chapter I"));
        assert_eq!(
            read.blocks,
            vec![
                Block::Anchor("ch1".to_string()),
                Block::Anchor("c1".to_string()),
                Block::Heading(2, "CHAPTER I. Down the Hole".to_string()),
                Block::Paragraph("Alice was very tired. She sat down.".to_string()),
                Block::Anchor("mid".to_string()),
                Block::Paragraph("It was hot\u{2014}very hot.".to_string()),
            ]
        );
    }

    #[test]
    fn a_skipped_element_ends_where_its_own_name_closes() {
        // A table inside a table, and void elements inside a skipped one.
        let read = read(
            "<body><table><tr><td><table><tr><td>x</td></tr></table><br><img src=a></td></tr></table><p>After.</p></body>",
            Mode::Book,
        );
        assert_eq!(read.blocks, vec![Block::Paragraph("After.".to_string())]);
    }
}
