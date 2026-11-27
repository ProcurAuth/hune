# Development

This document tells how to build, test and release Hune. Part 7 of the [specification](SPECIFICATION.md) has the full rules (sections 16 and 17). The [contribution guide](../CONTRIBUTING.md) tells how to propose a change.

## Tools

The toolchain is fixed in `rust-toolchain.toml` (Rust 1.99.1, with the two musl targets). The CI scripts run in Docker, so the host needs only Docker. The front-end tests need Node 22 and have no npm dependency. The browser tests need Google Chrome.

## Build

```sh
cargo build --release --locked -p hune
```

The Git dependencies `procurauth-core` and `procurauth-pep` come from `https://github.com/ProcurAuth/procurauth` at a fixed commit. `Cargo.lock` fixes every version. The directory `vendor/mongodb/` is a patched copy of the crate `mongodb` that reads no environment variable (see [THIRD-PARTY-NOTICES.md](../THIRD-PARTY-NOTICES.md)). `tools/check-vendor-lock.sh` checks the copies against their lock files.

## Checks

| Check | Command |
| --- | --- |
| Format | `cargo fmt --all -- --check` |
| Lints | `cargo clippy --workspace --all-targets --locked -- -D warnings` |
| Rust tests | `cargo test --workspace --locked` |
| Front-end tests | `node --test tests/frontend/` |
| Policy | `tests/ci/policy.sh` |
| Documents and site | `python3 -I scripts/check-docs.py` |
| Copied files | `tools/check-vendor-lock.sh` |

The Rust tests that need a database or a server are ignored by default. The scripts in `tests/ci/` start the containers that they need and remove them.

## The merge gate

The script `tests/ci/all.sh` runs the jobs of the gate at the same time: `policy`, `rust`, `store`, `pgsource`, `mongosource`, `api`, `source`, `frontend`, `browser`, `import`, `image`, `procurauth` and `failover`. Each job runs to its end, also after a failure, and writes its output to `target/ci-logs/<job>.log`. At the end the script prints the duration of each job and the tail of each failed log.

The option `--only JOB[,JOB]` runs some jobs, `--from JOB` runs the jobs from `JOB` on, and `--max-jobs N` limits the number of tasks at the same time. The jobs are heavy: run only the one that covers your change. When all jobs pass on a clean tree, the script records the commit in `target/ci-all.ok`.

The workflows in `.github/workflows/` run the same jobs on GitHub. They are an option. The scripts need no GitHub Actions.

## Release

The release path is the pair of scripts `tests/ci/all.sh` and `tests/ci/release.sh`. The GitHub workflow `release.yaml` makes the same release as an option.

1. Set the version in `Cargo.toml` and add the section `## X.Y.Z - date` to `CHANGELOG.md`.
2. Run `tests/ci/all.sh` on a clean `main`.
3. Run `tests/ci/release-build.sh X.Y.Z` on a machine of each architecture. It builds twice, checks that the two builds are equal, and makes `dist/hune_X.Y.Z_linux_<arch>.tar.gz`.
4. Run `tests/ci/release.sh X.Y.Z`. It checks the input, makes `SHA256SUMS`, makes a draft release with `gh`, checks it and publishes it. The option `--amd64-only` releases the amd64 archive only.

A release holds binaries and sources. It holds no container image, and no script of the project logs in to a registry or pushes an image. The archive has a `Dockerfile` that you can build on your machine.

## Documents

The Markdown files are the source of the website. After a change to a file in `docs/`, run `python3 -I scripts/build-site.py`. The page `docs/configuration-reference.md` comes from the code: run `hune print-reference --format markdown > docs/configuration-reference.md`. Write in ASD-STE100: `python3 -I tools/ste_lint.py <file>` checks the rules that a tool can check.

## The font files

The page serves the font Inter as two files in `web/fonts/inter/`, split by unicode range. The script `tools/split_font.py` makes them from `tools/font/InterVariable-source.woff2`, the unchanged font. The script needs Python 3, `fontTools` and `brotli`, and no network.

Run `python3 tools/split_font.py` to write the two files. It prints the two `unicode-range` values for `web/css/fonts.css`. Copy them into the file by hand. Run `python3 tools/split_font.py --check` to make sure that the two files and the values in `web/css/fonts.css` are the ones of the script.

To add a character to the first file, add it to `HUNE_EXTRA` in the script and run the script again. Do this when the interface of Hune writes a new character outside the Latin range. The font file is a Modified Version under the SIL Open Font License: see [THIRD-PARTY-NOTICES.md](../THIRD-PARTY-NOTICES.md).
