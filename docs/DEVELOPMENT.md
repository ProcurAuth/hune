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

The script `tests/ci/all.sh` runs the jobs of the gate in order and stops at the first failure: `policy`, `rust`, `store`, `pgsource`, `mongosource`, `api`, `source`, `frontend`, `browser`, `import`, `image`, `procurauth` and `failover`. The option `--from JOB` continues a run after a fix. The jobs are heavy: run only the one that covers your change. When all jobs pass on a clean tree, the script records the commit in `target/ci-all.ok`.

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
