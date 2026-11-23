# Changelog

This file follows the layout of [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and uses ASD-STE100 Simplified Technical English. The versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Before 1.0, a minor version can change a key, a route or the format of a document.

Each version has a section `## <version> - <YYYY-MM-DD>`. The newest version is first. The release scripts read the section of the version as the notes of the release. The section `## Unreleased` holds the changes that no release has yet.

## Unreleased

### Added

- The public documentation: a website at `https://procurauth.github.io/hune/` that `scripts/build-site.py` makes from the Markdown files, and the checks of `scripts/check-docs.py` for links, site, ASD-STE100 and hygiene.
- The guides `docs/GETTING-STARTED.md`, `docs/CONFIGURATION.md`, `docs/ARCHITECTURE.md`, `docs/SECURITY-MODEL.md`, `docs/OPERATIONS.md` and `docs/DEVELOPMENT.md`, and the index `docs/SPECIFICATION.md`.
- The demo in `examples/demo/`: synthetic ClickHouse data, a configuration and demo dashboards.
- `SECURITY.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, the issue forms, the pull request template and `THIRD-PARTY-NOTICES.md`.

### Changed

- The project licence is `MIT OR Apache-2.0`. The files `LICENSE-MIT` and `LICENSE-APACHE` replace `LICENSE`, and the release archive holds both. Before, the licence was Apache-2.0 only.
- The repository is `ProcurAuth/hune`. The git dependencies on `procurauth-core` and `procurauth-pep` use `https://github.com/ProcurAuth/procurauth`, at the same commit.
- The specification and the other documents are rewritten as public documents. The decision ids and the internal notes are removed. The section numbers stay the same.
- The class of the page selector of the editor lists has a new name, `flist__pages`.

### Removed

- The script `tools/rename-hune.py`.

## 0.1.1 - 2026-11-23

This release changes the loader of a panel. The checks ran on a local machine with `tests/ci/all.sh`. It has the `amd64` archive only.

### Added

- The script `tests/ci/all.sh` runs the jobs of the merge gate in order and stops at the first failure. The option `--from` continues a run after a fix.
- The script `tests/ci/release.sh` makes the release from a local machine. It checks the input, makes `SHA256SUMS`, makes and checks the draft release with `gh`, and publishes it. The option `--amd64-only` releases the `amd64` archive only.
- With these two scripts, a maintainer can run every job and the release with no GitHub Actions and no GitHub billing.

### Changed

- The loader of a panel is now three small dots that pulse in the middle of the body. They show 300 ms after the request starts. The old loader was a dashed bar under the title bar.
- A panel that refreshes still dims its earlier content to 60 %. The spinner in the title bar and the grey block of a panel that waits are removed.
- With `prefers-reduced-motion: reduce`, the dots do not pulse. A print has no dots.
- The workflows in `.github/workflows` stay as an option. They run the CI and the release on GitHub. Hune does not depend on them.
- The script `tests/conformance/run.sh` takes the commit of ProcurAuth from `Cargo.toml` only. No second pin exists.
- The job `policy` scans `tests/compose`, `tests/conformance` and the deployment files for image pushes, in addition to the workflows.

## 0.1.0 - 2026-11-22

This is the first release of Hune. It holds the work of the milestones that exists on this date. Version 1.0.0 comes after the exit criteria of the milestone M7.

### Added

- The server `hune` and the commands `serve`, `health`, `version`, `check-config`, `print-config` and `print-reference`. The configuration file is closed HCL, with a generated reference.
- The document format 3 in JSON: a closed schema, canonical bytes and an etag. The command `migrate-documents` changes the formats 1 and 2 to format 3.
- Two storage backends: a volume (a folder) and a PostgreSQL database. The commands `migrate-store`, `copy-store`, `export`, `import` and `restore` work on both.
- Three kinds of data source: ClickHouse, PostgreSQL and MongoDB. Each kind has a closed check of the query text, limits, cancellation and a probe.
- The importer `hune import-grafana` for the dashboards of Grafana 12.4.2, with a report, an acceptance file and an nginx map.
- The page: the list, the dashboard view, the time picker, the variables, 15 panel types, the transformations and two colour themes.
- The editor: the panel forms, the query editor with a syntax tree, the history of versions, the conflict screen and the restore.
- The access modes `open` and `authzen`. In mode `authzen`, Hune is a PEP of ProcurAuth.
- The logs, the metrics, the audit lines and an OTLP exporter of traces.
- The panel types `gauge`, `logs` and `geomap`. The map is off by default. Only the closed configuration block `geomap` gives it a tile source.
- The stat: the percent change and the text sizes.
- The time series and the bar chart: the gradient modes and the series options (dashes, axis width, border and colour, log base 2).
- The bar chart: the fill opacity, the line width, the grid lines and the orientation `auto`.
- Ad hoc filters on a ClickHouse source, for the columns of the configuration key `adhoc_columns` only, with the operators `=` and `!=`.
- The ClickHouse query parameters in the strict form `{name:Type}`, with a closed list of types.
- Annotations from PostgreSQL and MongoDB sources, with the rules of the ClickHouse annotations.
- 30 more units and 6 more data link tokens of Grafana.
- The time forms of Grafana in the URL, and the cell value inspect of a table.

### Changed

- Development builds before this release used another product name. This release is the first one with the name Hune. No compatibility layer exists for those builds.

### Security

- Security reviews of the code on 2026-11-14 and on 2026-11-16 found problems. Each high and medium problem has a fix and a test.
- The review of 2026-11-22 examined the new features of this release. Each high and medium problem has a fix and a test.
- Fixed: the ClickHouse lexer of Hune did not know three rules of ClickHouse 26.8. A comment can start with `//`, a line comment ends at LF only, and a heredoc tag can start with a digit. With such SQL, a variable value could run as code. A test sends 87 forms to ClickHouse and compares them with the lexer of Hune.
- Fixed: `$__adhocFilters` must be one AND term at the top level of the WHERE or PREWHERE clause, so a filter never adds rows. A map does not ask again for a failed tile, and it keeps decoded tiles only.

### Known limits

- The milestone M0 and the checks on real data (the parity spike and the verification V1 to V5) are not done. All tests use synthetic data.
- This release can have no `arm64` archive. The release workflow builds `arm64` only when its input `arm64` is `true`.
- The nightly job runs no fuzz target, because no pinned image has a nightly toolchain.
- The fuzz campaigns, the 30-minute load run and the CloudNativePG drill of M7 are not done.
- Some open questions remain. The file `docs/implementation-status.md` and section 19 of the specification list them.
