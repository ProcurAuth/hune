# Changelog

This file follows the layout of [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and uses ASD-STE100 Simplified Technical English. The versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Before 1.0, a minor version can change a key, a route or the format of a document.

Each version has a section `## <version> - <YYYY-MM-DD>`. The newest version is first. The release scripts read the section of the version as the notes of the release. The section `## Unreleased` holds the changes that no release has yet.

## Unreleased

### Added

- The public documentation: a website at `https://procurauth.github.io/hune/` that `scripts/build-site.py` makes from the Markdown files, and the checks of `scripts/check-docs.py` for links, site, ASD-STE100 and hygiene.
- The guides `docs/GETTING-STARTED.md`, `docs/CONFIGURATION.md`, `docs/ARCHITECTURE.md`, `docs/SECURITY-MODEL.md`, `docs/OPERATIONS.md` and `docs/DEVELOPMENT.md`, and the index `docs/SPECIFICATION.md`.
- The demo in `examples/demo/`: synthetic ClickHouse data, a configuration and demo dashboards.
- `SECURITY.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, the issue forms, the pull request template and `THIRD-PARTY-NOTICES.md`.
- `hune help`, `hune --help` and `hune -h` write the usage text on the standard output and exit with 0. A known command with only `--help` or `-h` after it does the same.
- `hune --version` and `hune -V` are aliases of `hune version`.
- `hune healthcheck` is an alias of `hune health`. It takes the same options and gives the same exit codes.
- `hune config-reference` is an alias of `hune print-reference --format markdown`. It takes no option.
- The usage text gives the exit codes of the commands and the exit codes of `import-grafana`.
- The ops listener answers `GET /healthz` and `HEAD /healthz`. This path is an alias of `/livez`, with the same status, body and headers.
- Each answer of `server.listen` has the header `Permissions-Policy: camera=(), microphone=(), geolocation=(), payment=(), usb=()`. The ops listener does not send it.
- The image recipe `build/Dockerfile` sets the label `org.opencontainers.image.licenses` and `STOPSIGNAL SIGTERM`. SIGTERM was already the stop signal of Docker.
- The page has a skip link "Skip to content". It is the first stop of the Tab key, and it moves the focus to the main region.
- The page has the meta `color-scheme` with the value `dark light`, and the meta `robots` with the value `noindex,nofollow`.
- The page "not found" and the page of a dashboard that does not exist have a title, "Page not found" or "Dashboard not found". The browser tab shows "Page not found - Hune" or "Dashboard not found - Hune".
- The CI job `no-image` stops the CI when a file in `.github` publishes a container image or logs in to a registry. It needs no Docker and no secret. The release workflow does the same check before the build.
- The files `.editorconfig` and `.gitattributes` of the ProcurAuth family. Git stores each text file with LF line ends. Git never changes the bytes of pictures, archives, fonts and fuzz inputs.

### Changed

- The project licence is `MIT OR Apache-2.0`. The files `LICENSE-MIT` and `LICENSE-APACHE` replace `LICENSE`, and the release archive holds both. Before, the licence was Apache-2.0 only.
- The repository is `ProcurAuth/hune`. The git dependencies on `procurauth-core` and `procurauth-pep` use `https://github.com/ProcurAuth/procurauth`, at the same commit.
- The specification and the other documents are rewritten as public documents. The decision ids and the internal notes are removed. The section numbers stay the same.
- The class of the page selector of the editor lists has a new name, `flist__pages`.
- The workflow `ci` runs on a push to `main` and on each pull request. Before, it ran on a push to any branch or tag.
- The workflows pin each action to a full commit SHA, with its version in a comment. No checkout keeps the credentials in the work tree.
- Each job of the workflows `ci`, `conformance` and `release` has a time limit from 5 to 90 minutes. A job that hangs fails at this limit. Before, the GitHub limit of 360 minutes applied.
- The job `procurauth` of the workflow `conformance` runs `tests/conformance/run.sh` directly. When the script is absent, the job fails. Only the job `failover` keeps a stub.
- The release workflow stops before the build when the section of the version in `CHANGELOG.md` is absent or empty.
- The release workflow stops when it cannot read the tags of `origin`. Before, it continued as if the tag did not exist.
- `.gitignore` also ignores `.env`, `.env.*`, `*.pem`, `*.key`, `*.swp` and `.DS_Store`. The test certificates and keys in `crates/hune-source/tests/fixtures/tls/`, `tests/api/tls/` and `tests/fixtures/config/` stay in the repository.
- `SECURITY.md` has a section about two known limits (RUSTSEC-2026-0387 and one large PostgreSQL row) and a table of the public test values. Its response times are in business days.
- `CONTRIBUTING.md` and the pull request template name the job scripts of `tests/ci/` and the tools that they use on the host. They also give the command of the STE checker.
- `docs/ACCESS.md` tells that Hune sets `plain_retries = 1` in the PEP, and when the PEP sends a question to the PDP again.
- Docs: the website uses the shared ProcurAuth style sheet and script. The main menu has the same eight entries as the other sites of the family. Reference and Changelog are new in it.
- Docs: Access, Specification, Status, Conformance, Development and Chart engine left the main menu. Each of these pages marks the menu entry of its section, and the footer links to it.
- Docs: the footer has a ProcurAuth column with links to the family home page, the specification, portal and Stowr. The page 404 uses the common layout of the family.
- Docs: each local image has its width and height. The hero image loads first. The other images load when they come near the visible part of the page.
- Docs: the social preview image `docs/assets/og.png` has no colour bands now.
- Docs: the README has the section "The ProcurAuth family", with the same picture and table of parts as the other ProcurAuth repositories.
- Docs: the steps to apply the edit E25 again in `docs/engine-patches.md` are a numbered list, with one instruction in each step. The status of the applied edits is "applied, not yet reviewed".
- Tools: `tools/ste_lint.py` gives the warning S20 only for a procedural sentence. It checks the text of a block quote. A bold run-in label of 8 words or fewer does not count as a sentence.
- Tools: `scripts/check-docs.py` reports a local image that has no width and height.

### Fixed

- In the forced colors mode of the system, the mask glyphs show in the system text colour. These glyphs are the sort mark of a table, the check mark of a list and the resize mark of a panel. The type glyphs of the editor keep their own colours.
- `check-config` names `access.identity.clock_skew` for a PEP error on the clock skew. Hune sets some PEP values from constants. A PEP error on such a value is now an internal error, not an error on a key of the file. With the pinned PEP, no output changes.
- The job `import` no longer hangs on a runner. The service `plugin-fetch` ends with the code 0 at the start. `docker compose up --abort-on-container-exit` then stopped ClickHouse, 5 of the 323 expansion vectors differed, and the project did not end. The job now uses `docker compose run`, shows the output at once, and has a time limit of 25 minutes.

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
