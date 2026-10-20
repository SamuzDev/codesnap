//! Writes `target/zedsnap-preview.png` so the final look can be reviewed
//! without opening Zed.

use zedsnap_ls::config::Config;
use zedsnap_ls::snapshot::{self, OutputFormat, SnapRequest};

const SAMPLE: &str = r#"//! ZedSnap example.
use std::collections::HashMap;

/// Counts how many times each word appears.
#[derive(Debug, Default)]
pub struct Counter {
    words: HashMap<String, usize>,
}

impl Counter {
    pub fn count(&mut self, text: &str) -> usize {
        let total = text.split_whitespace().count();
        for word in text.split_whitespace() {
            *self.words.entry(word.to_string()).or_insert(0) += 1;
        }
        total // Returns the total number of words
    }

    pub fn top(&self, n: usize) -> Vec<(&str, &usize)> {
        let mut sorted: Vec<_> = self.words.iter().map(|(k, v)| (k.as_str(), v)).collect();
        sorted.sort_by_key(|(_, v)| **v);
        sorted.reverse();
        sorted.truncate(n);
        sorted
    }
}

fn main() {
    let mut counter = Counter::default();
    let text = "hello world hello zedsnap";
    println!("words: {}", counter.count(text));
    for (word, times) in counter.top(3) {
        println!("{word}: {times}");
    }
}
"#;

#[test]
fn writes_a_preview_png() {
    let config = Config {
        line_numbers: true,
        ..Config::default()
    };
    let request = SnapRequest {
        text: SAMPLE,
        language_id: Some("rust"),
        extension: Some("rs"),
        file_name: Some("counter.rs".to_string()),
        lines: None,
        format: OutputFormat::Png,
    };
    let bytes = snapshot::render(&request, &config).expect("should render");
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");

    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/target/zedsnap-preview.png");
    std::fs::write(path, &bytes).expect("could not write the preview");
}
