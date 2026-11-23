//! The files that `hune import-grafana` writes: the report and the nginx map (15.2, 15.11, 15.17).
//!
//! This module is the one place of the command line that writes files (policy check 16.18).
//! The report holds confidential SQL (15.18). So the module refuses an output path inside a git
//! checkout. It writes each file to a temporary file in the same directory, makes the data
//! durable, and renames the temporary file onto the target. A file that exists is replaced.

use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The mode of a report file: the owner only (15.18).
const MODE_REPORT: u32 = 0o600;
/// The mode of the nginx map: the owner writes, others read. It holds no SQL.
const MODE_MAP: u32 = 0o644;

/// One output file that the command writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// The final path.
    path: PathBuf,
    /// The directory of the path, in its canonical form.
    parent: PathBuf,
}

/// Tells if a directory is in a git checkout: it holds a `.git` entry, or a directory above it does.
fn in_git_checkout(dir: &Path) -> bool {
    dir.ancestors().any(|d| fs::symlink_metadata(d.join(".git")).is_ok())
}

/// Checks an output path before the command does any work (15.2).
///
/// The parent directory must exist. The path must not be a directory. The parent directory
/// must be outside every git checkout. The message names the option and the path.
pub fn check_target(option: &str, path: &Path) -> Result<Target, String> {
    let name = path.file_name().ok_or_else(|| format!("{option}: the path {} names no file", path.display()))?;
    if name.to_string_lossy().is_empty() {
        return Err(format!("{option}: the path {} names no file", path.display()));
    }
    let parent = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let canonical = fs::canonicalize(&parent)
        .map_err(|e| format!("{option}: the parent directory {} cannot be used: {e}", parent.display()))?;
    if !canonical.is_dir() {
        return Err(format!("{option}: the parent {} is not a directory", parent.display()));
    }
    if in_git_checkout(&canonical) {
        return Err(format!(
            "{option}: the path {} is inside a git checkout; a report holds confidential SQL (15.18)",
            path.display()
        ));
    }
    let target = canonical.join(name);
    if fs::symlink_metadata(&target).is_ok_and(|m| m.is_dir()) {
        return Err(format!("{option}: the path {} is a directory", path.display()));
    }
    Ok(Target { path: target, parent: canonical })
}

/// Makes the two targets of `--report <path>`: `<path>.json` and `<path>.md`.
pub fn check_report_targets(base: &Path) -> Result<(Target, Target), String> {
    let name = base
        .file_name()
        .ok_or_else(|| format!("--report: the path {} names no file", base.display()))?
        .to_string_lossy()
        .into_owned();
    let with = |ext: &str| base.with_file_name(format!("{name}.{ext}"));
    let json = check_target("--report", &with("json"))?;
    let md = check_target("--report", &with("md"))?;
    Ok((json, md))
}

fn unique_suffix() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    format!("{}-{nanos}", std::process::id())
}

/// Writes a temporary file of the target. The caller renames it.
fn write_temp(target: &Target, bytes: &[u8], mode: u32) -> Result<PathBuf, String> {
    let name = target.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let temp = target.parent.join(format!(".{name}.tmp-{}", unique_suffix()));
    let fail = |e: std::io::Error| format!("cannot write {}: {e}", temp.display());
    let mut file = fs::OpenOptions::new().write(true).create_new(true).mode(mode).open(&temp).map_err(fail)?;
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    if let Err(e) = written {
        let _ = fs::remove_file(&temp);
        return Err(fail(e));
    }
    Ok(temp)
}

/// Writes the files at the same time: each one is complete in a temporary file before the
/// first rename. A failure removes the temporary files.
fn write_all_files(files: &[(&Target, &[u8], u32)]) -> Result<(), String> {
    let mut temps: Vec<(PathBuf, &Target)> = Vec::new();
    for (target, bytes, mode) in files {
        match write_temp(target, bytes, *mode) {
            Ok(temp) => temps.push((temp, target)),
            Err(e) => {
                for (t, _) in &temps {
                    let _ = fs::remove_file(t);
                }
                return Err(e);
            }
        }
    }
    let mut error = None;
    for (temp, target) in &temps {
        if error.is_none()
            && let Err(e) = fs::rename(temp, &target.path)
        {
            error = Some(format!("cannot replace {}: {e}", target.path.display()));
        }
        if error.is_some() {
            let _ = fs::remove_file(temp);
        }
    }
    match error {
        None => {
            for (_, target) in &temps {
                // The directory sync makes the rename durable. A failure here has no recovery.
                if let Ok(dir) = File::open(&target.parent) {
                    let _ = dir.sync_all();
                }
            }
            Ok(())
        }
        Some(e) => Err(e),
    }
}

/// Writes the report files (15.11): the JSON and the Markdown.
pub fn write_report(json: &Target, md: &Target, json_text: &str, md_text: &str) -> Result<(), String> {
    write_all_files(&[(json, json_text.as_bytes(), MODE_REPORT), (md, md_text.as_bytes(), MODE_REPORT)])
}

/// Writes the nginx map file (15.17).
pub fn write_nginx_map(target: &Target, text: &str) -> Result<(), String> {
    write_all_files(&[(target, text.as_bytes(), MODE_MAP)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_replaces_without_temporary_files() {
        let dir = tempfile::tempdir().unwrap();
        let (json, md) = check_report_targets(&dir.path().join("rep")).unwrap();
        write_report(&json, &md, "{}\n", "# one\n").unwrap();
        write_report(&json, &md, "{\"a\":1}\n", "# two\n").unwrap();
        assert_eq!(fs::read_to_string(dir.path().join("rep.json")).unwrap(), "{\"a\":1}\n");
        assert_eq!(fs::read_to_string(dir.path().join("rep.md")).unwrap(), "# two\n");
        let names: Vec<_> = fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names.len(), 2, "{names:?}");
    }

    #[test]
    fn refuses_a_missing_parent_a_directory_and_a_git_checkout() {
        let dir = tempfile::tempdir().unwrap();
        assert!(check_target("--nginx-map", &dir.path().join("no/such/file")).is_err());
        fs::create_dir(dir.path().join("sub")).unwrap();
        assert!(check_target("--nginx-map", &dir.path().join("sub")).is_err());
        fs::create_dir_all(dir.path().join("repo/.git")).unwrap();
        fs::create_dir_all(dir.path().join("repo/deep/er")).unwrap();
        let err = check_target("--nginx-map", &dir.path().join("repo/deep/er/map")).unwrap_err();
        assert!(err.contains("git checkout"), "{err}");
        assert!(check_target("--nginx-map", &dir.path().join("map")).is_ok());
    }
}
