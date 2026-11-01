//! Measures the shelf: every item of the catalogue fetched through the
//! reader's own import, counted with the lexicon, and written to
//! `shelves/en/profiles.tsv`.
//!
//! Ignored by the gate - it reads the internet. Run it after changing the
//! catalogue, the lexicon or the bands:
//!
//! ```console
//! cargo test --test shelf -- --ignored --nocapture
//! ```

use std::time::Duration;

use hilvan::import::web;
use hilvan::lexicon;
use hilvan::shelf::{self, Profile, Shelf};

#[tokio::test]
#[ignore = "fetches every item of the shelf: run after changing the catalogue or the lexicon"]
async fn measure_the_shelf() {
    let (language, entries) = shelf::catalogue(shelf::ENGLISH_CATALOGUE).unwrap();
    let client = web::client().unwrap();
    let mut measured = Vec::with_capacity(entries.len());
    for entry in &entries {
        let fetched = web::fetch(&client, &entry.source, lexicon::of(&language))
            .await
            .unwrap_or_else(|error| panic!("{} could not be fetched: {error}", entry.id));
        let profile = Profile::of(&language, &fetched.document);
        println!(
            "{:<24} {:>3} chapters {:>7} words  level {:?}",
            entry.id,
            fetched.document.chapters.len(),
            profile.words,
            profile.level()
        );
        measured.push((entry.id.clone(), profile));
        // A library's servers are shared: one request at a time, and a
        // breath between them.
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let text = Shelf::profiles(&language, &measured);
    Shelf::parse(shelf::ENGLISH_CATALOGUE, &text).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("shelves")
        .join(&language)
        .join("profiles.tsv");
    std::fs::write(&path, text).unwrap();
    println!("wrote {}", path.display());
}
