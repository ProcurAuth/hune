// SPDX-License-Identifier: MIT OR Apache-2.0
//! The command line (3.1, 13.18 to 13.21).
//!
//! The parser is written by hand. A subcommand is one function of `run`. To add a
//! subcommand, add a variant to `Command`, a case in `parse`, a case in `run_command`
//! and its name in `COMMANDS`.
//!
//! Each alias gives the same command as its subcommand:
//!
//! - `help`, `--help` and `-h` write the usage text on the standard output and exit with 0.
//!   A known command with only `--help` or `-h` after it does the same.
//! - `--version` and `-V` are aliases of `version`.
//! - `healthcheck` is an alias of `health`.
//! - `config-reference` is an alias of `print-reference --format markdown`.
//!
//! Exit codes (14.12): 0 success, 1 configuration error or failure, 2 command line error.
//! `import-grafana` has its own exit codes (15.12). The usage text gives all of them.

mod import_grafana;
mod output;
mod store_cmds;

use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;

use crate::config::{self, CapturedEnvironment, Profile, ReferenceFormat};

/// Exit code of a success.
pub const EXIT_OK: u8 = 0;
/// Exit code of a configuration error.
pub const EXIT_CONFIG: u8 = 1;
/// Exit code of a command line error.
pub const EXIT_USAGE: u8 = 2;

const USAGE: &str = "usage: hune <command> [options]\n\
commands:\n\
\x20 check-config --config <path>\n\
\x20 print-config --config <path>\n\
\x20 print-reference --format hcl --profile local|distributed\n\
\x20 print-reference --format markdown\n\
\x20 config-reference                  (the same as print-reference --format markdown)\n\
\x20 serve --config <path>\n\
\x20 health --config <path>            (also healthcheck)\n\
\x20 export --config <path> --to <dir>\n\
\x20 import --config <path> --from <dir>\n\
\x20 copy-store --from <path> --to <path>\n\
\x20 restore --config <path> --uid <uid> [--version <n>]\n\
\x20 migrate-documents --config <path> [--write]\n\
\x20 migrate-store --config <path>\n\
\x20 import-grafana --in <dir> --source-map <grafana-key>=<source-id> [--folders <file>] [--link-prefix <old>=<new>]\n\
\x20     [--accept <file>] [--report <path>] [--nginx-map <file>] [-c <config>] [--write [--replace]]\n\
\x20 version                           (also --version and -V)\n\
\x20 help                              (also --help and -h, or <command> --help)\n\
the short form of --config is -c\n\
exit codes:\n\
\x20 0  success\n\
\x20 1  a configuration error, or the command failed\n\
\x20 2  the command line is not valid\n\
exit codes of import-grafana:\n\
\x20 0  success\n\
\x20 1  an item needs acceptance and has none, or an acceptance line is stale\n\
\x20 2  a usage error or an input error\n\
\x20 3  the store refused or failed a write";

/// The names that `parse` accepts as the first argument, aliases included. The commands of
/// `store_cmds::NAMES` are also commands.
const COMMANDS: [&str; 14] = [
    "help",
    "--help",
    "-h",
    "version",
    "--version",
    "-V",
    "check-config",
    "print-config",
    "print-reference",
    "config-reference",
    "serve",
    "health",
    "healthcheck",
    "import-grafana",
];

/// Tells if `name` is a command or an alias of a command.
fn is_command(name: &str) -> bool {
    COMMANDS.contains(&name) || store_cmds::NAMES.contains(&name)
}

/// Tells if the arguments after a command are only one `--help` or one `-h`.
fn asks_for_help(rest: &[OsString]) -> bool {
    matches!(rest, [one] if one == "--help" || one == "-h")
}

/// The `Accepts` of a command that accepts no option: `version`, `help` and `config-reference`.
const ACCEPTS_NOTHING: Accepts = Accepts { config: false, format: false, profile: false };

/// A parsed command line.
#[derive(Debug, PartialEq, Eq)]
enum Command {
    Help,
    Version,
    CheckConfig { config: PathBuf },
    PrintConfig { config: PathBuf },
    PrintReference { format: ReferenceFormat, profile: Option<Profile> },
    Serve { config: PathBuf },
    Health { config: PathBuf },
    Store(store_cmds::StoreCommand),
    ImportGrafana(import_grafana::ImportCommand),
}

/// A command line that is not valid. The text is the message for the standard error.
#[derive(Debug, PartialEq, Eq)]
struct UsageError(String);

fn usage<T>(message: impl Into<String>) -> Result<T, UsageError> {
    Err(UsageError(message.into()))
}

/// The options that a subcommand reads.
#[derive(Default)]
struct Options {
    config: Option<PathBuf>,
    format: Option<String>,
    profile: Option<String>,
}

/// Which options a subcommand accepts.
struct Accepts {
    config: bool,
    format: bool,
    profile: bool,
}

fn parse_options(name: &str, args: &[OsString], accepts: &Accepts) -> Result<Options, UsageError> {
    let mut options = Options::default();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let Some(text) = arg.to_str() else {
            return usage(format!("{name}: an argument is not valid UTF-8"));
        };
        let (flag, inline) = match text.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f, Some(v.to_string())),
            _ => (text, None),
        };
        let slot_name = match flag {
            "--config" | "-c" if accepts.config => "--config",
            "--format" if accepts.format => "--format",
            "--profile" if accepts.profile => "--profile",
            _ if text.starts_with('-') => return usage(format!("{name}: unknown option {flag}")),
            _ => return usage(format!("{name}: unexpected argument {text}")),
        };
        let value = match inline {
            Some(v) => v,
            None => match iter.next() {
                Some(v) => match v.to_str() {
                    Some(v) => v.to_string(),
                    None => return usage(format!("{name}: the value of {slot_name} is not valid UTF-8")),
                },
                None => return usage(format!("{name}: the option {slot_name} needs a value")),
            },
        };
        let already = match slot_name {
            "--config" => options.config.replace(PathBuf::from(value)).is_some(),
            "--format" => options.format.replace(value).is_some(),
            _ => options.profile.replace(value).is_some(),
        };
        if already {
            return usage(format!("{name}: the option {slot_name} is given two times"));
        }
    }
    Ok(options)
}

fn need_config(name: &str, options: Options) -> Result<PathBuf, UsageError> {
    match options.config {
        Some(path) if !path.as_os_str().is_empty() => Ok(path),
        Some(_) => usage(format!("{name}: the value of --config is empty")),
        None => usage(format!("{name}: the option --config <path> is necessary")),
    }
}

fn parse(args: &[OsString]) -> Result<Command, UsageError> {
    let Some(first) = args.first() else {
        return usage("no command");
    };
    let rest = &args[1..];
    // In Hune 0.1.1, a lone `--help` or `-h` after a command was an unknown option (exit code 2).
    if first.to_str().is_some_and(is_command) && asks_for_help(rest) {
        return Ok(Command::Help);
    }
    match first.to_str() {
        Some(name @ ("help" | "--help" | "-h")) => {
            parse_options(name, rest, &ACCEPTS_NOTHING)?;
            Ok(Command::Help)
        }
        Some(name @ ("version" | "--version" | "-V")) => {
            parse_options(name, rest, &ACCEPTS_NOTHING)?;
            Ok(Command::Version)
        }
        Some("config-reference") => {
            parse_options("config-reference", rest, &ACCEPTS_NOTHING)?;
            Ok(Command::PrintReference { format: ReferenceFormat::Markdown, profile: None })
        }
        Some("check-config") => {
            let options =
                parse_options("check-config", rest, &Accepts { config: true, format: false, profile: false })?;
            Ok(Command::CheckConfig { config: need_config("check-config", options)? })
        }
        Some("print-config") => {
            let options =
                parse_options("print-config", rest, &Accepts { config: true, format: false, profile: false })?;
            Ok(Command::PrintConfig { config: need_config("print-config", options)? })
        }
        Some("serve") => {
            let options = parse_options("serve", rest, &Accepts { config: true, format: false, profile: false })?;
            Ok(Command::Serve { config: need_config("serve", options)? })
        }
        Some(name @ ("health" | "healthcheck")) => {
            let options = parse_options(name, rest, &Accepts { config: true, format: false, profile: false })?;
            Ok(Command::Health { config: need_config(name, options)? })
        }
        Some(name) if store_cmds::NAMES.contains(&name) => match store_cmds::parse(name, rest) {
            Ok(cmd) => Ok(Command::Store(cmd)),
            Err(message) => usage(message),
        },
        Some("import-grafana") => match import_grafana::parse(rest) {
            Ok(cmd) => Ok(Command::ImportGrafana(cmd)),
            Err(message) => usage(message),
        },
        Some("print-reference") => {
            let options =
                parse_options("print-reference", rest, &Accepts { config: false, format: true, profile: true })?;
            let format = match options.format.as_deref() {
                Some("hcl") => ReferenceFormat::Hcl,
                Some("markdown") => ReferenceFormat::Markdown,
                Some(other) => return usage(format!("print-reference: unknown format {other:?}; use hcl or markdown")),
                None => return usage("print-reference: the option --format is necessary"),
            };
            let profile = match options.profile.as_deref() {
                Some("local") => Some(Profile::Local),
                Some("distributed") => Some(Profile::Distributed),
                Some(other) => {
                    return usage(format!("print-reference: unknown profile {other:?}; use local or distributed"));
                }
                None => None,
            };
            match (format, profile) {
                (ReferenceFormat::Hcl, None) => {
                    usage("print-reference: the option --profile is necessary with --format hcl")
                }
                (ReferenceFormat::Markdown, Some(_)) => {
                    usage("print-reference: the option --profile does not apply to --format markdown")
                }
                _ => Ok(Command::PrintReference { format, profile }),
            }
        }
        Some(other) => usage(format!("unknown command {other:?}")),
        None => usage("the command is not valid UTF-8"),
    }
}

/// The text that `hune version` prints: `hune <version> <commit of 12 characters> <target>`
/// (17.2). The build script gives the commit and the target.
fn version_line() -> String {
    crate::server::build_info::version_line()
}

fn load_and_report(
    path: &std::path::Path,
    env: &CapturedEnvironment,
    err: &mut dyn Write,
) -> Option<config::LoadedConfig> {
    match config::load(path, env) {
        Ok(loaded) => {
            for warning in &loaded.warnings {
                // A write error on the standard error has no recovery.
                let _ = writeln!(err, "warning: {}: {}", warning.key, warning.message);
            }
            Some(loaded)
        }
        Err(errors) => {
            for error in &errors {
                let _ = writeln!(err, "{error}");
            }
            None
        }
    }
}

fn run_command(command: Command, env: &CapturedEnvironment, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    match command {
        Command::Help => {
            let _ = writeln!(out, "{USAGE}");
            EXIT_OK
        }
        Command::Version => {
            let _ = writeln!(out, "{}", version_line());
            EXIT_OK
        }
        Command::CheckConfig { config } => match load_and_report(&config, env, err) {
            Some(_) => EXIT_OK,
            None => EXIT_CONFIG,
        },
        Command::PrintConfig { config } => match load_and_report(&config, env, err) {
            Some(loaded) => {
                let _ = out.write_all(config::render_print_config(&loaded).as_bytes());
                EXIT_OK
            }
            None => EXIT_CONFIG,
        },
        Command::Serve { config } => run_serve(&config, env, err),
        Command::Health { config } => run_health(&config, env, err),
        Command::Store(cmd) => store_cmds::run(cmd, env, out, err),
        Command::ImportGrafana(cmd) => import_grafana::run(cmd, env, out, err),
        Command::PrintReference { format, profile } => {
            let text = config::render_reference(format, profile.unwrap_or(Profile::Local));
            let _ = out.write_all(text.as_bytes());
            EXIT_OK
        }
    }
}

/// Loads the file for a subcommand that writes its own log. Each error goes to the standard error.
fn load_quiet(path: &std::path::Path, env: &CapturedEnvironment, err: &mut dyn Write) -> Option<config::LoadedConfig> {
    match config::load(path, env) {
        Ok(loaded) => Some(loaded),
        Err(errors) => {
            for error in &errors {
                let _ = writeln!(err, "{error}");
            }
            None
        }
    }
}

/// `hune serve` (14.12): the start log writes the warnings as `config.warning` lines.
fn run_serve(path: &std::path::Path, env: &CapturedEnvironment, err: &mut dyn Write) -> u8 {
    let Some(loaded) = load_quiet(path, env, err) else { return EXIT_CONFIG };
    let runtime = match crate::server::build_runtime(&loaded.config) {
        Ok(r) => r,
        Err(e) => {
            let _ = writeln!(err, "hune: cannot build the runtime: {e}");
            return EXIT_CONFIG;
        }
    };
    runtime.block_on(crate::server::run_serve(loaded))
}

/// `hune health` (14.5): one `GET /livez` on `server.ops_listen`.
///
/// The command reads no `_env` and no `_file` value (13.18). It runs the checks of section 13
/// for the values that it reads, then calls `/livez` on `server.ops_listen` (14.5).
fn run_health(path: &std::path::Path, env: &config::CapturedEnvironment, err: &mut dyn Write) -> u8 {
    match config::load_with(path, env, config::ReadMode::Nothing) {
        Ok(loaded) => crate::server::run_health(loaded.config.server.ops_listen),
        Err(errors) => {
            for error in &errors {
                let _ = writeln!(err, "{error}");
            }
            EXIT_CONFIG
        }
    }
}

/// Run a command line. `args` has the arguments after the program name. The function
/// writes to `out` and `err`, and gives the exit code.
pub fn run_with<I>(args: I, env: &CapturedEnvironment, out: &mut dyn Write, err: &mut dyn Write) -> u8
where
    I: IntoIterator<Item = OsString>,
{
    let args: Vec<OsString> = args.into_iter().collect();
    match parse(&args) {
        Ok(command) => run_command(command, env, out, err),
        Err(UsageError(message)) => {
            let _ = writeln!(err, "hune: {message}");
            let _ = writeln!(err, "{USAGE}");
            EXIT_USAGE
        }
    }
}

/// Run a command line with the standard output and the standard error.
pub fn run<I>(args: I, env: &CapturedEnvironment) -> u8
where
    I: IntoIterator<Item = OsString>,
{
    // The handles lock the stream for each write only. `serve` writes its log lines on the same
    // stream for the whole run, so a lock that the command line holds would stop the log.
    let mut out = std::io::stdout();
    let mut err = std::io::stderr();
    let code = run_with(args, env, &mut out, &mut err);
    let _ = out.flush();
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    fn run_text(list: &[&str]) -> (u8, String, String) {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = run_with(args(list), &CapturedEnvironment::default(), &mut out, &mut err);
        (code, String::from_utf8(out).unwrap(), String::from_utf8(err).unwrap())
    }

    #[test]
    fn parses_the_commands() {
        assert_eq!(parse(&args(&["version"])), Ok(Command::Version));
        assert_eq!(parse(&args(&["serve", "-c", "/a"])), Ok(Command::Serve { config: "/a".into() }));
        assert_eq!(parse(&args(&["health", "--config", "/a"])), Ok(Command::Health { config: "/a".into() }));
        assert_eq!(parse(&args(&["check-config", "--config", "/a"])), Ok(Command::CheckConfig { config: "/a".into() }));
        assert_eq!(parse(&args(&["print-config", "-c", "/a"])), Ok(Command::PrintConfig { config: "/a".into() }));
        assert_eq!(parse(&args(&["print-config", "--config=/a"])), Ok(Command::PrintConfig { config: "/a".into() }));
        assert_eq!(
            parse(&args(&["print-reference", "--format", "hcl", "--profile", "local"])),
            Ok(Command::PrintReference { format: ReferenceFormat::Hcl, profile: Some(Profile::Local) })
        );
        assert_eq!(
            parse(&args(&["print-reference", "--format", "markdown"])),
            Ok(Command::PrintReference { format: ReferenceFormat::Markdown, profile: None })
        );
    }

    #[test]
    fn refuses_a_bad_command_line_with_exit_code_2() {
        let cases: &[&[&str]] = &[
            &[],
            &["unknown"],
            &["check-config"],
            &["check-config", "--config"],
            &["check-config", "--config", "/a", "--config", "/b"],
            &["check-config", "-c", "/a", "extra"],
            &["check-config", "--bogus"],
            &["check-config", "--config", ""],
            &["version", "--config", "/a"],
            &["version", "extra"],
            &["print-config"],
            &["serve"],
            &["health"],
            &["serve", "--config", "/a", "extra"],
            &["health", "--format", "hcl"],
            &["print-reference"],
            &["print-reference", "--format", "hcl"],
            &["print-reference", "--format", "json", "--profile", "local"],
            &["print-reference", "--format", "hcl", "--profile", "other"],
            &["print-reference", "--format", "markdown", "--profile", "local"],
            &["print-reference", "--format", "hcl", "--profile", "local", "--config", "/a"],
        ];
        for case in cases {
            let (code, out, err) = run_text(case);
            assert_eq!(code, 2, "{case:?}");
            assert_eq!(out, "", "{case:?}");
            assert!(err.starts_with("hune: "), "{case:?}: {err}");
            assert!(err.contains("usage: hune"), "{case:?}");
        }
    }

    #[test]
    fn version_prints_one_line() {
        let (code, out, err) = run_text(&["version"]);
        assert_eq!((code, err.as_str()), (0, ""));
        assert!(out.starts_with("hune 0.1.1 "), "{out}");
        assert_eq!(out.lines().count(), 1);
    }

    #[test]
    fn print_reference_needs_no_file() {
        let (code, out, err) = run_text(&["print-reference", "--format", "hcl", "--profile", "local"]);
        assert_eq!((code, err.as_str()), (0, ""));
        assert!(out.starts_with("# Hune configuration, profile local"));
    }

    #[test]
    fn a_missing_file_is_a_configuration_error() {
        let (code, out, err) = run_text(&["check-config", "-c", "/nonexistent/hune.hcl"]);
        assert_eq!(code, 1);
        assert_eq!(out, "");
        assert!(err.starts_with("cannot read the configuration file /nonexistent/hune.hcl"), "{err}");
    }

    #[test]
    fn help_writes_the_usage_on_the_standard_output() {
        let cases: &[&[&str]] = &[
            &["help"],
            &["--help"],
            &["-h"],
            &["help", "--help"],
            &["serve", "--help"],
            &["health", "-h"],
            &["healthcheck", "--help"],
            &["version", "-h"],
            &["-V", "--help"],
            &["print-reference", "--help"],
            &["config-reference", "-h"],
            &["import-grafana", "--help"],
            &["export", "--help"],
            &["migrate-store", "-h"],
        ];
        for case in cases {
            let (code, out, err) = run_text(case);
            assert_eq!((code, err.as_str()), (0, ""), "{case:?}");
            assert_eq!(out, format!("{USAGE}\n"), "{case:?}");
        }
    }

    #[test]
    fn help_is_strict_like_the_other_commands() {
        let cases: &[&[&str]] = &[
            &["help", "serve"],
            &["--help", "extra"],
            &["-h", "--config", "/a"],
            &["unknown", "--help"],
            &["serve", "-c", "/a", "--help"],
            &["serve", "--help", "-c", "/a"],
            &["serve", "--help=yes"],
        ];
        for case in cases {
            let (code, out, err) = run_text(case);
            assert_eq!(code, 2, "{case:?}");
            assert_eq!(out, "", "{case:?}");
            assert!(err.starts_with("hune: "), "{case:?}: {err}");
            assert!(err.contains("usage: hune"), "{case:?}");
        }
    }

    #[test]
    fn version_has_two_aliases() {
        let (_, expected, _) = run_text(&["version"]);
        for alias in ["--version", "-V"] {
            let (code, out, err) = run_text(&[alias]);
            assert_eq!((code, err.as_str()), (0, ""), "{alias}");
            assert_eq!(out, expected, "{alias}");
            let (code, out, err) = run_text(&[alias, "extra"]);
            assert_eq!((code, out.as_str()), (2, ""), "{alias}");
            assert!(err.starts_with(&format!("hune: {alias}: unexpected argument extra")), "{err}");
        }
    }

    #[test]
    fn healthcheck_is_an_alias_of_health() {
        assert_eq!(parse(&args(&["healthcheck", "-c", "/a"])), Ok(Command::Health { config: "/a".into() }));
        assert_eq!(parse(&args(&["healthcheck", "--config=/a"])), Ok(Command::Health { config: "/a".into() }));
        let (code, out, err) = run_text(&["healthcheck"]);
        assert_eq!((code, out.as_str()), (2, ""));
        assert!(err.starts_with("hune: healthcheck: the option --config <path> is necessary"), "{err}");
        let (code, out, err) = run_text(&["healthcheck", "-c", "/nonexistent/hune.hcl"]);
        assert_eq!((code, out.as_str()), (1, ""));
        assert!(err.starts_with("cannot read the configuration file /nonexistent/hune.hcl"), "{err}");
    }

    #[test]
    fn config_reference_is_print_reference_in_markdown() {
        assert_eq!(
            parse(&args(&["config-reference"])),
            Ok(Command::PrintReference { format: ReferenceFormat::Markdown, profile: None })
        );
        let (_, expected, _) = run_text(&["print-reference", "--format", "markdown"]);
        let (code, out, err) = run_text(&["config-reference"]);
        assert_eq!((code, err.as_str()), (0, ""));
        assert_eq!(out, expected);
        for extra in [&["--format", "markdown"][..], &["--profile", "local"][..], &["extra"][..]] {
            let mut case = vec!["config-reference"];
            case.extend_from_slice(extra);
            let (code, out, _) = run_text(&case);
            assert_eq!((code, out.as_str()), (2, ""), "{case:?}");
        }
    }

    #[test]
    fn the_usage_names_each_command_and_the_exit_codes() {
        for name in COMMANDS.iter().chain(store_cmds::NAMES.iter()) {
            assert!(USAGE.contains(name), "{name}");
            assert!(is_command(name), "{name}");
        }
        for line in ["  0  success", "  1  a configuration error", "  2  the command line is not valid"] {
            assert!(USAGE.contains(line), "{line}");
        }
        assert!(USAGE.contains("exit codes of import-grafana:\n"));
        assert!(USAGE.contains("  3  the store refused or failed a write"));
        assert_eq!((EXIT_OK, EXIT_CONFIG, EXIT_USAGE), (0, 1, 2));
    }
}
