//! A text by its link: a book at Project Gutenberg, an article of
//! Wikipedia, a file of a book or a text, or any web page.
//!
//! Two places are asked the way they ask to be: a Gutenberg book is fetched
//! as its `EPUB`, whose table of contents gives its chapters; a Wikipedia
//! article as the plain text the encyclopedia gives of it, without its
//! tables, references and boxes. Anything else is read as what it is.

use std::time::Duration;

use reqwest::Url;
use serde::Deserialize;

use super::{Document, Format, Unreadable, file_declared, percent_decoded, written};
use crate::lexicon::Lexicon;

/// How much one link may bring: a book with its pictures, not a film.
pub const LIMIT: usize = 64 << 20;

/// How long a link may take to answer and send what it holds.
const TIMEOUT: Duration = Duration::from_secs(90);

/// The client links are fetched with: it says who is asking, as Wikipedia
/// wants, and gives up rather than hang.
///
/// # Errors
///
/// Fails when the TLS backend cannot start.
pub fn client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(concat!("hilvan/", env!("CARGO_PKG_VERSION"), " (+https://github.com/lacodda/hilvan)"))
        .timeout(TIMEOUT)
        .build()
}

/// A text fetched by its link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetched {
    pub format: Format,
    pub document: Document,
    /// The link the text is known by: the book's page, the article's, or
    /// the link as it was given.
    pub source: String,
}

/// Why a link gave nothing to read.
#[derive(Debug, thiserror::Error)]
pub enum Unfetched {
    #[error("{0:?} is not a link to follow: it has to start with http:// or https://")]
    NotALink(String),
    #[error("the link could not be reached: {0}")]
    Unreachable(String),
    #[error("the link answered {0}")]
    Refused(u16),
    #[error("the link holds more than {} MB", LIMIT >> 20)]
    TooLarge,
    #[error("there is no such article")]
    NoArticle,
    #[error(transparent)]
    Unreadable(#[from] Unreadable),
}

/// Where a link's text is fetched from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    /// A book of Project Gutenberg, by its number.
    Gutenberg(u32),
    /// An article of a Wikipedia, by the host of its language and its title.
    Wikipedia { host: String, title: String },
    /// The link itself.
    Direct(Url),
}

impl Route {
    /// The route a link takes.
    ///
    /// # Errors
    ///
    /// [`Unfetched::NotALink`] for anything but an `http` or `https` link.
    pub fn of(link: &str) -> Result<Self, Unfetched> {
        let url = Url::parse(link.trim()).map_err(|_| Unfetched::NotALink(link.to_string()))?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            return Err(Unfetched::NotALink(link.to_string()));
        }
        let host = url.host_str().unwrap_or_default().to_lowercase();
        let segments: Vec<&str> = url.path_segments().map(Iterator::collect).unwrap_or_default();
        if host == "gutenberg.org" || host.ends_with(".gutenberg.org") {
            // /ebooks/1342, /ebooks/1342.epub.noimages, /cache/epub/1342/...,
            // /files/1342/...: the first number is the book.
            let number = segments
                .iter()
                .find_map(|segment| segment.split('.').next().and_then(|stem| stem.parse::<u32>().ok()));
            if let Some(number) = number {
                return Ok(Self::Gutenberg(number));
            }
        }
        if let Some(language) = host.strip_suffix(".wikipedia.org") {
            let language = language.trim_end_matches(".m");
            if let ["wiki", title] = segments.as_slice()
                && !language.is_empty()
                && !language.contains('.')
                && !title.is_empty()
            {
                let title = percent_decoded(title).replace('_', " ");
                return Ok(Self::Wikipedia {
                    host: format!("{language}.wikipedia.org"),
                    title,
                });
            }
        }
        Ok(Self::Direct(url))
    }

    /// The link the text is known by afterwards.
    #[must_use]
    pub fn source(&self) -> String {
        match self {
            Self::Gutenberg(number) => format!("https://www.gutenberg.org/ebooks/{number}"),
            Self::Wikipedia { host, title } => format!("https://{host}/wiki/{}", title.replace(' ', "_")),
            Self::Direct(url) => url.to_string(),
        }
    }

    /// What is asked for to read the text.
    fn request(&self) -> Result<Url, Unfetched> {
        let parse = |link: String| Url::parse(&link).map_err(|_| Unfetched::NotALink(link));
        Ok(match self {
            Self::Gutenberg(number) => parse(format!("https://www.gutenberg.org/ebooks/{number}.epub.noimages"))?,
            Self::Wikipedia { host, title } => {
                let mut url = parse(format!("https://{host}/w/api.php"))?;
                url.query_pairs_mut()
                    .append_pair("action", "query")
                    .append_pair("prop", "extracts")
                    .append_pair("explaintext", "1")
                    .append_pair("exsectionformat", "wiki")
                    .append_pair("redirects", "1")
                    .append_pair("format", "json")
                    .append_pair("formatversion", "2")
                    .append_pair("titles", title);
                url
            }
            Self::Direct(url) => url.clone(),
        })
    }
}

/// What a link answered: its bytes, the type it said, and the name of the
/// file at the end of its path.
struct Answer {
    bytes: Vec<u8>,
    media: Option<String>,
    charset: Option<String>,
    name: Option<String>,
}

async fn get(client: &reqwest::Client, url: Url) -> Result<Answer, Unfetched> {
    let mut response = client.get(url).send().await.map_err(|error| Unfetched::Unreachable(error.to_string()))?;
    if !response.status().is_success() {
        return Err(Unfetched::Refused(response.status().as_u16()));
    }
    if response.content_length().is_some_and(|length| length > LIMIT as u64) {
        return Err(Unfetched::TooLarge);
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_lowercase);
    let media = content_type
        .as_deref()
        .and_then(|value| value.split(';').next())
        .map(|media| media.trim().to_string());
    let charset = content_type.as_deref().and_then(|value| {
        value
            .split(';')
            .find_map(|part| part.trim().strip_prefix("charset=").map(|label| label.trim_matches('"').to_string()))
    });
    let name = response
        .url()
        .path_segments()
        .and_then(|mut segments| segments.next_back().map(percent_decoded))
        .filter(|name| !name.is_empty());
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| Unfetched::Unreachable(error.to_string()))? {
        if bytes.len() + chunk.len() > LIMIT {
            return Err(Unfetched::TooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(Answer { bytes, media, charset, name })
}

/// What the encyclopedia answers for an article.
#[derive(Deserialize)]
struct Extracts {
    query: Option<Pages>,
}

#[derive(Deserialize)]
struct Pages {
    #[serde(default)]
    pages: Vec<Page>,
}

#[derive(Deserialize)]
struct Page {
    title: String,
    #[serde(default)]
    extract: Option<String>,
    #[serde(default)]
    missing: bool,
}

/// Fetches a link and takes its text apart.
///
/// # Errors
///
/// [`Unfetched`] for a link that is not one, cannot be reached, refuses,
/// holds too much or holds nothing to read.
pub async fn fetch(client: &reqwest::Client, link: &str, lexicon: Option<&Lexicon>) -> Result<Fetched, Unfetched> {
    let route = Route::of(link)?;
    let answer = get(client, route.request()?).await?;
    let source = route.source();
    match route {
        Route::Wikipedia { .. } => {
            let extracts: Extracts =
                serde_json::from_slice(&answer.bytes).map_err(|error| Unfetched::Unreachable(format!("Wikipedia answered something else: {error}")))?;
            let page = extracts.query.and_then(|query| query.pages.into_iter().next()).ok_or(Unfetched::NoArticle)?;
            let text = page.extract.filter(|_| !page.missing).ok_or(Unfetched::NoArticle)?;
            let mut document = written(Format::Wiki, &text, lexicon);
            document.title = Some(page.title);
            Ok(Fetched {
                format: Format::Wiki,
                document,
                source,
            })
        }
        Route::Gutenberg(_) | Route::Direct(_) => {
            let page = answer
                .media
                .as_deref()
                .is_some_and(|media| media == "text/html" || media == "application/xhtml+xml");
            let named = match answer.media.as_deref() {
                Some("application/epub+zip") => Some("book.epub".to_string()),
                Some("application/x-fictionbook+xml" | "application/x-fictionbook") => Some("book.fb2".to_string()),
                Some("text/vtt") => Some("captions.vtt".to_string()),
                Some("application/x-subrip") => Some("captions.srt".to_string()),
                Some("text/markdown") => Some("text.md".to_string()),
                _ if page => Some("page.html".to_string()),
                _ => answer.name,
            };
            let imported = file_declared(named.as_deref(), &answer.bytes, answer.charset.as_deref(), lexicon)?;
            Ok(Fetched {
                format: imported.format,
                document: imported.document,
                source,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gutenberg_link_of_any_shape_is_its_book() {
        for link in [
            "https://www.gutenberg.org/ebooks/1342",
            "http://gutenberg.org/ebooks/1342.epub.noimages",
            "https://www.gutenberg.org/cache/epub/1342/pg1342.txt",
            "https://www.gutenberg.org/files/1342/1342-h/1342-h.htm",
        ] {
            assert_eq!(Route::of(link).unwrap(), Route::Gutenberg(1342), "{link}");
        }
        assert_eq!(Route::Gutenberg(11).source(), "https://www.gutenberg.org/ebooks/11");
        assert_eq!(
            Route::Gutenberg(11).request().unwrap().as_str(),
            "https://www.gutenberg.org/ebooks/11.epub.noimages"
        );
        assert!(matches!(Route::of("https://www.gutenberg.org/about/"), Ok(Route::Direct(_))));
    }

    #[test]
    fn a_wikipedia_link_is_its_article_in_its_language() {
        let route = Route::of("https://simple.m.wikipedia.org/wiki/Solar_System%27s_moons").unwrap();
        assert_eq!(
            route,
            Route::Wikipedia {
                host: "simple.wikipedia.org".to_string(),
                title: "Solar System's moons".to_string()
            }
        );
        assert_eq!(route.source(), "https://simple.wikipedia.org/wiki/Solar_System's_moons");
        let request = route.request().unwrap();
        assert_eq!(request.host_str(), Some("simple.wikipedia.org"));
        assert!(request.query_pairs().any(|(key, value)| key == "titles" && value == "Solar System's moons"));
        assert!(matches!(Route::of("https://en.wikipedia.org/w/index.php?title=Cat"), Ok(Route::Direct(_))));
    }

    #[test]
    fn only_a_web_link_is_followed() {
        for link in ["ftp://example.org/a.txt", "file:///etc/passwd", "not a link", "javascript:alert(1)"] {
            assert!(matches!(Route::of(link), Err(Unfetched::NotALink(_))), "{link}");
        }
    }
}
