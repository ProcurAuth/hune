// SPDX-License-Identifier: MIT OR Apache-2.0
//! The text outputs of the configuration: `print-config` (13.20), `print-reference` (13.21)
//! and the objects of the start log (13.22).

use hclconf::value::json_string;
use hclconf::{MarkdownSpec, Profile};

use super::LoadedConfig;
use super::keys::SCHEMA;

/// The format of `print-reference`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceFormat {
    /// A complete configuration file.
    Hcl,
    /// The reference document `docs/configuration-reference.md`.
    Markdown,
}

/// Write the effective configuration: one line `<key path> = <value>  # <source>` for each
/// key, in the order of the key table (13.20).
pub fn render_print_config(loaded: &LoadedConfig) -> String {
    let mut out = String::new();
    for entry in &loaded.effective {
        out.push_str(&format!("{} = {}  # {}\n", entry.key, entry.value, entry.source));
    }
    out
}

/// The JSON object of the field `config` of the start line `server.config` (13.22): one
/// member for each line of `print-config`.
pub fn start_log_config_json(loaded: &LoadedConfig) -> String {
    let members: Vec<String> = loaded.effective.iter().map(|e| format!("{}:{}", json_string(&e.key), e.json)).collect();
    format!("{{{}}}", members.join(","))
}

/// The JSON object of the field `sources` of the start line `server.config.sources`
/// (13.22): one member for each key whose source is not `file`.
pub fn start_log_sources_json(loaded: &LoadedConfig) -> String {
    let members: Vec<String> = loaded
        .effective
        .iter()
        .filter(|e| e.source != "file")
        .map(|e| format!("{}:{}", json_string(&e.key), json_string(&e.source)))
        .collect();
    format!("{{{}}}", members.join(","))
}

const INTRO: [&str; 6] = [
    "This document is the reference of the configuration keys of Hune. The command `hune print-reference --format markdown` generates it from the key table in the code. Do not edit it by hand.",
    "Hune reads one HCL file. The file is the only source of the configuration. Six keys have a default value and can be absent: `server.query_inspect` (default `false`), `server.repeat_max_per_panel` (default 24), `server.repeat_max_per_section` (default 12), `server.repeat_max_per_dashboard` (default 120), `server.compress_static` (default `true`) and `server.transform_max_cells` (default 10000000). The key `clickhouse \"<id>\".adhoc_columns` is optional and has no default: absent, no ad hoc filter applies on that source. For each other key, a key that applies is necessary and a key that does not apply is forbidden.",
    "The column Forms uses three letters. V is a value in the file. E is `<name>_env`, the name of an environment variable. F is `<name>_file`, the absolute path of a file. Give exactly one form for each key. A secret accepts only E and F.",
    "A duration is a text with one whole number and one unit: `ms`, `s`, `m` or `h`. A size is a whole number of bytes. An integer is a whole number with no sign and no zero at the start. A text is in double quotes. A list is texts in brackets with commas.",
    "The reference value is the value that the reference files use. It is not a default, except for the six keys that have a default value. Run `hune print-reference --format hcl --profile local` or `--profile distributed` to get a complete file. These two files hold no `postgres` block, no `mongodb` block and no `geomap` block. The reference value of a `postgres` key, a `mongodb` key or a `geomap` key is a value for the example block of section 13.",
    "Section 13 of the specification gives the checks after the key table, the contradictions and the warnings.",
];

/// Write a reference. `Hcl` writes the complete file of a profile (13.16, 13.17), and
/// `Markdown` writes the reference document (the profile has no effect).
pub fn render_reference(format: ReferenceFormat, profile: Profile) -> String {
    match format {
        ReferenceFormat::Hcl => {
            let first = match profile {
                Profile::Local => "Hune configuration, profile local: volume storage, access mode open, no telemetry.",
                Profile::Distributed => {
                    "Hune configuration, profile distributed: PostgreSQL storage, access mode authzen, no telemetry."
                }
            };
            hclconf::render_hcl(
                &SCHEMA,
                profile,
                &[
                    first,
                    "Each key that applies is in the file, except the six keys that have a default: `server.query_inspect`, `server.repeat_max_per_panel`, `server.repeat_max_per_section`, `server.repeat_max_per_dashboard`, `server.compress_static` and `server.transform_max_cells`.",
                ],
            )
        }
        ReferenceFormat::Markdown => {
            let spec = MarkdownSpec {
                title: "Hune configuration reference",
                intro: &INTRO,
                profile_names: ["local", "distributed"],
            };
            let mut out = hclconf::render_markdown(&SCHEMA, &spec);
            out.push('\n');
            out
        }
    }
}
