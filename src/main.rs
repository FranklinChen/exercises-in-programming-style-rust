#![allow(missing_docs)]

use std::{env, fs, process};
use term_frequency::{self, OutputFormat};

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: tf <input-file> [--parallel] [--format=classic|csv|json]");
        process::exit(1);
    }

    let input_path = &args[1];
    let use_parallel = args.iter().any(|a| a == "--parallel");
    let format = args
        .iter()
        .find_map(|a| a.strip_prefix("--format="))
        .unwrap_or("classic");
    let format = match format {
        "csv" => OutputFormat::Csv,
        "json" => OutputFormat::Json,
        _ => OutputFormat::Classic,
    };

    let text = fs::read_to_string(input_path).unwrap_or_else(|e| {
        eprintln!("Error reading {input_path}: {e}");
        process::exit(1);
    });

    let stop_content = find_stop_words(input_path);
    let stop_words = term_frequency::load_stop_words(&stop_content);

    let results = if use_parallel {
        term_frequency::parallel::pipeline(&text, &stop_words)
    } else {
        term_frequency::pipeline(&text, &stop_words)
    };

    println!("{}", term_frequency::format_output(&results, 25, format));
}

/// Search for stop_words.txt in several locations relative to the input file.
fn find_stop_words(input_path: &str) -> String {
    let input_dir = std::path::Path::new(input_path)
        .parent()
        .unwrap_or(std::path::Path::new("."));

    let candidates = [
        input_dir.join("stop_words.txt"),
        std::path::PathBuf::from("stop_words.txt"),
        std::path::PathBuf::from("data/stop_words.txt"),
    ];

    for path in &candidates {
        if let Ok(content) = fs::read_to_string(path) {
            return content;
        }
    }

    eprintln!("Could not find stop_words.txt");
    process::exit(1);
}
