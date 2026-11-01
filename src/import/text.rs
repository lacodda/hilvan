//! The formats that are words: plain text, markdown, subtitles and the plain
//! text of a Wikipedia article - and cutting a paragraph into sentences,
//! which every format ends with.

use crate::lexicon::{self, Lexicon};

use super::{Draft, squeeze};

pub(super) fn plain(body: &str) -> Vec<String> {
    // A text with blank lines between its paragraphs wraps them across
    // lines; one without a blank line anywhere is a paragraph a line - what
    // a paste from a chat or a page usually is.
    if body.lines().any(|line| line.trim().is_empty()) {
        body.split("\n\n").map(squeeze).filter(|paragraph| !paragraph.is_empty()).collect()
    } else {
        body.lines().map(squeeze).filter(|paragraph| !paragraph.is_empty()).collect()
    }
}

/// Whether a text looks like markdown: a line is a heading or opens a block
/// of code, or a link or a bold mark stands in it.
pub(super) fn is_markdown(body: &str) -> bool {
    body.lines().any(|line| {
        let line = line.trim_start();
        let hashes = line.chars().take_while(|ch| *ch == '#').count();
        ((1..=6).contains(&hashes) && line[hashes..].starts_with(' ')) || line.starts_with("```")
    }) || body.contains("](")
        || body.contains("**")
}

pub(super) fn markdown(body: &str) -> Draft {
    use pulldown_cmark::{Event, Parser, Tag, TagEnd};

    fn flush(current: &mut String, heading_one: bool, paragraphs: &mut Vec<String>, title: &mut Option<String>) {
        let text = squeeze(current);
        current.clear();
        if text.is_empty() {
            return;
        }
        // The first top heading before any prose is the title, not a line
        // of the text.
        if heading_one && title.is_none() && paragraphs.is_empty() {
            *title = Some(text);
        } else {
            paragraphs.push(text);
        }
    }

    let mut title = None;
    let mut paragraphs = Vec::new();
    let mut current = String::new();
    // Inside a block of code, an image or raw HTML: nothing in them is prose.
    let mut skipping = 0usize;
    let mut heading_one = false;
    for event in Parser::new(body) {
        match event {
            Event::Start(Tag::CodeBlock(_) | Tag::Image { .. } | Tag::HtmlBlock) => skipping += 1,
            Event::End(TagEnd::CodeBlock | TagEnd::Image | TagEnd::HtmlBlock) => skipping = skipping.saturating_sub(1),
            Event::Start(Tag::Heading { level, .. }) => {
                flush(&mut current, false, &mut paragraphs, &mut title);
                heading_one = level == pulldown_cmark::HeadingLevel::H1;
            }
            Event::End(TagEnd::Heading(_)) => {
                flush(&mut current, heading_one, &mut paragraphs, &mut title);
                heading_one = false;
            }
            Event::Start(Tag::Paragraph | Tag::Item) | Event::End(TagEnd::Paragraph | TagEnd::Item) => {
                flush(&mut current, false, &mut paragraphs, &mut title);
            }
            Event::Text(text) | Event::Code(text) if skipping == 0 => current.push_str(&text),
            Event::SoftBreak | Event::HardBreak => current.push(' '),
            _ => {}
        }
    }
    flush(&mut current, false, &mut paragraphs, &mut title);
    Draft::single(title, paragraphs)
}

/// How long the speech has to stop for the subtitles to start a new
/// paragraph: a breath is shorter, a change of scene longer.
const PAUSE_MS: i64 = 2_000;

/// The start and end of a cue, in milliseconds, when the line times one:
/// `00:01:02,500 --> 00:01:04,000` (or with full stops, as `WebVTT` writes
/// them, and with an hour that may be left off).
fn cue_times(line: &str) -> Option<(i64, i64)> {
    let (from, to) = line.split_once("-->")?;
    let moment = |text: &str| -> Option<i64> {
        let text = text.split_whitespace().next()?;
        let (clock, fraction) = text.rsplit_once([',', '.'])?;
        let mut seconds = 0i64;
        let fields: Vec<&str> = clock.split(':').collect();
        if !(2..=3).contains(&fields.len()) {
            return None;
        }
        for field in fields {
            seconds = seconds * 60 + field.parse::<i64>().ok()?;
        }
        Some(seconds * 1000 + fraction.parse::<i64>().ok()?)
    };
    Some((moment(from.trim())?, moment(to.trim())?))
}

/// Whether a line of the text times a cue.
pub(super) fn is_subtitles(body: &str) -> bool {
    body.lines().any(|line| cue_times(line).is_some())
}

/// What a cue says, without what is not speech: markup (`<i>`, `{\an8}`),
/// sounds and speakers in brackets, the dash that opens a line of dialogue,
/// the notes of a song.
fn cue_text(line: &str) -> String {
    let mut text = String::with_capacity(line.len());
    let mut depth: Vec<char> = Vec::new();
    for ch in line.chars() {
        match ch {
            '<' | '{' | '[' | '(' => depth.push(ch),
            '>' | '}' | ']' | ')' if !depth.is_empty() => {
                depth.pop();
            }
            '♪' | '♫' => {}
            _ if depth.is_empty() => text.push(ch),
            _ => {}
        }
    }
    let text = squeeze(&text);
    text.trim_start_matches(['-', '–', '—']).trim().to_string()
}

pub(super) fn subtitles(body: &str) -> Vec<String> {
    let mut paragraphs = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut last_end: Option<i64> = None;
    for block in body.split("\n\n") {
        let mut lines = block.lines().map(str::trim).filter(|line| !line.is_empty());
        // A block that times nothing is a header, a note or a style: skipped.
        let Some((start, end)) = lines.by_ref().find_map(cue_times) else {
            continue;
        };
        if last_end.is_some_and(|last| start - last >= PAUSE_MS) && !current.is_empty() {
            paragraphs.push(current.join(" "));
            current.clear();
        }
        last_end = Some(end);
        current.extend(lines.map(cue_text).filter(|line| !line.is_empty()));
    }
    if !current.is_empty() {
        paragraphs.push(current.join(" "));
    }
    paragraphs
}

/// The heading a line of a Wikipedia article's plain text is: `== History ==`.
fn wiki_heading(line: &str) -> Option<String> {
    let line = line.trim();
    let level = line.chars().take_while(|ch| *ch == '=').count();
    if level < 2 || !line.ends_with(&"=".repeat(level)) || line.len() <= 2 * level {
        return None;
    }
    Some(squeeze(&line[level..line.len() - level]))
}

/// The plain text of a Wikipedia article: a paragraph a line, headings
/// between `==`. A heading stays when prose follows it before the next one;
/// a line that is no prose - a link's name under "Other websites" - goes.
pub(super) fn wiki(body: &str) -> Vec<String> {
    let mut paragraphs = Vec::new();
    let mut heading: Option<String> = None;
    for line in body.lines() {
        if let Some(found) = wiki_heading(line) {
            heading = Some(found);
            continue;
        }
        let line = squeeze(line);
        if line.is_empty() || !is_prose(&line) {
            continue;
        }
        if let Some(heading) = heading.take() {
            paragraphs.push(heading);
        }
        paragraphs.push(line);
    }
    paragraphs
}

/// What ends a sentence.
const TERMINATORS: [char; 4] = ['.', '!', '?', '…'];

/// What may close a sentence after its stop: quotes and brackets.
const CLOSERS: [char; 7] = ['"', '\'', '”', '’', ')', ']', '»'];

/// How many words a line without a stop needs to be prose rather than a
/// label: a menu's item, a button, a link's name.
const LABEL_WORDS: usize = 6;

/// Whether a line read off a page is prose: it has a stop in it, or is too
/// long to be a label.
pub(super) fn is_prose(line: &str) -> bool {
    line.contains(TERMINATORS) || line.split_whitespace().count() >= LABEL_WORDS
}

/// The sentences of a paragraph.
///
/// A sentence ends at a stop - a full stop, a question or an exclamation
/// mark, an ellipsis, with the quotes and brackets that close after it - when
/// what follows the space does not begin in lower case. A full stop after a
/// single letter (an initial) or after an abbreviation the language's rules
/// list ("Mr.") ends nothing.
#[must_use]
pub fn sentences(paragraph: &str, lexicon: Option<&Lexicon>) -> Vec<String> {
    let chars: Vec<char> = paragraph.chars().collect();
    let mut found = Vec::new();
    let mut start = 0;
    let mut at = 0;
    while at < chars.len() {
        if !TERMINATORS.contains(&chars[at]) {
            at += 1;
            continue;
        }
        let stop = at;
        let mut end = at;
        while end + 1 < chars.len() && TERMINATORS.contains(&chars[end + 1]) {
            end += 1;
        }
        while end + 1 < chars.len() && CLOSERS.contains(&chars[end + 1]) {
            end += 1;
        }
        at = end + 1;
        if at >= chars.len() || !chars[at].is_whitespace() {
            continue;
        }
        let mut next = at;
        while next < chars.len() && chars[next].is_whitespace() {
            next += 1;
        }
        if next >= chars.len() || chars[next].is_lowercase() {
            continue;
        }
        if chars[stop] == '.' && end == stop && abbreviated(&chars[..stop], lexicon) {
            continue;
        }
        let sentence: String = chars[start..=end].iter().collect();
        found.push(sentence.trim().to_string());
        start = next;
        at = next;
    }
    let rest: String = chars[start.min(chars.len())..].iter().collect();
    if !rest.trim().is_empty() {
        found.push(rest.trim().to_string());
    }
    found.retain(|sentence| !lexicon::tokens(sentence).is_empty());
    found
}

/// Whether the word before a full stop makes it no end of a sentence: an
/// initial, or an abbreviation of the language.
fn abbreviated(before: &[char], lexicon: Option<&Lexicon>) -> bool {
    let word: String = before
        .iter()
        .rev()
        .take_while(|ch| ch.is_alphabetic() || **ch == '.')
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let word = word.trim_start_matches('.');
    if word.chars().count() == 1 && word.chars().all(char::is_uppercase) {
        return true;
    }
    lexicon.is_some_and(|lexicon| lexicon.is_abbreviation(&word.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::super::{Format, written};
    use super::*;

    fn english() -> Option<&'static Lexicon> {
        lexicon::of("en")
    }

    fn paragraphs(format: Format, body: &str) -> Vec<Vec<String>> {
        let document = written(format, body, english());
        document.chapters.into_iter().flat_map(|chapter| chapter.paragraphs).collect()
    }

    #[test]
    fn a_paragraph_breaks_into_sentences_at_their_stops() {
        assert_eq!(
            sentences("She went home. Was it late? \"Yes!\" he said. It was... Then it rained.", english()),
            vec!["She went home.", "Was it late?", "\"Yes!\" he said.", "It was...", "Then it rained."]
        );
    }

    #[test]
    fn a_stop_before_lower_case_or_inside_a_word_ends_nothing() {
        assert_eq!(
            sentences("It costs 3.50 now, e.g. on Mondays.", english()),
            vec!["It costs 3.50 now, e.g. on Mondays."]
        );
    }

    #[test]
    fn an_initial_and_an_abbreviation_end_no_sentence() {
        assert_eq!(
            sentences("Mr. Brown met J. R. Smith. Dr. Lee was late.", english()),
            vec!["Mr. Brown met J. R. Smith.", "Dr. Lee was late."]
        );
        // Without the rules of the language, "Mr." is the end of a sentence.
        assert_eq!(sentences("Mr. Brown came.", None), vec!["Mr.", "Brown came."]);
    }

    #[test]
    fn a_piece_with_no_word_in_it_is_no_sentence() {
        assert_eq!(sentences("Go. 42. Now.", english()), vec!["Go.", "Now."]);
    }

    #[test]
    fn plain_text_is_paragraphs_by_blank_lines_or_by_lines() {
        assert_eq!(
            paragraphs(Format::Plain, "One line\nwrapped here.\n\nTwo."),
            vec![vec!["One line wrapped here.".to_string()], vec!["Two.".to_string()]]
        );
        assert_eq!(
            paragraphs(Format::Plain, "First line.\r\nSecond line.").len(),
            2,
            "with no blank line anywhere, a line is a paragraph"
        );
    }

    #[test]
    fn markdown_keeps_its_prose_and_leaves_its_marks() {
        let document = written(
            Format::Markdown,
            "# The trip\n\nWe *went* to [the sea](http://x). It was **cold**.\n\n```\ncode is not prose\n```\n\n- A list item.\n\n## Later\n\n![a picture](p.png) Home `again`.",
            english(),
        );
        assert_eq!(document.title.as_deref(), Some("The trip"));
        assert_eq!(
            document.chapters[0].paragraphs,
            vec![
                vec!["We went to the sea.".to_string(), "It was cold.".to_string()],
                vec!["A list item.".to_string()],
                vec!["Later".to_string()],
                vec!["Home again.".to_string()],
            ]
        );
    }

    const SRT: &str = "1\n00:00:01,000 --> 00:00:02,500\n<i>Where are you</i>\ngoing?\n\n2\n00:00:02,700 --> 00:00:04,000\n- Home. [door slams]\n\n3\n00:00:09,000 --> 00:00:10,000\n♪ Later that night ♪\n";

    #[test]
    fn subtitles_run_their_cues_together_and_pause_into_paragraphs() {
        assert_eq!(
            paragraphs(Format::Subtitles, SRT),
            vec![
                vec!["Where are you going?".to_string(), "Home.".to_string()],
                vec!["Later that night".to_string()]
            ]
        );
        let vtt = "WEBVTT\n\n00:01.000 --> 00:02.000\nHello there.\n";
        assert_eq!(paragraphs(Format::Subtitles, vtt), vec![vec!["Hello there.".to_string()]]);
    }

    #[test]
    fn a_pasted_text_says_what_it_is() {
        assert_eq!(Format::guess(SRT), Format::Subtitles);
        assert_eq!(Format::guess("# Title\n\nText."), Format::Markdown);
        assert_eq!(Format::guess("See [this](x)."), Format::Markdown);
        assert_eq!(Format::guess("  <!DOCTYPE html><p>Hi.</p>"), Format::Html);
        assert_eq!(Format::guess("Just a #hashtag and a line.\nAnother."), Format::Plain);
    }

    #[test]
    fn a_wikipedia_article_keeps_its_prose_and_the_headings_over_it() {
        let article = "The cat is a small animal. People keep cats.\n\n== History ==\n\n=== Early times ===\nCats lived with people long ago.\n\n== Related pages ==\nDog\nLion\n\n== Other websites ==\nCats at the zoo\n";
        assert_eq!(
            paragraphs(Format::Wiki, article),
            vec![
                vec!["The cat is a small animal.".to_string(), "People keep cats.".to_string()],
                vec!["Early times".to_string()],
                vec!["Cats lived with people long ago.".to_string()],
            ]
        );
    }

    #[test]
    fn a_label_is_no_prose_and_a_long_line_or_a_stop_is() {
        assert!(!is_prose("Read more"));
        assert!(is_prose("Yes."));
        assert!(is_prose("one two three four five six"));
    }
}
