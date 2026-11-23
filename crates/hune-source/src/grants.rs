//! The check of `SHOW GRANTS` of 8.11 and 8.12.

/// Finds the byte positions of `pat` that are outside quotes and outside parentheses.
fn top_level_positions(s: &str, pat: &str) -> Vec<usize> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    for (i, &c) in b.iter().enumerate() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            b'`' | b'\'' | b'"' => quote = Some(c),
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            _ => {}
        }
        if depth == 0 && quote.is_none() && s[i..].starts_with(pat) {
            out.push(i);
        }
    }
    out
}

/// Splits `s` at the commas that are outside quotes and parentheses.
fn split_top_level(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    for i in top_level_positions(s, ",") {
        parts.push(s[start..i].trim());
        start = i + 1;
    }
    parts.push(s[start..].trim());
    parts
}

/// Removes the quotes of an identifier.
fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('`') && s.ends_with('`') {
        s[1..s.len() - 1].replace("\\`", "`").replace("\\\\", "\\")
    } else {
        s.to_owned()
    }
}

/// Checks one line of `SHOW GRANTS`. The line MUST be a `GRANT` to the user itself.
/// It MUST hold only `SELECT` or `dictGet`, with or without a column list, and no `WITH GRANT OPTION`.
pub(crate) fn check_grant_line(line: &str, user: &str) -> Result<(), String> {
    let shown = crate::types::cut_utf8(line, 200);
    let Some(rest) = line.strip_prefix("GRANT ") else {
        return Err(format!("the line `{shown}` is not a GRANT"));
    };
    if line.trim_end().to_ascii_uppercase().ends_with("WITH GRANT OPTION") {
        return Err(format!("the line `{shown}` has WITH GRANT OPTION"));
    }
    let Some(&on) = top_level_positions(rest, " ON ").first() else {
        return Err(format!("the line `{shown}` grants a role or has no ON clause"));
    };
    for privilege in split_top_level(&rest[..on]) {
        let name = privilege.split('(').next().unwrap_or("").trim();
        if !(name.eq_ignore_ascii_case("SELECT") || name.eq_ignore_ascii_case("dictGet")) {
            return Err(format!("the line `{shown}` holds the privilege {name}"));
        }
        if privilege.contains('(') && !privilege.ends_with(')') {
            return Err(format!("the line `{shown}` has a column list that Hune cannot read"));
        }
    }
    let after_on = &rest[on + 4..];
    let Some(&to) = top_level_positions(after_on, " TO ").last() else {
        return Err(format!("the line `{shown}` has no TO clause"));
    };
    let target = unquote(&after_on[to + 4..]);
    if target != user {
        return Err(format!("the line `{shown}` is not a grant to the user of the source"));
    }
    Ok(())
}

/// Checks every line of `SHOW GRANTS`. An empty list passes: the account has no grant, and
/// the queries then give `source_access_denied`.
pub(crate) fn check_grants(lines: &[String], user: &str) -> Result<(), String> {
    lines.iter().try_for_each(|l| check_grant_line(l, user))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(line: &str) {
        assert_eq!(check_grant_line(line, "hune"), Ok(()), "{line}");
    }

    fn bad(line: &str) -> String {
        check_grant_line(line, "hune").expect_err(line)
    }

    #[test]
    fn select_and_dict_get_pass() {
        ok("GRANT SELECT ON analytics.* TO hune");
        ok("GRANT SELECT ON analytics.events TO hune");
        ok("GRANT SELECT ON *.* TO hune");
        ok("GRANT SELECT(a, b) ON db.t TO hune");
        ok("GRANT SELECT(a), dictGet ON db.t TO hune");
        ok("GRANT dictGet ON analytics.d TO hune");
        ok("GRANT SELECT ON `my db`.* TO hune");
        ok("GRANT SELECT(`a, b`) ON db.t TO `hune`");
    }

    #[test]
    fn another_privilege_fails() {
        assert!(bad("GRANT SELECT, URL ON *.* TO hune").contains("URL"));
        assert!(bad("GRANT INSERT ON db.* TO hune").contains("INSERT"));
        assert!(bad("GRANT KILL QUERY ON *.* TO hune").contains("KILL QUERY"));
        assert!(bad("GRANT ALL ON *.* TO hune").contains("ALL"));
        assert!(bad("GRANT SELECT, S3 ON *.* TO hune").contains("S3"));
    }

    #[test]
    fn roles_and_grant_option_fail() {
        bad("GRANT admin TO hune");
        assert!(bad("GRANT SELECT ON db.* TO hune WITH GRANT OPTION").contains("GRANT OPTION"));
        bad("GRANT SELECT ON db.* TO hune with grant option");
    }

    #[test]
    fn a_grant_to_another_user_or_another_statement_fails() {
        bad("GRANT SELECT ON db.* TO other");
        bad("CREATE USER hune");
        bad("");
        bad("GRANT SELECT ON db.*");
    }

    #[test]
    fn the_list_check() {
        let good = vec!["GRANT SELECT ON a.* TO hune".to_owned(), "GRANT SELECT ON b.* TO hune".to_owned()];
        assert!(check_grants(&good, "hune").is_ok());
        let mixed = vec!["GRANT SELECT ON a.* TO hune".to_owned(), "GRANT SELECT, URL ON *.* TO hune".to_owned()];
        assert!(check_grants(&mixed, "hune").is_err());
        assert!(check_grants(&[], "hune").is_ok());
    }
}
