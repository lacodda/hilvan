//! `EPUB`: a zip of XHTML chapters, a package that lists them in reading
//! order, and a table of contents that names them.
//!
//! The chapters are the table of contents', not the files': a publisher
//! splits files by size as often as by chapter, and one file may hold three
//! chapters. Each entry of the contents points at a file, or at an element
//! inside one; the reading starts a chapter where it reaches that point. A
//! book with no usable contents is a chapter a file, named by its first
//! heading.

use std::collections::HashMap;
use std::io::{Cursor, Read as _};

use zip::ZipArchive;

use super::markup::{self, Block, Mode, Tag};
use super::{Binder, Document, Draft, Unreadable, decode, percent_decoded};
use crate::lexicon::Lexicon;

/// How large one file inside a book may be once unpacked: a chapter is a
/// few hundred kilobytes, and a zip that unpacks into gigabytes is an
/// attack, not a book.
const ENTRY_LIMIT: u64 = 32 << 20;

/// How much of a book is unpacked in all.
const TOTAL_LIMIT: u64 = 256 << 20;

type Archive<'a> = ZipArchive<Cursor<&'a [u8]>>;

/// Whether a zip is an `EPUB`: it says so in its `mimetype`, or carries the
/// container that points at its package.
pub(super) fn is_epub(bytes: &[u8]) -> bool {
    let Ok(mut archive) = ZipArchive::new(Cursor::new(bytes)) else {
        return false;
    };
    if archive.by_name("META-INF/container.xml").is_ok() {
        return true;
    }
    let mut mimetype = String::new();
    archive
        .by_name("mimetype")
        .is_ok_and(|entry| entry.take(64).read_to_string(&mut mimetype).is_ok() && mimetype.trim() == "application/epub+zip")
}

fn damaged(what: impl std::fmt::Display) -> Unreadable {
    Unreadable::Damaged(what.to_string())
}

/// One file of the book, unpacked, within the limits.
fn entry(archive: &mut Archive, name: &str, unpacked: &mut u64) -> Result<Vec<u8>, Unreadable> {
    let file = archive.by_name(name).map_err(|_| damaged(format!("{name} is missing")))?;
    let mut bytes = Vec::new();
    file.take(ENTRY_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| damaged(format!("{name} cannot be unpacked: {error}")))?;
    let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    *unpacked += length;
    if length > ENTRY_LIMIT || *unpacked > TOTAL_LIMIT {
        return Err(damaged("it unpacks into more than a book can hold"));
    }
    Ok(bytes)
}

/// A path inside the book from a reference made in the file at `base`, and
/// the fragment after `#` when there is one.
fn resolve(base: &str, reference: &str) -> (String, Option<String>) {
    let (path, fragment) = match reference.split_once('#') {
        Some((path, fragment)) => (path, Some(percent_decoded(fragment)).filter(|fragment| !fragment.is_empty())),
        None => (reference, None),
    };
    if path.is_empty() {
        return (base.to_string(), fragment);
    }
    let folder = base.rsplit_once('/').map_or("", |(folder, _)| folder);
    let mut parts: Vec<&str> = if path.starts_with('/') {
        Vec::new()
    } else {
        folder.split('/').filter(|part| !part.is_empty()).collect()
    };
    let decoded = percent_decoded(path);
    for part in decoded.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    (parts.join("/"), fragment)
}

/// What the package says: the book's title and author, its files by id,
/// the reading order, and where the contents are.
#[derive(Debug, Default)]
struct Package {
    title: Option<String>,
    author: Option<String>,
    /// id → (path inside the book, media type, properties).
    manifest: HashMap<String, (String, String, String)>,
    /// Ids in reading order, the ones meant to be read in the flow of the
    /// book.
    spine: Vec<String>,
    /// The id of the `NCX` contents, as the spine names it.
    ncx: Option<String>,
}

fn package(source: &str, at: &str) -> Package {
    let mut package = Package::default();
    let mut in_metadata = false;
    let mut capturing: Option<&str> = None;
    let mut text = String::new();
    for tag in markup::tags(source, false) {
        match tag {
            Tag::Open { name, attributes, closed } => {
                let local = name.rsplit(':').next().unwrap_or(&name).to_string();
                match local.as_str() {
                    "metadata" => in_metadata = !closed,
                    "title" if in_metadata && package.title.is_none() && !closed => capturing = Some("title"),
                    "creator" if in_metadata && package.author.is_none() && !closed => capturing = Some("creator"),
                    "item" => {
                        if let (Some(id), Some(href)) = (attributes.get("id"), attributes.get("href")) {
                            let (path, _) = resolve(at, href);
                            let media = attributes.get("media-type").cloned().unwrap_or_default();
                            let properties = attributes.get("properties").cloned().unwrap_or_default();
                            package.manifest.insert(id.clone(), (path, media, properties));
                        }
                    }
                    "spine" => package.ncx = attributes.get("toc").cloned(),
                    "itemref" => {
                        let linear = attributes.get("linear").is_none_or(|linear| linear != "no");
                        if let (Some(id), true) = (attributes.get("idref"), linear) {
                            package.spine.push(id.clone());
                        }
                    }
                    _ => {}
                }
                text.clear();
            }
            Tag::Close(name) => {
                let local = name.rsplit(':').next().unwrap_or(&name);
                match (local, capturing) {
                    ("metadata", _) => in_metadata = false,
                    ("title", Some("title")) => {
                        package.title = Some(super::squeeze(&text)).filter(|title| !title.is_empty());
                        capturing = None;
                    }
                    ("creator", Some("creator")) => {
                        package.author = Some(super::squeeze(&text)).filter(|author| !author.is_empty());
                        capturing = None;
                    }
                    _ => {}
                }
            }
            Tag::Text(more) => {
                if capturing.is_some() {
                    text.push_str(&more);
                }
            }
        }
    }
    package
}

/// One entry of the contents: the file, the element in it, the title.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    file: String,
    fragment: Option<String>,
    title: String,
}

/// The entries of an `EPUB` 3 navigation document's table of contents.
fn navigation(source: &str, at: &str) -> Vec<Entry> {
    let tags = markup::tags(source, false);
    // The navigation that is the table of contents; a document with no nav
    // typed as one has its first.
    let typed = tags.iter().any(|tag| {
        matches!(tag, Tag::Open { name, attributes, .. } if name == "nav" && attributes.get("epub:type").is_some_and(|kind| kind.split_whitespace().any(|word| word == "toc")))
    });
    let mut entries = Vec::new();
    let mut depth = 0usize;
    let mut done = false;
    let mut link: Option<(String, String)> = None;
    for tag in tags {
        match tag {
            Tag::Open { name, attributes, closed } if name == "nav" && !closed => {
                let toc = attributes
                    .get("epub:type")
                    .is_some_and(|kind| kind.split_whitespace().any(|word| word == "toc"));
                if depth > 0 {
                    depth += 1;
                } else if !done && (toc || !typed) {
                    depth = 1;
                }
            }
            Tag::Close(name) if name == "nav" && depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    done = true;
                }
            }
            Tag::Open {
                name,
                attributes,
                closed: false,
            } if name == "a" && depth > 0 => {
                link = attributes.get("href").map(|href| (href.clone(), String::new()));
            }
            Tag::Close(name) if name == "a" => {
                if let Some((href, title)) = link.take() {
                    let (file, fragment) = resolve(at, &href);
                    entries.push(Entry {
                        file,
                        fragment,
                        title: super::squeeze(&title),
                    });
                }
            }
            Tag::Text(text) => {
                if let Some((_, title)) = link.as_mut() {
                    title.push_str(&text);
                }
            }
            _ => {}
        }
    }
    entries
}

/// The entries of an `EPUB` 2 `NCX` table of contents, in the order they
/// are written: its map of navigation, not its list of the printed pages.
fn ncx(source: &str, at: &str) -> Vec<Entry> {
    let mut entries = Vec::new();
    let mut label: Option<String> = None;
    let mut in_label = false;
    let mut in_map = false;
    for tag in markup::tags(source, false) {
        match tag {
            Tag::Open { name, closed, .. } if name == "navmap" => in_map = !closed,
            Tag::Close(name) if name == "navmap" => in_map = false,
            _ if !in_map => {}
            Tag::Open { name, closed, .. } if name == "navlabel" => {
                in_label = !closed;
                label = Some(String::new());
            }
            Tag::Close(name) if name == "navlabel" => in_label = false,
            Tag::Open { name, attributes, .. } if name == "content" => {
                if let Some(src) = attributes.get("src") {
                    let (file, fragment) = resolve(at, src);
                    entries.push(Entry {
                        file,
                        fragment,
                        title: super::squeeze(&label.take().unwrap_or_default()),
                    });
                }
            }
            Tag::Text(text) if in_label => {
                if let Some(label) = label.as_mut() {
                    label.push_str(&text);
                }
            }
            _ => {}
        }
    }
    entries
}

/// Reads an `EPUB` into chapters.
pub(super) fn read(bytes: &[u8], lexicon: Option<&Lexicon>) -> Result<Document, Unreadable> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|error| damaged(format!("not a readable zip: {error}")))?;
    let mut unpacked = 0;
    let container = decode(&entry(&mut archive, "META-INF/container.xml", &mut unpacked)?, None);
    let root = markup::tags(&container, false)
        .into_iter()
        .find_map(|tag| match tag {
            Tag::Open { name, attributes, .. } if name == "rootfile" => attributes.get("full-path").cloned(),
            _ => None,
        })
        .ok_or_else(|| damaged("its container names no package"))?;
    let root = percent_decoded(&root);
    let package = package(&decode(&entry(&mut archive, &root, &mut unpacked)?, None), &root);

    let navigation_file = package
        .manifest
        .values()
        .find(|(_, _, properties)| properties.split_whitespace().any(|word| word == "nav"));
    let mut contents = match navigation_file {
        Some((path, _, _)) => entry(&mut archive, path, &mut unpacked)
            .map(|bytes| navigation(&decode(&bytes, None), path))
            .unwrap_or_default(),
        None => Vec::new(),
    };
    if contents.is_empty() {
        let file = package
            .ncx
            .as_ref()
            .and_then(|id| package.manifest.get(id))
            .or_else(|| package.manifest.values().find(|(_, media, _)| media == "application/x-dtbncx+xml"));
        if let Some((path, _, _)) = file {
            contents = entry(&mut archive, path, &mut unpacked)
                .map(|bytes| ncx(&decode(&bytes, None), path))
                .unwrap_or_default();
        }
    }

    // The chapters' files in reading order, read once.
    let mut files: Vec<(String, Vec<Block>)> = Vec::new();
    for id in &package.spine {
        let Some((path, media, properties)) = package.manifest.get(id) else {
            continue;
        };
        let markup = media.contains("html") || media.is_empty();
        if !markup || properties.split_whitespace().any(|word| word == "nav") {
            continue;
        }
        let source = decode(&entry(&mut archive, path, &mut unpacked)?, None);
        files.push((path.clone(), markup::read(&source, Mode::Book).blocks));
    }
    if files.is_empty() {
        return Err(damaged("its reading order holds no chapter"));
    }

    let mut chapters = bind(&files, &contents);
    if !chapters.any_titled() {
        chapters = bind(&files, &[]);
    }
    Ok(Draft {
        title: package.title,
        author: package.author,
        chapters: chapters.into_chapters(),
    }
    .finish(lexicon))
}

/// The files bound into chapters by the contents, or a chapter a file when
/// there are no contents.
fn bind(files: &[(String, Vec<Block>)], contents: &[Entry]) -> Binder {
    let mut starts: HashMap<(&str, Option<&str>), &str> = HashMap::new();
    for entry in contents {
        starts.entry((entry.file.as_str(), entry.fragment.as_deref())).or_insert(entry.title.as_str());
    }
    let title_of = |title: &str| Some(title.to_string()).filter(|title| !title.is_empty());
    let mut binder = Binder::default();
    for (path, blocks) in files {
        if contents.is_empty() {
            binder.start(None);
        } else if let Some(title) = starts.get(&(path.as_str(), None)) {
            binder.start(title_of(title));
        }
        for block in blocks {
            match block {
                Block::Anchor(id) => {
                    if let Some(title) = starts.get(&(path.as_str(), Some(id.as_str()))) {
                        binder.start(title_of(title));
                    }
                }
                Block::Heading(_, text) => binder.title_if_untitled(text.clone()),
                Block::Paragraph(text) => binder.push(text.clone()),
            }
        }
    }
    binder
}

#[cfg(test)]
pub(crate) mod tests {
    use std::io::Write as _;

    use super::*;
    use crate::import::Chapter;
    use crate::lexicon;

    /// An `EPUB` made of the files given, each `(path, text)`.
    pub(crate) fn zip(files: &[(&str, &str)]) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(Cursor::new(&mut bytes));
            let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            for (path, text) in files {
                writer.start_file(*path, stored).unwrap();
                writer.write_all(text.as_bytes()).unwrap();
            }
            writer.finish().unwrap();
        }
        bytes
    }

    const CONTAINER: &str = r#"<?xml version="1.0"?><container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;

    fn opf(spine: &str, extra: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>A Small Book</dc:title><dc:creator>Ann Writer</dc:creator><dc:language>en</dc:language></metadata>
            <manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/><item id="title" href="Text/title.xhtml" media-type="application/xhtml+xml"/><item id="one" href="Text/one%20two.xhtml" media-type="application/xhtml+xml"/><item id="cover" href="cover.jpg" media-type="image/jpeg"/>{extra}</manifest>
            <spine>{spine}</spine></package>"#
        )
    }

    const TITLE_PAGE: &str = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><h1>A Small Book</h1><h2>by Ann Writer</h2></body></html>"#;

    /// Two chapters in one file, the second at an element inside it.
    const CHAPTERS: &str = r#"<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>x</title></head><body>
        <section id="c1"><h2>One</h2><p>The first day was cold.</p><p id="mid">We stayed&nbsp;in.</p></section>
        <section id="c2"><h2>Two</h2><p>The second day was warm.</p></section></body></html>"#;

    pub(crate) fn book() -> Vec<u8> {
        book_with(&[])
    }

    /// The small book, with more files beside its own: pictures, say.
    pub(crate) fn book_with(extra: &[(&str, &str)]) -> Vec<u8> {
        let nav = r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body><nav epub:type="landmarks"><ol><li><a href="Text/title.xhtml">Begin</a></li></ol></nav><nav epub:type="toc"><ol><li><a href="Text/title.xhtml">Title page</a></li><li><a href="Text/one%20two.xhtml#c1">Chapter One</a></li><li><a href="Text/one%20two.xhtml#c2">Chapter <i>Two</i></a></li></ol></nav></body></html>"#;
        let package = opf(r#"<itemref idref="title"/><itemref idref="one"/><itemref idref="nav" linear="no"/>"#, "");
        let mut files = vec![
            ("mimetype", "application/epub+zip"),
            ("META-INF/container.xml", CONTAINER),
            ("OEBPS/content.opf", package.as_str()),
            ("OEBPS/nav.xhtml", nav),
            ("OEBPS/Text/title.xhtml", TITLE_PAGE),
            ("OEBPS/Text/one two.xhtml", CHAPTERS),
        ];
        files.extend_from_slice(extra);
        zip(&files)
    }

    fn chapters(document: &Document) -> Vec<(Option<&str>, Vec<Vec<&str>>)> {
        document
            .chapters
            .iter()
            .map(|Chapter { title, paragraphs }| {
                (
                    title.as_deref(),
                    paragraphs.iter().map(|paragraph| paragraph.iter().map(String::as_str).collect()).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn an_epub_is_read_into_the_chapters_its_contents_name() {
        let bytes = book();
        assert!(is_epub(&bytes));
        let document = read(&bytes, lexicon::of("en")).unwrap();
        assert_eq!(document.title.as_deref(), Some("A Small Book"));
        assert_eq!(document.author.as_deref(), Some("Ann Writer"));
        assert_eq!(
            chapters(&document),
            vec![
                (Some("Chapter One"), vec![vec!["The first day was cold."], vec!["We stayed in."]]),
                (Some("Chapter Two"), vec![vec!["The second day was warm."]]),
            ],
            "the title page reads nothing and gives its place up; headings name nothing the contents name"
        );
    }

    #[test]
    fn an_epub_two_is_read_by_its_ncx() {
        // Nested points, and a list of printed pages that is no chapter.
        let ncx = r#"<?xml version="1.0"?><ncx xmlns="http://www.daisy.org/z3986/2005/ncx/"><navMap><navPoint id="a" playOrder="1"><navLabel><text>The First</text></navLabel><content src="Text/one%20two.xhtml#c1"/><navPoint id="b" playOrder="2"><navLabel><text>The Second</text></navLabel><content src="Text/one%20two.xhtml#c2"/></navPoint></navPoint></navMap><pageList><pageTarget id="p2" type="normal" value="2"><navLabel><text>{2}</text></navLabel><content src="Text/one%20two.xhtml#mid"/></pageTarget></pageList></ncx>"#;
        let package = opf(
            r#"<itemref idref="one"/>"#,
            r#"<item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>"#,
        )
        .replace(r#"properties="nav""#, "")
        .replace("<spine>", r#"<spine toc="ncx">"#);
        let bytes = zip(&[
            ("META-INF/container.xml", CONTAINER),
            ("OEBPS/content.opf", &package),
            ("OEBPS/toc.ncx", ncx),
            ("OEBPS/Text/one two.xhtml", CHAPTERS),
        ]);
        let document = read(&bytes, lexicon::of("en")).unwrap();
        let titles: Vec<Option<&str>> = chapters(&document).into_iter().map(|(title, _)| title).collect();
        assert_eq!(titles, vec![Some("The First"), Some("The Second")]);
    }

    #[test]
    fn an_epub_without_contents_is_a_chapter_a_file_named_by_its_heading() {
        let package = opf(r#"<itemref idref="title"/><itemref idref="one"/>"#, "").replace(r#"properties="nav""#, "");
        let bytes = zip(&[
            ("META-INF/container.xml", CONTAINER),
            ("OEBPS/content.opf", &package),
            ("OEBPS/Text/title.xhtml", TITLE_PAGE),
            ("OEBPS/Text/one two.xhtml", CHAPTERS),
        ]);
        let document = read(&bytes, lexicon::of("en")).unwrap();
        let read: Vec<(Option<&str>, usize)> = chapters(&document).into_iter().map(|(title, paragraphs)| (title, paragraphs.len())).collect();
        assert_eq!(read, vec![(Some("One"), 3)], "a file is a chapter; its first heading names it");
    }

    #[test]
    fn a_damaged_epub_says_what_is_wrong() {
        let no_package = zip(&[("META-INF/container.xml", "<container><rootfiles></rootfiles></container>")]);
        assert!(matches!(read(&no_package, None), Err(Unreadable::Damaged(_))));
        let missing = zip(&[("META-INF/container.xml", CONTAINER)]);
        assert_eq!(read(&missing, None), Err(Unreadable::Damaged("OEBPS/content.opf is missing".to_string())));
    }

    #[test]
    fn a_reference_is_resolved_from_the_file_it_is_made_in() {
        assert_eq!(resolve("OEBPS/content.opf", "Text/a%20b.xhtml"), ("OEBPS/Text/a b.xhtml".to_string(), None));
        assert_eq!(
            resolve("OEBPS/Text/nav.xhtml", "../Text/c.xhtml#p%201"),
            ("OEBPS/Text/c.xhtml".to_string(), Some("p 1".to_string()))
        );
        assert_eq!(resolve("OEBPS/a.xhtml", "#here"), ("OEBPS/a.xhtml".to_string(), Some("here".to_string())));
        assert_eq!(resolve("content.opf", "x.html"), ("x.html".to_string(), None));
    }
}
