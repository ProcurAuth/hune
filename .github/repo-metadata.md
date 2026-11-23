# Repository metadata

This file lists the settings that a maintainer applies on GitHub with `gh`. The repository files do not apply them.

## About

- Description (114 characters): `Dashboard server for ClickHouse, PostgreSQL and MongoDB. One static binary, one HCL file, SQL stays on the server.`
- Website: `https://procurauth.github.io/hune/`
- Social preview image: upload `docs/assets/og.png` (1200 by 630 pixels).

## Topics

`dashboards`, `clickhouse`, `postgresql`, `mongodb`, `grafana-alternative`, `grafana-import`, `observability`, `procurauth`, `authzen`, `self-hosted`, `rust`

## Settings

- Pages: serve the branch `main`, from the folder `/docs`.
- Enable the private reports of vulnerabilities (Settings, Code security). `SECURITY.md` and `CODE_OF_CONDUCT.md` link to them.
- Enable the alerts of Dependabot. `.github/dependabot.yml` updates the actions of the workflows.
- Protect `main`: require the checks of the workflow `ci` and the check `docs` before a merge. The release path is the pair of scripts `tests/ci/all.sh` and `tests/ci/release.sh`, so a required check is optional until the workflows run on GitHub.
- Do not publish a container image. The releases hold binaries only.

## Labels

Create these labels. The issue forms use them: `bug`, `enhancement`, `documentation`.

## Commands

```sh
gh repo edit ProcurAuth/hune \
  --description "Dashboard server for ClickHouse, PostgreSQL and MongoDB. One static binary, one HCL file, SQL stays on the server." \
  --homepage "https://procurauth.github.io/hune/" \
  --add-topic dashboards --add-topic clickhouse --add-topic postgresql --add-topic mongodb \
  --add-topic grafana-alternative --add-topic grafana-import --add-topic observability --add-topic procurauth \
  --add-topic authzen --add-topic self-hosted --add-topic rust
for label in bug enhancement documentation; do gh label create "$label" --repo ProcurAuth/hune || true; done
```
