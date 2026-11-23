//! The version, the commit and the target of the binary (17.2).

/// The version of the binary.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// The commit that `build.rs` read from `HUNE_BUILD_COMMIT`, or `unknown`.
pub const COMMIT: &str = env!("HUNE_BUILD_COMMIT");
/// The target triple of the build.
pub const TARGET: &str = env!("HUNE_TARGET");

/// The commit cut to 12 characters.
pub fn short_commit() -> &'static str {
    let end = COMMIT.char_indices().nth(12).map_or(COMMIT.len(), |(i, _)| i);
    &COMMIT[..end]
}

/// The number of hexadecimal characters of the checksum in the version line of the header (9.6).
pub const CHECKSUM_CHARS: usize = 9;

/// The text of the version line of the header and its title (9.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiVersion {
    /// The line `<version>:<checksum>`, for example `0.1.0:1f2e3d4c5`.
    pub text: String,
    /// The tooltip: the full value and the source of the checksum.
    pub title: String,
}

/// True when the text is hexadecimal and has `CHECKSUM_CHARS` characters or more.
fn is_commit_hash(text: &str) -> bool {
    text.len() >= CHECKSUM_CHARS && text.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Keeps the characters that a version text can hold, so the text is safe in the JSON block of the shell.
fn clean_version(version: &str) -> String {
    version.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '+' | '-' | '_')).collect()
}

/// Makes the version line of the header (9.6) from values that the build fixed.
///
/// The checksum is the first 9 characters of the commit when the commit is a hexadecimal hash.
/// Else (a development build) it is the first 9 characters of the build hash of the web files.
/// The function reads no file and no environment variable.
pub fn ui_version(version: &str, commit: &str, build_hash: &str) -> UiVersion {
    let version = clean_version(version);
    if is_commit_hash(commit) {
        let full = commit.to_ascii_lowercase();
        return UiVersion {
            text: format!("{version}:{}", &full[..CHECKSUM_CHARS]),
            title: format!("Hune {version}, commit {full}"),
        };
    }
    let hash: String = build_hash.chars().filter(char::is_ascii_hexdigit).collect::<String>().to_ascii_lowercase();
    let short = &hash[..hash.len().min(CHECKSUM_CHARS)];
    UiVersion {
        text: format!("{version}:{short}"),
        title: format!("Hune {version}, build {hash} (the hash of the web files; this build has no commit)"),
    }
}

/// The version line of this binary (9.6).
pub fn current_ui_version() -> UiVersion {
    ui_version(VERSION, COMMIT, crate::web_assets::BUILD_HASH)
}

/// The line that `hune version` prints: `hune <version> <commit of 12 characters> <target>`.
pub fn version_line() -> String {
    format!("hune {VERSION} {} {TARGET}", short_commit())
}

#[cfg(test)]
mod tests {
    use super::{UiVersion, ui_version};

    const COMMIT: &str = "1F2e3d4c5b6a79880011223344556677889900aa";

    #[test]
    fn version_line_uses_the_first_nine_characters_of_the_commit() {
        let v = ui_version("0.1.0", COMMIT, "0123456789abcdef");
        assert_eq!(v.text, "0.1.0:1f2e3d4c5");
        assert_eq!(v.title, "Hune 0.1.0, commit 1f2e3d4c5b6a79880011223344556677889900aa");
    }

    #[test]
    fn version_line_falls_back_to_the_build_hash() {
        for commit in ["unknown", "", "abc", "1f2e3d4c5z6a"] {
            let v = ui_version("0.1.0", commit, "0123456789abcdef");
            assert_eq!(v.text, "0.1.0:012345678", "{commit}");
            assert!(v.title.contains("build 0123456789abcdef"), "{}", v.title);
            assert!(v.title.contains("no commit"), "{}", v.title);
        }
    }

    #[test]
    fn version_line_text_is_safe_in_json_and_html() {
        let v: UiVersion = ui_version("1.2.3-rc.1+x\"<b>", "unknown", "0123456789abcdef");
        for text in [&v.text, &v.title] {
            assert!(!text.contains(['"', '<', '>', '\\', '&', '/', '\n']), "{text}");
        }
        assert_eq!(v.text, "1.2.3-rc.1+xb:012345678");
    }

    #[test]
    fn version_line_has_four_words() {
        let line = super::version_line();
        assert_eq!(line.split(' ').count(), 4, "{line}");
        assert!(line.starts_with("hune "));
    }
}
