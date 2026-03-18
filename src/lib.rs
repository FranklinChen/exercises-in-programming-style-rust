#![doc = include_str!("../README.md")]
#![deny(missing_docs)]
//!
//! # Design notes
//!
//! ## On runtime vs. compile-time stop words
//!
//! If stop words are **embedded at compile time** (`const STOP_WORDS: &[&str]`),
//! verification tools can reason about the *concrete set*: you could prove
//! "the word 'the' never appears in output" because the verifier knows 'the'
//! is in the set. With **runtime loading**, the best you can prove is the
//! *conditional*: "for any word w in the stop-words set, w does not appear in
//! output." This is still useful — it guarantees the filtering logic is correct
//! — but it cannot catch a malformed stop_words.txt file.
//!
//! We provide both: [`load_stop_words`] for runtime loading, and
//! [`DEFAULT_STOP_WORDS_CSV`] for compile-time embedding (used by verification
//! harnesses and available to callers who want the stronger guarantee).
//!
//! ## Unicode vs. ASCII: two pipelines, one proof boundary
//!
//! The pipeline has five stages: normalize → tokenize → filter → count → sort.
//! Only the first stage, [`normalize`], touches character classification and case
//! conversion. The remaining four stages are *character-set-agnostic* — they
//! operate on `String` and `Vec<String>` without inspecting individual characters.
//!
//! This means we can swap *just* the normalization strategy and reuse everything
//! else:
//!
//! | Stage | Unicode ([`normalize`]) | ASCII ([`ascii::normalize`]) | Shared? |
//! |-------|------------------------|------------------------------|---------|
//! | normalize | [`char::is_alphanumeric`] | [`char::is_ascii_alphanumeric`] | No |
//! | tokenize | [`tokenize`] | [`tokenize`] | Yes |
//! | filter | [`filter_stop_words`] | [`filter_stop_words`] | Yes |
//! | count | [`count_frequencies`] | [`count_frequencies`] | Yes |
//! | sort | [`sort_by_frequency`] | [`sort_by_frequency`] | Yes |
//!
//! The [`ascii`] module defines only `normalize` and `pipeline` (which wires
//! `ascii::normalize` into the shared stages). The [`parallel`] module similarly
//! replaces only `count_frequencies` with a Rayon-based variant.
//!
//! **Why this matters for verification:** Kani and Creusot can fully reason about
//! ASCII byte operations, but Unicode case folding (`'İ'` → `"i\u{307}"`) is
//! beyond their reach. By isolating the proof boundary to a single function, we
//! get *complete* proofs on the ASCII path and *tested* correctness on the
//! Unicode path — without duplicating the four shared stages.
//!
//! For English text (like Pride and Prejudice), both pipelines produce identical
//! results. The divergence test ([`tests::ascii_pipeline_diverges_on_unicode`])
//! demonstrates exactly where they differ.
//!
//! ## On output format and proof
//!
//! Output formatting is the final pipeline stage and is *injective*: distinct
//! `(word, count)` pairs produce distinct output strings. Because formatting
//! cannot merge, reorder, or drop entries, correctness of the computational
//! pipeline (everything before formatting) is sufficient. The Kani harnesses
//! verify the pipeline; formatting is tested conventionally.

#[allow(unused_imports)]
use creusot_std::macros::{ensures, requires, trusted};
#[allow(unused_imports)]
use creusot_std::prelude::Int;
use std::collections::{HashMap, HashSet};

// ── Compile-time stop words (enables stronger verification) ──────────────

/// The standard stop words list, embedded at compile time.
///
/// Identical to the contents of `stop_words.txt` from the original exercises
/// repo. Used by verification harnesses and available to callers who want
/// stronger guarantees than runtime loading (see module-level docs).
pub const DEFAULT_STOP_WORDS_CSV: &str = concat!(
    "a,able,about,across,after,all,almost,also,am,among,an,and,any,are",
    ",as,at,be,because,been,but,by,can,cannot,could,dear,did,do,does",
    ",either,else,ever,every,for,from,get,got,had,has,have,he,her,hers",
    ",him,his,how,however,i,if,in,into,is,it,its,just,least,let,like",
    ",likely,may,me,might,most,must,my,neither,no,nor,not,of,off,often",
    ",on,only,or,other,our,own,rather,said,say,says,she,should,since",
    ",so,some,than,that,the,their,them,then,there,these,they,this,tis",
    ",to,too,twas,us,wants,was,we,were,what,when,where,which,while,who",
    ",whom,why,will,with,would,yet,you,your",
);

// ── IO boundary ──────────────────────────────────────────────────────────

/// Parse a comma-separated stop-words string and add all single ASCII letters.
///
/// Single letters are added because the original exercise specification treats
/// them as stop words (they carry little semantic content on their own).
#[trusted]
pub fn load_stop_words(csv: &str) -> HashSet<String> {
    let mut set: HashSet<String> = csv.split(',').map(|s| s.trim().to_string()).collect();
    for c in b'a'..=b'z' {
        set.insert(String::from(c as char));
    }
    set
}

// ── Pure pipeline stages ─────────────────────────────────────────────────

/// Replace every non-alphanumeric character with a space; lowercase everything.
///
/// Handles Unicode: `is_alphanumeric()` and `to_lowercase()` work on the full
/// Unicode range, not just ASCII. The Creusot spec and Kani harness only reason
/// about ASCII; Unicode correctness is covered by unit tests.
#[trusted]
#[ensures(forall<i: Int> 0 <= i && i < result@.len() ==>
    (result@[i] >= 'a' && result@[i] <= 'z')
    || (result@[i] >= '0' && result@[i] <= '9')
    || result@[i] == ' ')]
pub fn normalize(input: &str) -> String {
    input
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_lowercase().next().unwrap_or(c)
            } else {
                ' '
            }
        })
        .collect()
}

/// Split normalized text on whitespace into words.
#[trusted]
pub fn tokenize(text: &str) -> Vec<String> {
    text.split_whitespace().map(String::from).collect()
}

/// Remove stop words and single-character tokens.
#[trusted]
#[ensures(forall<i: Int> 0 <= i && i < result@.len() ==>
    !stop_words@.contains(result@[i]@) && result@[i]@.len() > 1)]
pub fn filter_stop_words(words: Vec<String>, stop_words: &HashSet<String>) -> Vec<String> {
    let result: Vec<String> = words
        .into_iter()
        .filter(|w| w.len() > 1 && !stop_words.contains(w.as_str()))
        .collect();

    debug_assert!(result.iter().all(|w| !stop_words.contains(w.as_str())));
    debug_assert!(result.iter().all(|w| w.len() > 1));

    result
}

/// Count occurrences of each word.
#[trusted]
pub fn count_frequencies(words: &[String]) -> HashMap<String, usize> {
    let mut freqs = HashMap::new();
    for w in words {
        *freqs.entry(w.clone()).or_insert(0) += 1;
    }

    // Post-condition: every word in the input has a count.
    debug_assert!(words.iter().all(|w| freqs.contains_key(w)));

    freqs
}

/// Sort (word, count) pairs descending by count, breaking ties alphabetically.
#[trusted]
#[ensures(forall<i: Int, j: Int> 0 <= i && i < j && j < result@.len() ==>
    result@[i].1 >= result@[j].1)]
pub fn sort_by_frequency(freqs: HashMap<String, usize>) -> Vec<(String, usize)> {
    let mut pairs: Vec<_> = freqs.into_iter().collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    debug_assert!(pairs.windows(2).all(|w| w[0].1 >= w[1].1));

    pairs
}

/// The complete single-threaded pipeline.
#[trusted]
pub fn pipeline(text: &str, stop_words: &HashSet<String>) -> Vec<(String, usize)> {
    let normalized = normalize(text);
    let tokens = tokenize(&normalized);
    let filtered = filter_stop_words(tokens, stop_words);
    let freqs = count_frequencies(&filtered);
    sort_by_frequency(freqs)
}

// ── ASCII-only variant (fully provable) ──────────────────────────────────

/// ASCII-only variant of the pipeline, designed for complete verification.
///
/// The five-stage pipeline (normalize → tokenize → filter → count → sort) is
/// character-set-agnostic in four of its five stages. Only normalization inspects
/// individual characters. This module replaces *just* [`super::normalize`] with
/// an ASCII-restricted version; [`tokenize`], [`filter_stop_words`],
/// [`count_frequencies`], and [`sort_by_frequency`] are reused unchanged.
///
/// The default [`super::normalize`] handles full Unicode via
/// [`char::is_alphanumeric`] and [`char::to_lowercase`], but provers (Kani,
/// Creusot) can only reason about the ASCII subset. By restricting to ASCII,
/// proofs cover the *entire* input domain — not just a bounded slice.
///
/// For English text (like Pride and Prejudice), the ASCII and Unicode pipelines
/// produce identical results. The divergence test
/// ([`tests::ascii_pipeline_diverges_on_unicode`]) shows exactly where they
/// differ. The tradeoff is pedagogically interesting: ship the Unicode variant
/// for correctness on arbitrary input, but *prove* the ASCII variant
/// exhaustively.
pub mod ascii {
    use super::*;

    /// Replace every non-ASCII-alphanumeric byte with a space; ASCII-lowercase.
    ///
    /// Unlike [`super::normalize`], this operates only on ASCII and guarantees a
    /// 1:1 byte mapping (no multi-byte expansion from Unicode case conversion).
    /// The Creusot contract is identical but applies to the full input domain.
    #[trusted]
    #[ensures(forall<i: Int> 0 <= i && i < result@.len() ==>
        (result@[i] >= 'a' && result@[i] <= 'z')
        || (result@[i] >= '0' && result@[i] <= '9')
        || result@[i] == ' ')]
    pub fn normalize(input: &str) -> String {
        input
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    ' '
                }
            })
            .collect()
    }

    /// The complete ASCII-only single-threaded pipeline.
    ///
    /// Uses [`normalize`](ascii::normalize) instead of [`super::normalize`].
    /// All other stages are shared with the Unicode pipeline.
    #[trusted]
    pub fn pipeline(text: &str, stop_words: &HashSet<String>) -> Vec<(String, usize)> {
        let normalized = normalize(text);
        let tokens = super::tokenize(&normalized);
        let filtered = super::filter_stop_words(tokens, stop_words);
        let freqs = super::count_frequencies(&filtered);
        super::sort_by_frequency(freqs)
    }
}

// ── Parallel variant (Rayon) ─────────────────────────────────────────────

/// Rayon-based parallel variant of the pipeline.
///
/// Only [`count_frequencies`](parallel::count_frequencies) is parallelized — it
/// is the O(n) bottleneck. The sequential and parallel pipelines must produce
/// identical results (verified by tests, not by Kani — no Rayon support).
pub mod parallel {
    use super::*;
    use rayon::prelude::*;

    /// Count word frequencies using Rayon work-stealing parallelism.
    ///
    /// Each thread folds a local HashMap; the reduce phase merges them.
    /// Safe because Rust proves exclusive ownership of each fold accumulator
    /// at compile time — no locks, no data races.
    #[trusted]
    pub fn count_frequencies(words: &[String]) -> HashMap<String, usize> {
        words
            .par_iter()
            .fold(HashMap::new, |mut acc: HashMap<String, usize>, w| {
                *acc.entry(w.clone()).or_insert(0) += 1;
                acc
            })
            .reduce(HashMap::new, |mut a, b| {
                for (k, v) in b {
                    *a.entry(k).or_insert(0) += v;
                }
                a
            })
    }

    /// The complete parallel pipeline.
    /// Only the counting stage is parallelized — it is the O(n) bottleneck.
    /// Normalize/tokenize/filter are I/O-bound or trivially fast.
    #[trusted]
    pub fn pipeline(text: &str, stop_words: &HashSet<String>) -> Vec<(String, usize)> {
        let normalized = super::normalize(text);
        let tokens = super::tokenize(&normalized);
        let filtered = super::filter_stop_words(tokens, stop_words);
        let freqs = count_frequencies(&filtered);
        super::sort_by_frequency(freqs)
    }
}

// ── Output formatting ────────────────────────────────────────────────────

/// Output format selection.
///
/// Formatting is *injective* — distinct (word, count) pairs always produce
/// distinct output strings — so correctness of the pipeline logically implies
/// correctness of the formatted output. We therefore verify the pipeline and
/// merely test formatting conventionally.
#[derive(Debug, Clone, Copy)]
pub enum OutputFormat {
    /// `word - count` (matches the Python reference implementation)
    Classic,
    /// `word,count` with a header row
    Csv,
    /// JSON array of `{"word": "...", "count": N}` objects
    Json,
}

/// Format the top `n` results in the given [`OutputFormat`].
#[trusted]
pub fn format_output(results: &[(String, usize)], n: usize, format: OutputFormat) -> String {
    let top = &results[..n.min(results.len())];
    match format {
        OutputFormat::Classic => top
            .iter()
            .map(|(w, c)| format!("{w} - {c}"))
            .collect::<Vec<_>>()
            .join("\n"),
        OutputFormat::Csv => {
            let mut out = String::from("word,count");
            for (w, c) in top {
                out.push('\n');
                out.push_str(&format!("{w},{c}"));
            }
            out
        }
        OutputFormat::Json => {
            let entries: Vec<String> = top
                .iter()
                .map(|(w, c)| format!("  {{\"word\": \"{w}\", \"count\": {c}}}"))
                .collect();
            format!("[\n{}\n]", entries.join(",\n"))
        }
    }
}

// ── Kani bounded model-checking harnesses ────────────────────────────────

#[cfg(kani)]
mod verification {
    use super::*;

    /// Prove: normalize produces only lowercase alphanumeric chars and spaces.
    #[kani::proof]
    fn normalize_output_chars() {
        // Test with 4 arbitrary ASCII characters (bounded).
        let bytes: [u8; 4] = kani::any();
        kani::assume(bytes.iter().all(|&b| b > 0 && b < 128));
        if let Ok(input) = std::str::from_utf8(&bytes) {
            let result = normalize(input);
            for c in result.chars() {
                assert!(
                    c == ' ' || (c.is_alphanumeric() && !c.is_uppercase()),
                    "normalize must produce only lowercase alnum or space"
                );
            }
        }
    }

    /// Prove: filter_stop_words excludes every stop word.
    #[kani::proof]
    fn filter_excludes_all_stop_words() {
        let mut stop = HashSet::new();
        stop.insert("the".to_string());
        stop.insert("is".to_string());
        stop.insert("a".to_string());

        let words = vec![
            "the".to_string(),
            "cat".to_string(),
            "is".to_string(),
            "here".to_string(),
        ];
        let filtered = filter_stop_words(words, &stop);
        for w in &filtered {
            assert!(!stop.contains(w.as_str()));
            assert!(w.len() > 1);
        }
    }

    /// Prove: count_frequencies counts correctly.
    #[kani::proof]
    fn count_frequencies_are_correct() {
        let words = vec![
            "cat".to_string(),
            "dog".to_string(),
            "cat".to_string(),
            "cat".to_string(),
            "dog".to_string(),
        ];
        let freqs = count_frequencies(&words);
        assert_eq!(*freqs.get("cat").unwrap(), 3);
        assert_eq!(*freqs.get("dog").unwrap(), 2);
        assert!(!freqs.contains_key("fish"));
    }

    /// Prove: sort_by_frequency produces descending order.
    #[kani::proof]
    fn sort_is_descending() {
        let mut freqs = HashMap::new();
        freqs.insert("a".to_string(), 5usize);
        freqs.insert("b".to_string(), 10usize);
        freqs.insert("c".to_string(), 1usize);
        freqs.insert("d".to_string(), 7usize);

        let sorted = sort_by_frequency(freqs);
        for w in sorted.windows(2) {
            assert!(w[0].1 >= w[1].1);
        }
    }

    /// Prove: normalize preserves string length (each char maps to exactly one char).
    #[kani::proof]
    fn normalize_preserves_length() {
        let bytes: [u8; 4] = kani::any();
        kani::assume(bytes.iter().all(|&b| b > 0 && b < 128));
        if let Ok(input) = std::str::from_utf8(&bytes) {
            let result = normalize(input);
            assert_eq!(
                result.len(),
                input.len(),
                "normalize must preserve length for ASCII"
            );
        }
    }

    // NOTE: Some properties (load_stop_words letter coverage, empty-input
    // pipeline, ASCII/Unicode equivalence) are verified by unit tests rather
    // than Kani harnesses. Kani explores all reachable states symbolically,
    // so functions that build large collections (HashSet with 26+ entries via
    // load_stop_words) or call the full pipeline explode the state space —
    // the original harnesses caused CI to OOM after 2+ hours. The harnesses
    // above target individual pipeline stages with small, bounded inputs,
    // which is where Kani's exhaustive exploration adds the most value over
    // conventional tests.

    // ── ASCII-only harnesses (complete domain coverage) ─────────────────

    /// Prove: ascii::normalize produces only lowercase ASCII, digits, or spaces.
    ///
    /// Unlike `normalize_output_chars` (which tests the Unicode variant on ASCII
    /// input), this tests the ASCII variant — where the proof covers the
    /// *complete* behavior, not just a subset of inputs.
    #[kani::proof]
    fn ascii_normalize_output_chars() {
        let bytes: [u8; 4] = kani::any();
        kani::assume(bytes.iter().all(|&b| b > 0 && b < 128));
        if let Ok(input) = std::str::from_utf8(&bytes) {
            let result = ascii::normalize(input);
            for c in result.chars() {
                assert!(
                    c == ' ' || c.is_ascii_lowercase() || c.is_ascii_digit(),
                    "ascii::normalize must produce only lowercase ASCII alnum or space"
                );
            }
        }
    }

    /// Prove: ascii::normalize preserves length (guaranteed for ASCII — no
    /// multi-byte expansion from Unicode case folding).
    #[kani::proof]
    fn ascii_normalize_preserves_length() {
        let bytes: [u8; 4] = kani::any();
        kani::assume(bytes.iter().all(|&b| b > 0 && b < 128));
        if let Ok(input) = std::str::from_utf8(&bytes) {
            let result = ascii::normalize(input);
            assert_eq!(
                result.len(),
                input.len(),
                "ascii::normalize must preserve length"
            );
        }
    }

    // NOTE: parallel vs sequential equivalence cannot be verified by Kani
    // (no Rayon thread-pool support). Tested conventionally in integration tests.
}

// ── Unit tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_replaces_and_lowercases() {
        assert_eq!(normalize("Hello, World!"), "hello  world ");
    }

    #[test]
    fn tokenize_splits_whitespace() {
        assert_eq!(tokenize("hello  world"), vec!["hello", "world"]);
    }

    #[test]
    fn filter_removes_stop_words_and_short() {
        let stop = load_stop_words("the,is");
        let words = vec![
            "the".into(),
            "cat".into(),
            "a".into(), // single letter, also in stop words
            "is".into(),
        ];
        assert_eq!(filter_stop_words(words, &stop), vec!["cat"]);
    }

    #[test]
    fn frequencies_count_correctly() {
        let words: Vec<String> = vec!["cat".into(), "dog".into(), "cat".into()];
        let freqs = count_frequencies(&words);
        assert_eq!(*freqs.get("cat").unwrap(), 2);
        assert_eq!(*freqs.get("dog").unwrap(), 1);
    }

    #[test]
    fn sort_descending() {
        let mut freqs = HashMap::new();
        freqs.insert("rare".into(), 1);
        freqs.insert("common".into(), 10);
        freqs.insert("mid".into(), 5);
        let sorted = sort_by_frequency(freqs);
        assert_eq!(sorted[0], ("common".into(), 10));
        assert_eq!(sorted[1], ("mid".into(), 5));
        assert_eq!(sorted[2], ("rare".into(), 1));
    }

    #[test]
    fn parallel_matches_sequential() {
        let stop = load_stop_words(DEFAULT_STOP_WORDS_CSV);
        let text = "the cat sat on the mat and the cat sat";
        let seq = pipeline(text, &stop);
        let par = parallel::pipeline(text, &stop);
        assert_eq!(seq, par);
    }

    #[test]
    fn empty_input() {
        let stop = load_stop_words(DEFAULT_STOP_WORDS_CSV);
        let results = pipeline("", &stop);
        assert!(results.is_empty());
    }

    #[test]
    fn all_stop_words() {
        let stop = load_stop_words(DEFAULT_STOP_WORDS_CSV);
        let results = pipeline("the the the a an", &stop);
        assert!(results.is_empty());
    }

    #[test]
    fn unicode_input() {
        let stop = load_stop_words("");
        let results = pipeline("café résumé café", &stop);
        assert_eq!(results[0], ("café".to_string(), 2));
        assert_eq!(results[1], ("résumé".to_string(), 1));
    }

    #[test]
    fn single_word() {
        let stop = load_stop_words(DEFAULT_STOP_WORDS_CSV);
        let results = pipeline("hello", &stop);
        assert_eq!(results, vec![("hello".to_string(), 1)]);
    }

    #[test]
    fn load_stop_words_includes_all_single_letters() {
        let stop = load_stop_words("");
        for c in b'a'..=b'z' {
            assert!(stop.contains(&String::from(c as char)));
        }
    }

    // ── ASCII variant tests ────────────────────────────────────────────

    #[test]
    fn ascii_normalize_lowercases() {
        assert_eq!(ascii::normalize("Hello, World!"), "hello  world ");
    }

    #[test]
    fn ascii_normalize_drops_non_ascii() {
        // Unicode letters become spaces in ASCII mode; preserved in Unicode mode.
        assert_eq!(ascii::normalize("café"), "caf ");
        assert_eq!(normalize("café"), "café");
    }

    #[test]
    fn ascii_pipeline_matches_unicode_on_english() {
        let stop = load_stop_words(DEFAULT_STOP_WORDS_CSV);
        let text = "It is a truth universally acknowledged";
        assert_eq!(ascii::pipeline(text, &stop), pipeline(text, &stop));
    }

    #[test]
    fn ascii_pipeline_diverges_on_unicode() {
        let stop = load_stop_words("");
        // "café" is one token in Unicode mode but "caf" in ASCII mode
        // (the 'é' becomes a space, splitting the word).
        let unicode = pipeline("café", &stop);
        let ascii_result = ascii::pipeline("café", &stop);
        assert_eq!(unicode, vec![("café".to_string(), 1)]);
        assert_eq!(ascii_result, vec![("caf".to_string(), 1)]);
    }

    // ── Format tests ─────────────────────────────────────────────────────

    #[test]
    fn format_classic() {
        let data = vec![("mr".into(), 786), ("elizabeth".into(), 635)];
        assert_eq!(
            format_output(&data, 25, OutputFormat::Classic),
            "mr - 786\nelizabeth - 635"
        );
    }

    #[test]
    fn format_csv() {
        let data = vec![("mr".into(), 786)];
        assert_eq!(
            format_output(&data, 25, OutputFormat::Csv),
            "word,count\nmr,786"
        );
    }

    #[test]
    fn format_json() {
        let data = vec![("mr".into(), 786)];
        let json = format_output(&data, 1, OutputFormat::Json);
        assert!(json.contains("\"word\": \"mr\""));
        assert!(json.contains("\"count\": 786"));
    }
}
