//! The command line (3.1, 13.18 to 13.21).
//!
//! The parser is written by hand. A subcommand is one function of `run`. To add a
//! subcommand, add a variant to `Command`, a case in `parse` and a case in `run_command`.
//!
//! Exit codes (14.12): 0 success, 1 configuration error, 2 command line error.

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
\x20 serve --config <path>\n\
\x20 health --config <path>\n\
\x20 export --config <path> --to <dir>\n\
\x20 import --config <path> --from <dir>\n\
\x20 copy-store --from <path> --to <path>\n\
\x20 restore --config <path> --uid <uid> [--version <n>]\n\
\x20 migrate-documents --config <path> [--write]\n\
\x20 migrate-store --config <path>\n\
\x20 import-grafana --in <dir> --source-map <grafana-key>=<source-id> [--folders <file>] [--link-prefix <old>=<new>]\n\
\x20     [--accept <file>] [--report <path>] [--nginx-map <file>] [-c <config>] [--write [--replace]]\n\
\x20 version\n\
the short form of --config is -c";

/// A parsed command line.
#[derive(Debug, PartialEq, Eq)]
enum Command {
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
    match first.to_str() {
        Some("version") => {
            parse_options("version", rest, &Accepts { config: false, format: false, profile: false })?;
            Ok(Command::Version)
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
        Some("health") => {
            let options = parse_options("health", rest, &Accepts { config: true, format: false, profile: false })?;
            Ok(Command::Health { config: need_config("health", options)? })
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
}
