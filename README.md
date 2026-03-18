# Exercises in Programming Style: Rust solutions and proofs

[![CI](https://github.com/FranklinChen/exercises-in-programming-style-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/FranklinChen/exercises-in-programming-style-rust/actions/workflows/ci.yml)

Rust implementation of the term-frequency exercise from [Exercises in Programming Style](https://github.com/crista/exercises-in-programming-style). Reads a text file, filters stop words, and outputs the top 25 words by frequency.

## Usage

```bash
cargo run -- <file> [--parallel] [--format=classic|csv|json]
```

**Examples:**

```bash
cargo run -- data/pride-and-prejudice.txt
cargo run -- data/pride-and-prejudice.txt --parallel
cargo run -- data/pride-and-prejudice.txt --format=json
```

## Output Formats

- **classic** (default): `word - count`, one per line (matches the Python reference)
- **csv**: `word,count` with a header row
- **json**: JSON array of `{"word": "...", "count": N}` objects

## Testing

```bash
cargo test                     # Unit tests
cargo test --test integration  # Integration tests
```

Test data is included in `data/`.

## Verification

Three layers of assurance beyond conventional tests:

| Layer | Tool | Scope | Command |
|-------|------|-------|---------|
| Runtime contracts | `debug_assert!` | Checks postconditions in debug builds | `cargo test` |
| Bounded model checking | [Kani](https://model-checking.github.io/kani/) | Proves properties for *all* inputs up to a bound | `cargo kani` |
| Deductive verification | [Creusot](https://github.com/creusot-rs/creusot) | Proves properties for *unbounded* inputs via Why3 | `cargo creusot prove -- --lib` |

### Unicode vs. ASCII: two pipelines, one proof boundary

The pipeline has five stages: normalize → tokenize → filter → count → sort. Only `normalize` touches character classification — the other four stages are character-set-agnostic. This lets us swap *just* the normalization strategy:

| Stage | Unicode | ASCII | Shared? |
|-------|---------|-------|---------|
| normalize | `is_alphanumeric` / `to_lowercase` | `is_ascii_alphanumeric` / `to_ascii_lowercase` | No |
| tokenize | | | Yes |
| filter | | | Yes |
| count | | | Yes |
| sort | | | Yes |

The `ascii` module defines only `normalize` and `pipeline`. Kani and Creusot can fully reason about ASCII byte operations but not Unicode case folding, so the ASCII path gets *complete* proofs while the Unicode path is covered by tests. For English text, both produce identical results.

### Kani harnesses

Harnesses in `src/lib.rs` prove properties for all inputs up to a bound:

- **`normalize_output_chars`** — output contains only lowercase a-z, digits, or spaces
- **`normalize_preserves_length`** — output length equals input length (ASCII)
- **`filter_excludes_all_stop_words`** — no stop word survives filtering
- **`count_frequencies_are_correct`** — word counts match expected values
- **`sort_is_descending`** — output is sorted descending by count
- **`ascii_normalize_output_chars`** — ASCII variant: same property, complete domain
- **`ascii_normalize_preserves_length`** — ASCII variant: length preserved

Properties like load_stop_words letter coverage, empty-input pipeline behavior, tokenize-no-empty-strings, count sum invariants, and ASCII/Unicode equivalence are covered by unit tests rather than Kani harnesses. Kani explores states symbolically, so functions that build large collections (e.g., `load_stop_words` inserting 26+ entries into a `HashSet`) or call the full pipeline explode the state space. The harnesses above target individual stages with small inputs, where exhaustive exploration adds the most value.

### Creusot contracts

Active `#[ensures]` contracts on pipeline functions specify:

- **`normalize`** / **`ascii::normalize`** — every output character is lowercase alpha, digit, or space
- **`filter_stop_words`** — no output element is in the stop-words set, and all have length > 1
- **`sort_by_frequency`** — output is sorted descending by count

Functions whose bodies use iterator adaptors or `format!` (which Creusot cannot yet analyze) are marked `#[trusted]` — Creusot accepts their contracts as axioms. The `debug_assert!` and Kani layers provide independent assurance for these same properties.

### CI verification cost

CI runs four parallel jobs: build/test/lint, Kani, Creusot spec check, and Creusot prove.

| Job | First run | Cached runs | What's cached |
|-----|-----------|-------------|---------------|
| **build-and-test** | ~2 min | ~30s | Cargo registry + target (`rust-cache`) |
| **kani** | ~5 min | ~3 min | Kani manages its own install |
| **creusot-check** | ~3 min | ~10s | `cargo-creusot` + `creusot-rustc` binaries |
| **creusot-prove** | ~15 min | ~30s | opam switch with Why3, why3find, 4 SMT solvers (~1.4GB) |

Creusot proving is the heavyweight job: it requires opam, Why3, why3find, and four SMT solvers (Alt-Ergo, Z3, CVC4, CVC5). The first run installs ~1.4GB of tooling, but all of it is version-pinned and aggressively cached. Once warm, the actual proving takes <1 second for this codebase — the cached job time is dominated by cache restore and `cargo creusot` compilation, not SMT solving.

## Acknowledgments

Test data (`data/pride-and-prejudice.txt`, `data/stop_words.txt`, `data/expected-output.txt`) is from Cristina Lopes' [Exercises in Programming Style](https://github.com/crista/exercises-in-programming-style), companion repository to the book *Exercises in Programming Style* (CRC Press).
