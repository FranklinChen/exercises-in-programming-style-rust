# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Rust implementation of the term-frequency exercise from "Exercises in Programming Style." The binary (`tf`) reads a text file, filters stop words, and outputs the top 25 words by frequency.

## Build and Test Commands

```bash
cargo build                    # Build
cargo test                     # Unit tests (in lib.rs)
cargo test --test integration  # Integration tests
cargo test <test_name>         # Run a single test
cargo run -- <file> [--parallel] [--format=classic|csv|json]  # Run binary
```

Test data is included in `data/` (from [Exercises in Programming Style](https://github.com/crista/exercises-in-programming-style)).

### Kani (bounded model checking)

```bash
cargo kani    # Proves properties for all inputs up to a bound
```

Kani harnesses live in `src/lib.rs` under `#[cfg(kani)] mod verification`. They verify normalize output chars, filter correctness, counting, and sort ordering.

### Creusot (deductive verification)

```bash
cargo creusot -- --lib        # Compile library to Coma (Why3)
cargo creusot prove -- --lib  # Prove all obligations
```

Creusot contracts (`#[ensures]`, `#[requires]`) are active attributes in `src/lib.rs`. Functions using iterator adaptors or `format!` that Creusot can't analyze are marked `#[trusted]` — Creusot accepts their contracts as axioms without verifying the body. Only `--lib` is used because the binary crate (`main.rs`) contains unsupported constructs.

Creusot v0.10.0 requires `nightly-2026-01-29`. When upgrading Creusot, check the matching nightly in `git show <tag>:rust-toolchain` from the Creusot repo.

## Architecture

The codebase is a single-library + binary project:

- **`src/lib.rs`** — All logic. Pure functional pipeline: `normalize → tokenize → filter_stop_words → count_frequencies → sort_by_frequency`. Also contains `parallel::pipeline` (Rayon-based), output formatting, Kani harnesses, and unit tests.
- **`src/main.rs`** — CLI entry point. Parses args, locates `stop_words.txt`, calls the pipeline, prints output.
- **`tests/integration.rs`** — End-to-end tests against the reference Pride and Prejudice output.

## CI

GitHub Actions CI is in `.github/workflows/ci.yml` with four parallel jobs: build/test/lint, Kani, Creusot check, Creusot prove.

**Always use the latest major versions of GitHub Actions.** Current versions as of March 2026:

- `actions/checkout@v6`
- `actions/cache@v5`
- `Swatinem/rust-cache@v2`
- `model-checking/kani-github-action@v1`
- `dtolnay/rust-toolchain@stable` (or `@master` with toolchain input for nightly)

When adding or updating Actions, check for newer major versions.

## Key Design Decisions

- **Three verification layers**: `debug_assert!` (runtime in debug), Kani (bounded model checking), Creusot (deductive via Why3, contracts are active `#[ensures]`/`#[requires]` attributes with `#[trusted]` on functions whose bodies use unsupported Rust features).
- **Compile-time stop words** (`DEFAULT_STOP_WORDS_CSV`) exist alongside runtime loading (`load_stop_words`) to enable stronger verification guarantees.
- **Parallel variant** only parallelizes `count_frequencies` (the O(n) bottleneck) via Rayon fold/reduce. Sequential and parallel pipelines must produce identical results.
- Uses Rust edition 2024.
