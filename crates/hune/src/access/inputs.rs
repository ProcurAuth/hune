// SPDX-License-Identifier: MIT OR Apache-2.0
//! The values that build the PEP and the action document (11.10, 11.18, 13.13).
//!
//! One structure, `PepInputs`, holds these values. Stage 2 of the configuration checks builds it from
//! the checked keys and calls `check`. The server builds it from the typed `Config` and calls
//! `build_pep_config` and `build_document`. Both paths use the same functions, so the check at
//! `check-config` finds each error that the start of `serve` would find. No function here uses the
//! network.

use std::time::Duration;

use procurauth_core::jws::{KeySet, PublicKey};
use procurauth_pep::config::{PdpConfig, PepConfig, Secret};
use procurauth_pep::document::{ActionDocument, ActionDocumentBuilder};

use hune_source::SourceKind;

use crate::config::{AccessConfig, Config, ParsedKey, parse_public_key_set};

/// The kept answers: 16384 entries at most (11.10, 11.31).
pub const CACHE_MAX_ENTRIES: usize = 16_384;
/// The most questions of one display call (11.10).
pub const DISPLAY_MAX_QUESTIONS: usize = 200;
/// The largest answer of the PDP, in bytes (11.12, 11.31).
pub const MAX_RESPONSE_BYTES: u64 = 262_144;
/// The idle time of a connection to the PDP (11.10, 11.12).
pub const IDLE_CLOSE_AFTER: Duration = Duration::from_secs(5);

/// One `clickhouse`, `postgres` or `mongodb` block, as far as the PEP needs it. The PDP never gets the kind
/// (11.33): the kind only names the key of a configuration error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceInfo {
    /// The kind of the block.
    pub kind: SourceKind,
    /// The label of the block: the source id.
    pub id: String,
    /// The label in the page.
    pub label: String,
    /// The key `max_result_rows`.
    pub max_rows: u64,
    /// The key `max_execution_time`, in whole seconds.
    pub max_seconds: u64,
}

/// The values of the blocks `access`, `server`, `clickhouse`, `postgres` and `mongodb` that the PEP needs.
#[derive(Debug, Clone)]
pub struct PepInputs {
    /// `access.identity.audience`: the service id.
    pub audience: String,
    /// `access.identity.header`.
    pub header: String,
    /// `access.identity.issuer`.
    pub issuer: String,
    /// The keys of `access.identity.public_keys`.
    pub keys: Vec<ParsedKey>,
    /// `access.identity.max_age`, in seconds.
    pub max_age_s: u64,
    /// `access.identity.clock_skew`, in seconds.
    pub clock_skew_s: u64,
    /// `access.pdp.url`.
    pub pdp_url: String,
    /// The PDP token.
    pub pdp_token: Secret,
    /// `access.pdp.plain_http`.
    pub plain_http: bool,
    /// `access.pdp.timeout`.
    pub timeout: Duration,
    /// `access.pdp.cache_ttl`.
    pub cache_ttl: Duration,
    /// `access.pdp.max_connections`.
    pub max_connections: usize,
    /// `server.editing`.
    pub editing: bool,
    /// The `clickhouse` blocks, the `postgres` blocks and then the `mongodb` blocks, each in the order of the file.
    pub sources: Vec<SourceInfo>,
}

impl PepInputs {
    /// Builds the inputs from the typed configuration of the mode `authzen`. It gives `None` in the
    /// mode `open`. The error is the text of the first fault of the keys (a fault that stage 2
    /// would have found).
    pub fn from_config(config: &Config) -> Result<Option<PepInputs>, String> {
        let AccessConfig::Authzen(a) = &config.access else { return Ok(None) };
        let keys =
            parse_public_key_set(&a.identity.public_keys).map_err(|e| format!("access.identity.public_keys: {e}"))?;
        Ok(Some(PepInputs {
            audience: a.identity.audience.clone(),
            header: a.identity.header.clone(),
            issuer: a.identity.issuer.clone(),
            keys,
            max_age_s: a.identity.max_age.as_secs(),
            clock_skew_s: a.identity.clock_skew.as_secs(),
            pdp_url: a.pdp.url.clone(),
            pdp_token: Secret::new(a.pdp.token.expose().to_owned()),
            plain_http: a.pdp.plain_http,
            timeout: a.pdp.timeout,
            cache_ttl: a.pdp.cache_ttl,
            max_connections: a.pdp.max_connections as usize,
            editing: config.storage.editing(),
            sources: crate::server::source_blocks::blocks(config)
                .into_iter()
                .map(|b| SourceInfo {
                    kind: b.kind,
                    id: b.id.to_owned(),
                    label: b.label.to_owned(),
                    max_rows: b.max_result_rows,
                    max_seconds: b.max_execution_time.as_secs(),
                })
                .collect(),
        }))
    }

    /// The key set of 11.9: each key is read from its SPKI PEM block, with its `kid`.
    pub fn key_set(&self) -> Result<KeySet, String> {
        let mut keys = Vec::with_capacity(self.keys.len());
        for k in &self.keys {
            let key = PublicKey::from_spki_pem(&k.spki_pem, Some(&k.info.kid))
                .map_err(|e| format!("the key {:?} is not valid: {e:?}", k.info.kid))?;
            keys.push(key);
        }
        Ok(KeySet::from_keys(keys))
    }

    /// The `PepConfig` of 11.10. `lenient_scheme` lets an `http` URL pass with `plain_http = false`, and an `https` URL with `plain_http = true`:
    /// stage 2 uses it, because the contradiction check C5 (13.14) owns this error.
    pub fn build_pep_config(&self, lenient_scheme: bool) -> Result<PepConfig, String> {
        let mut pdp = PdpConfig::new(self.pdp_url.clone(), self.pdp_token.clone());
        // Stage 2 (`lenient_scheme`) lets both contradictions of the scheme pass, `http` with `plain_http = false`
        // and `https` with `plain_http = true`. The PEP refuses both since v1.1, and C5 owns these errors.
        pdp.allow_plain_http = if lenient_scheme {
            if self.pdp_url.starts_with("https://") {
                false
            } else {
                self.plain_http || self.pdp_url.starts_with("http://")
            }
        } else {
            self.plain_http
        };
        pdp.request_timeout = self.timeout;
        pdp.usage_wait_margin = Duration::from_secs(1);
        pdp.idle_close_after = IDLE_CLOSE_AFTER;
        pdp.max_connections = self.max_connections;
        pdp.evaluations_max_bytes = 32_768;
        pdp.evaluations_max_items = 50;
        pdp.max_response_bytes = MAX_RESPONSE_BYTES;
        pdp.plain_retries = 1;
        pdp.metadata_max_age = Duration::from_secs(3600);
        let mut config = PepConfig::new(
            self.audience.clone(),
            self.header.clone(),
            self.issuer.clone(),
            self.key_set()?,
            self.max_age_s,
            self.clock_skew_s,
            pdp,
        );
        config.cache_ttl = self.cache_ttl;
        config.cache_max_entries = CACHE_MAX_ENTRIES;
        config.display_max_questions = DISPLAY_MAX_QUESTIONS;
        Ok(config)
    }

    /// The action document of 11.18.
    pub fn build_document(&self) -> Result<ActionDocument, Vec<procurauth_core::finding::Finding>> {
        let mut builder = ActionDocumentBuilder::new(self.audience.clone())
            .resource_type("folder", "service", "One folder of dashboards, by its id.")
            .resource_type("dashboard", "folder", "One dashboard, by its uid.")
            .resource_type(
                "source",
                "service",
                "One data source of the configuration, by the id of its block.",
            )
            .action(
                "dashboard.read",
                &["service", "folder", "dashboard"],
                "List and open dashboards, run their saved queries and the queries of their variables, and read their history.",
                &["max_rows", "max_duration"],
            );
        if self.editing {
            builder = builder
                .action(
                    "dashboard.write",
                    &["service", "folder", "dashboard"],
                    "Create, change, move, restore and delete dashboards. Create, rename and delete folders.",
                    &[],
                )
                .action(
                    "source.query",
                    &["service", "source"],
                    "Run SQL that is not saved on this source, and save dashboards whose SQL uses this source.",
                    &["max_rows", "max_duration"],
                );
        }
        let max_rows = self.sources.iter().map(|s| s.max_rows).max().unwrap_or(1).max(1);
        let max_seconds = self.sources.iter().map(|s| s.max_seconds).max().unwrap_or(1).max(1);
        builder = builder
            .ceiling(
                "max_rows",
                "Most rows of one query",
                "The most rows that the result of one query can have.",
                "row",
                1,
                max_rows,
            )
            .ceiling(
                "max_duration",
                "Longest query",
                "The longest run of one query, in seconds.",
                "second",
                1,
                max_seconds,
            );
        if self.editing {
            for source in self.sorted_sources() {
                builder = builder.resource(&format!("source:{}", source.id), Some(&source.label));
            }
        }
        builder.build()
    }

    /// The sources in byte order of the id (11.18).
    pub fn sorted_sources(&self) -> Vec<&SourceInfo> {
        let mut sources: Vec<&SourceInfo> = self.sources.iter().collect();
        sources.sort_by(|a, b| a.id.as_bytes().cmp(b.id.as_bytes()));
        sources
    }

    /// The checks of stage 2 (13.13): the `PepConfig` and the action document, with no network.
    /// Each error is a key path of the configuration and a message of the form
    /// `expected <rule>, found <value>`.
    pub fn check(&self) -> Vec<(String, String)> {
        let mut errors = Vec::new();
        match self.build_pep_config(true) {
            Ok(config) => {
                if let Err(list) = config.check() {
                    for e in list {
                        let message = format!("expected {}, found {}", e.expected, e.found);
                        errors.push(match config_key_of(&e.key) {
                            Some(key) => (key.to_owned(), message),
                            // No key of the file gives this field: Hune sets it from a constant.
                            None => (
                                "access".to_owned(),
                                format!("internal error: the PEP refuses the value of Hune for `{}`: {message}", e.key),
                            ),
                        });
                    }
                }
            }
            Err(message) => errors.push(("access.identity.public_keys".to_owned(), message)),
        }
        if let Err(findings) = self.build_document() {
            let sorted = self.sorted_sources();
            for f in findings {
                let path = f.path.as_str().to_owned();
                // The finding names the resource by its index, or by its path in the message.
                let by_index =
                    path.strip_prefix("/resources/").and_then(|r| r.split('/').next()?.parse::<usize>().ok());
                let found = match by_index {
                    Some(i) => sorted.get(i).copied(),
                    None => sorted.iter().copied().find(|s| f.message.contains(&format!("\"source:{}\"", s.id))),
                };
                let key = found.map_or_else(
                    || "access.identity.audience".to_owned(),
                    |s| format!("{} \"{}\".label", s.kind.as_str(), s.id),
                );
                errors.push((
                    key,
                    format!("expected an action document that follows 5.2, 5.3 and 6.1, found {} at {path}", f.message),
                ));
            }
        }
        errors
    }
}

/// The key of 13.11 that gives a field of the `PepConfig` (13.14). `None` is a field that no key
/// of the file gives: Hune sets it from a constant, so an error on it is an internal error.
pub(super) fn config_key_of(field: &str) -> Option<&'static str> {
    Some(match field {
        "service_id" => "access.identity.audience",
        "token_header" => "access.identity.header",
        "issuer" => "access.identity.issuer",
        "trusted_keys" => "access.identity.public_keys",
        "max_token_age_seconds" => "access.identity.max_age",
        "clock_skew_seconds" => "access.identity.clock_skew",
        "cache_ttl" => "access.pdp.cache_ttl",
        "pdp.url" => "access.pdp.url",
        "pdp.credential" => "access.pdp.token",
        "pdp.allow_plain_http" => "access.pdp.plain_http",
        "pdp.request_timeout" => "access.pdp.timeout",
        "pdp.max_connections" => "access.pdp.max_connections",
        // `cache_max_entries`, `display_max_questions` and the other `pdp.*` fields.
        _ => return None,
    })
}
