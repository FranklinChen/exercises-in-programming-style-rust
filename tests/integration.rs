use term_frequency::{self, OutputFormat};

fn load_test_data() -> (String, std::collections::HashSet<String>) {
    let text = std::fs::read_to_string("data/pride-and-prejudice.txt")
        .expect("data/pride-and-prejudice.txt not found");
    let csv =
        std::fs::read_to_string("data/stop_words.txt").expect("data/stop_words.txt not found");
    (text, term_frequency::load_stop_words(&csv))
}

fn expected_output() -> String {
    std::fs::read_to_string("data/expected-output.txt").expect("data/expected-output.txt not found")
}

#[test]
fn sequential_matches_expected() {
    let (text, stop) = load_test_data();
    let expected = expected_output();

    let results = term_frequency::pipeline(&text, &stop);
    let actual = term_frequency::format_output(&results, 25, OutputFormat::Classic);

    // Compare ignoring whitespace differences (matches test.sh's diff -b)
    let normalize_ws = |s: &str| -> Vec<String> {
        s.lines()
            .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect()
    };
    assert_eq!(normalize_ws(&actual), normalize_ws(expected.trim()));
}

#[test]
fn parallel_matches_sequential() {
    let (text, stop) = load_test_data();

    let seq = term_frequency::pipeline(&text, &stop);
    let par = term_frequency::parallel::pipeline(&text, &stop);
    assert_eq!(seq, par);
}

#[test]
fn pipeline_with_embedded_stop_words() {
    let stop = term_frequency::load_stop_words(term_frequency::DEFAULT_STOP_WORDS_CSV);
    let text = "the cat sat on the mat the cat";
    let results = term_frequency::pipeline(text, &stop);
    assert_eq!(results[0].0, "cat");
    assert_eq!(results[0].1, 2);
    assert_eq!(results[1].0, "mat");
    assert_eq!(results[1].1, 1);
    assert_eq!(results[2].0, "sat");
    assert_eq!(results[2].1, 1);
}
