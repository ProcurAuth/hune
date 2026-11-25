# Contribute to Hune

Thank you for your interest. This document tells how to propose a change.

## Ask a question or report a defect

Open an issue with [the issue forms](https://github.com/ProcurAuth/hune/issues/new/choose). Give the version of Hune (`hune version`), the data source, the store, and the part of the configuration that matters. Remove each secret and each real host name. Do not report a vulnerability in an issue. Read [SECURITY.md](SECURITY.md).

## Change the code

1. Open an issue first for a large change or a new feature. State the problem and the idea.
2. Read [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md). It tells how to build and how to run the checks. The jobs of the gate in `tests/ci/` build and test in Docker. Some jobs also use git, openssl and python3 on the host.
3. Write a test for each change of behavior. Rust tests are in the `tests/` folder of each crate or in the module. Front-end tests are in `tests/frontend/`.
4. Run the checks that cover your change:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p hune-core --locked
node --test tests/frontend/<name>.test.mjs
python3 -I scripts/check-docs.py
```

The full gate (`tests/ci/all.sh`) is heavy. It needs Docker, and it starts ClickHouse, PostgreSQL and MongoDB. It runs the jobs at the same time, and it needs a host with many cores and about 25 GB of free disk. 

Each job has its own script in `tests/ci/`, for example `tests/ci/frontend.sh` or `tests/ci/browser.sh`. Run the script of the job that covers your change, or run `tests/ci/all.sh --only JOB`. The logs of a gate run are in `target/ci-logs/`. The CI on GitHub runs the same jobs.

## Rules of the code

- The specification in `docs/` is the reference. A change of behavior changes the paragraph that describes it. The code and the tests cite the paragraph numbers.
- Each dependency has an exact version in `Cargo.toml` and one line with its reason. Add a dependency only when the code that it replaces is larger or less strict.
- The program reads its configuration from the HCL file only. It reads an environment variable only when the file names it with an `_env` key. A change of the configuration changes the table of keys in `crates/hune/src/config/`. Make the reference again with `hune print-reference --format markdown`.
- The program never builds SQL in the browser. The server builds every query from the saved document.
- The page has no framework, no build step and no request to another origin. A test limits the size of the page.
- The code forbids `unsafe`.
- Add `SPDX-License-Identifier: MIT OR Apache-2.0` as the first comment of each new Rust, script or test file. The files of `web/` have no header, because a test limits the size of the page. The licence of the project covers them.
- Do not edit the copied files of `web/js/vendor/`, `web/css/vendor/` and `vendor/` by hand. Their lock files and patches tell how to change them.

## Rules for the documents

- Write in ASD-STE100 Simplified Technical English. A procedural sentence has 20 words at most. A descriptive sentence has 25 words at most. A paragraph has six sentences at most. Use the active voice and the imperative in a procedure. Run `python3 -I tools/ste_lint.py <file>`.
- The text must describe the real behavior of the code. Check the code, not the old text.
- The Markdown files in `docs/` are the source of the website. After you change one, run `python3 -I scripts/build-site.py`.
- Use `example.com` and the address range `203.0.113.0/24` in examples. Never write a real host name, a real address or a real secret.
- Do not add a container image to the release or to a registry. A release holds binaries only.

## Pull requests

Open a pull request against `main`. Complete the template. The CI must pass. Your contribution is under the licence of the project: `MIT OR Apache-2.0`.

## Code of conduct

This project follows [the code of conduct](CODE_OF_CONDUCT.md).
