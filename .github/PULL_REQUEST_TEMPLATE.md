## What changes

Describe the change in a few sentences.

## Why

Give the reason. Link the issue.

## Checks

- [ ] I ran `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings` and the tests that cover my change.
- [ ] I ran the front-end tests that cover my change (`node --test tests/frontend/<name>.test.mjs`).
- [ ] A change of the configuration changes the table of keys, and I made `docs/configuration-reference.md` again with `hune print-reference --format markdown`.
- [ ] A change of a document follows ASD-STE100. I ran `python3 -I tools/ste_lint.py <file>`, `python3 -I scripts/build-site.py` and `python3 -I scripts/check-docs.py`.
- [ ] I updated `CHANGELOG.md` under "Unreleased".
- [ ] The files have no real host name, no real address and no secret.
