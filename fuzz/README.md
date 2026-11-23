# Fuzz tests of Hune

This directory holds the fuzz targets of Hune. Section 16.16 of the specification describes them.
The `nightly` job runs them. Each target runs 10 minutes.

## Targets

| Target | Input | Property |
|---|---|---|
| `lexer` | SQL text | The tokens cover the text with no gap. `lex` and `lex_all` agree. |
| `lexer_pg` | SQL text | The same properties as `lexer`, with the lexer of the dialect `postgres` (7.20). The content range of a token is valid. The statement check of 7.25 does not panic. |
| `expand` | A byte format, see the header of `fuzz_targets/expand.rs` | No panic. A result obeys the size limit and the lexer. A free value stays in its literal (property P of 7.11 and 7.24). Bit 3 of the flags byte selects the dialect `postgres`. |
| `document_parse` | A document or a folder file | No panic. A valid result validates its own canonical bytes to the same bytes and the same etag. |
| `hcl_config` | A configuration file | No panic, with the real key table of Hune. The result is the same in two runs. |
| `grafana_import` | A Grafana dashboard file | No panic. The bytes of a document validate again to the same bytes and the same etag. |
| `frame_decode` | The text of an answer of ClickHouse (8.4) | No panic. A valid frame gives valid JSON with the right shape. |
| `mongo_bson_decode` | A mode byte, then BSON bytes, Extended JSON, or the text of a MongoDB query | No panic in the conversion of a BSON reply to the value model (8b.16), in the reader of Extended JSON and in the parser of the query profile (7.28). The conversion is the same in two runs and its documents go through the frame builder. |
| `annotation_frame` | The text of an answer of ClickHouse (8.4), as for `frame_decode` | No panic in `annotations_from_frame` (7.37). A fault names a row that exists. A result has one mark for each row, in the order of the time, and each mark obeys the limits and the character rules. The result is the same in two runs. |

The crate `hune-fuzz` is its own workspace. It has its own `Cargo.lock`. It uses the Hune crates by path.
The target `hcl_config` includes the key table `crates/hune/src/config/keys.rs` with a `#[path]` attribute,
because the binary crate has no library.

## Prepare

1. Install the nightly toolchain: `rustup toolchain install nightly --profile minimal`.
2. Install the runner: `cargo +nightly install cargo-fuzz --locked`.

## Run

- Run all targets: `tests/ci/fuzz.sh [seconds]`. The default is 600 seconds for each target.
- Run some targets: `tests/ci/fuzz.sh 60 lexer lexer_pg expand`.
- Build only: `cd fuzz && cargo +nightly fuzz build`.

The script stops at the first failure with a non-zero code.

## Corpus

The directory `corpus/<target>/` holds the seeds. The script `seed_corpus.py` makes them from the fixtures
of `tests/`. Run it with `python3 -I fuzz/seed_corpus.py`.

A seed file has a name that starts with `seed-`. Git keeps only these files. The inputs that a run finds stay local.

## Crash

A crash input is in `artifacts/<target>/`. Do these steps:

1. Minimise the input: `cargo +nightly fuzz tmin <target> <file>`.
2. Copy the minimal input to `regression/<target>/`.
3. Fix the code. Then run `tests/ci/fuzz.sh 60 <target>`.

The script replays each file of `regression/<target>/` one time before each run.
A regression input must pass at all times.
