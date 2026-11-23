// SPDX-License-Identifier: MIT OR Apache-2.0
//! The grammars of texts in a document: units (4.20), colours (4.19), link URLs (4.23), zone
//! names (4.6) and variable names (4.3).

/// The units without a text argument (4.20): the first 8 are the units of the first release, the
/// next 21 are the Grafana units that the Grafana-parity work adds, and the last 30 are the Grafana units that
/// the later options adds (P01).
const FIXED_UNITS: [&str; 59] = [
    "none",
    "short",
    "percent",
    "percentunit",
    "ms",
    "s",
    "bytes",
    "decbytes",
    "ns",
    "\u{b5}s",
    "m",
    "h",
    "d",
    "dtdurationms",
    "dtdurations",
    "bits",
    "bps",
    "Bps",
    "binBps",
    "reqps",
    "ops",
    "rps",
    "iops",
    "locale",
    "dateTimeAsIso",
    "dateTimeFromNow",
    "bool_yes_no",
    "currencyUSD",
    "currencyEUR",
    "string",
    "bool",
    "bool_on_off",
    "kbytes",
    "mbytes",
    "gbytes",
    "tbytes",
    "deckbytes",
    "decmbytes",
    "decgbytes",
    "dectbytes",
    "decbits",
    "KBs",
    "MBs",
    "GBs",
    "Kbits",
    "Mbits",
    "Gbits",
    "rowsps",
    "rowspm",
    "recps",
    "cps",
    "wps",
    "opm",
    "reqpm",
    "rpm",
    "dthms",
    "dtdhms",
    "hertz",
    "sishort",
];

/// The hues of the colour tokens (4.19).
const HUES: [&str; 6] = ["red", "orange", "yellow", "green", "blue", "purple"];

/// Tells if a text is a unit of 4.20.
///
/// The text of `suffix:` and `prefix:` has 1 to 32 characters.
pub fn is_valid_unit(s: &str) -> bool {
    if FIXED_UNITS.contains(&s) {
        return true;
    }
    s.strip_prefix("suffix:")
        .or_else(|| s.strip_prefix("prefix:"))
        .is_some_and(|t| (1..=32).contains(&t.chars().count()))
}

/// Tells if a text is a colour of 4.19: a token, a hex colour, or `rgb(...)` or `rgba(...)`.
pub fn is_valid_colour(s: &str) -> bool {
    is_token(s) || is_hex_colour(s) || is_rgb_colour(s)
}

fn is_token(s: &str) -> bool {
    if s == "text" || s == "transparent" {
        return true;
    }
    HUES.iter().any(|hue| {
        s == *hue
            || s.strip_prefix("super-light-") == Some(hue)
            || s.strip_prefix("light-") == Some(hue)
            || s.strip_prefix("semi-dark-") == Some(hue)
            || s.strip_prefix("dark-") == Some(hue)
    })
}

fn is_hex_colour(s: &str) -> bool {
    match s.strip_prefix('#') {
        Some(h) => matches!(h.len(), 3 | 6 | 8) && h.bytes().all(|b| b.is_ascii_hexdigit()),
        None => false,
    }
}

/// An integer from 0 to 255 with no `0` before another digit.
fn is_channel(s: &str) -> bool {
    !s.is_empty()
        && s.bytes().all(|b| b.is_ascii_digit())
        && (s == "0" || !s.starts_with('0'))
        && s.parse::<u16>().is_ok_and(|v| v <= 255)
}

/// `0`, `1`, or `0.` followed by 1 to 3 digits.
fn is_alpha(s: &str) -> bool {
    match s {
        "0" | "1" => true,
        _ => s.strip_prefix("0.").is_some_and(|d| (1..=3).contains(&d.len()) && d.bytes().all(|b| b.is_ascii_digit())),
    }
}

fn is_rgb_colour(s: &str) -> bool {
    if let Some(inner) = s.strip_prefix("rgb(").and_then(|r| r.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(", ").collect();
        return parts.len() == 3 && parts.iter().all(|p| is_channel(p));
    }
    if let Some(inner) = s.strip_prefix("rgba(").and_then(|r| r.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(", ").collect();
        return parts.len() == 4
            && parts.iter().take(3).all(|p| is_channel(p))
            && parts.get(3).is_some_and(|p| is_alpha(p));
    }
    false
}

/// Tells if the prefix of a link URL is permitted (4.23).
///
/// The prefix is the text before the first `${`, or all the text. It MUST be empty, or start
/// with `/`, `?`, `#`, `http://` or `https://`. It MUST NOT start with `//` or `/\`.
pub fn is_valid_url_prefix(url: &str) -> bool {
    let prefix = url.split("${").next().unwrap_or_default();
    if prefix.is_empty() {
        return true;
    }
    if prefix.starts_with("//") || prefix.starts_with("/\\") {
        return false;
    }
    ["/", "?", "#", "http://", "https://"].iter().any(|p| prefix.starts_with(p))
}

/// Tells if a text is a zone name (4.6): `area "/" place [ "/" place ]`, 3 to 64 bytes.
///
/// Each part starts with a letter and holds letters, digits, `_`, `+` and `-`. The check does
/// not use a list of zones. The texts `browser` and `utc` are not names.
pub fn is_valid_zone_name(s: &str) -> bool {
    if !(3..=crate::limits::MAX_TIMEZONE_BYTES).contains(&s.len()) {
        return false;
    }
    let parts: Vec<&str> = s.split('/').collect();
    if !(2..=3).contains(&parts.len()) {
        return false;
    }
    parts.iter().all(|part| {
        let b = part.as_bytes();
        b.first().is_some_and(u8::is_ascii_alphabetic)
            && b.iter().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'+' | b'-'))
    })
}

/// Tells if a text is a variable name (4.3): `^[A-Za-z_][A-Za-z0-9_]{0,63}$`, and not `__` at
/// the start.
pub fn is_valid_variable_name(s: &str) -> bool {
    let b = s.as_bytes();
    if b.is_empty() || b.len() > 64 || s.starts_with("__") {
        return false;
    }
    (b[0].is_ascii_alphabetic() || b[0] == b'_') && b.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_')
}

/// Tells if a text is the ref of a query (4.3): `^[A-Za-z][A-Za-z0-9_]{0,15}$`.
pub fn is_valid_query_ref(s: &str) -> bool {
    let b = s.as_bytes();
    if b.is_empty() || b.len() > 16 {
        return false;
    }
    b[0].is_ascii_alphabetic() && b.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_')
}

/// Tells if a text matches `^-?[0-9]+(\.[0-9]+)?$` (the default of a `text` variable with
/// `number`, 4.10).
pub fn is_decimal_number(s: &str) -> bool {
    let s = s.strip_prefix('-').unwrap_or(s);
    let (int, frac) = match s.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (s, None),
    };
    let digits = |t: &str| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit());
    digits(int) && frac.is_none_or(digits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units() {
        for u in FIXED_UNITS {
            assert!(is_valid_unit(u));
        }
        assert!(is_valid_unit("suffix: pts"));
        assert!(is_valid_unit("prefix:$"));
        assert!(is_valid_unit(&format!("suffix:{}", "x".repeat(32))));
        for u in ["", "suffix:", "prefix:", "Percent", "kbits", "suffix", &format!("suffix:{}", "x".repeat(33))] {
            assert!(!is_valid_unit(u), "{u}");
        }
    }

    #[test]
    fn the_units_of_wave_h_and_the_closed_list() {
        let added = [
            "string",
            "bool",
            "bool_on_off",
            "kbytes",
            "mbytes",
            "gbytes",
            "tbytes",
            "deckbytes",
            "decmbytes",
            "decgbytes",
            "dectbytes",
            "decbits",
            "KBs",
            "MBs",
            "GBs",
            "Kbits",
            "Mbits",
            "Gbits",
            "rowsps",
            "rowspm",
            "recps",
            "cps",
            "wps",
            "opm",
            "reqpm",
            "rpm",
            "dthms",
            "dtdhms",
            "hertz",
            "sishort",
        ];
        assert_eq!(added.len(), 30);
        assert_eq!(FIXED_UNITS[29..], added);
        for u in added {
            assert!(is_valid_unit(u), "{u}");
        }
        let mut sorted = FIXED_UNITS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), FIXED_UNITS.len(), "a unit id shows two times");
        // Grafana ids and forms outside 4.20, and near misses of the new ids: the list stays closed.
        for u in [
            "Tbits",
            "TBs",
            "pbytes",
            "decpbytes",
            "pixel",
            "pps",
            "cpm",
            "bool:on/off",
            "si:B",
            "count:rows",
            "currency:CHF",
            "time:YYYY-MM",
            "String",
            "Bool",
            "bool_true_false",
            "KBS",
            "kBs",
            "kbits",
            "Hertz",
            "dtdhms ",
            " dthms",
            "rows/s",
        ] {
            assert!(!is_valid_unit(u), "{u}");
        }
    }

    #[test]
    fn colours() {
        let mut tokens = vec!["text".to_owned(), "transparent".to_owned()];
        for h in HUES {
            for p in ["super-light-", "light-", "", "semi-dark-", "dark-"] {
                tokens.push(format!("{p}{h}"));
            }
        }
        assert_eq!(tokens.len(), 32);
        for t in &tokens {
            assert!(is_valid_colour(t), "{t}");
        }
        for ok in [
            "#abc",
            "#ABCDEF",
            "#aabbccdd",
            "rgb(0, 255, 7)",
            "rgba(1, 2, 3, 0)",
            "rgba(1, 2, 3, 1)",
            "rgba(1, 2, 3, 0.125)",
        ] {
            assert!(is_valid_colour(ok), "{ok}");
        }
        for bad in [
            "",
            "Red",
            "light-text",
            "super-light-transparent",
            "#ab",
            "#abcd",
            "#abcdefg",
            "abc",
            "rgb(256, 0, 0)",
            "rgb(01, 0, 0)",
            "rgb(1,2,3)",
            "rgb(1, 2,3)",
            "rgb(1, 2, 3, 4)",
            "rgba(1, 2, 3)",
            "rgba(1, 2, 3, 0.1234)",
            "rgba(1, 2, 3, 2)",
            "rgba(1, 2, 3, .5)",
            "rgba(1, 2, 3, 0.)",
            "rgb( 1, 2, 3)",
            "rgb(1, 2, 3) ",
            "rgb(-1, 2, 3)",
        ] {
            assert!(!is_valid_colour(bad), "{bad}");
        }
    }

    #[test]
    fn url_prefix() {
        for ok in ["", "/d/x", "?a=1", "#top", "http://a", "https://a/${x}", "${__value.raw}", "${a}/b", "/"] {
            assert!(is_valid_url_prefix(ok), "{ok}");
        }
        for bad in
            ["//evil.example", "/\\evil.example", "javascript:alert(1)", "ftp://a", "a/b", "http${x}", "mailto:a@b"]
        {
            assert!(!is_valid_url_prefix(bad), "{bad}");
        }
    }

    #[test]
    fn names() {
        for ok in ["a", "_a", "A1_", &"a".repeat(64), "_x", "a__b"] {
            assert!(is_valid_variable_name(ok), "{ok}");
        }
        for bad in ["", "1a", "__a", "__", "a-b", "a b", &"a".repeat(65)] {
            assert!(!is_valid_variable_name(bad), "{bad}");
        }
    }

    #[test]
    fn decimals() {
        for ok in ["0", "-1", "12.5", "-0.25", "007"] {
            assert!(is_decimal_number(ok), "{ok}");
        }
        for bad in ["", "-", "1.", ".5", "1e3", "+1", "1,5", "--1", "1.2.3"] {
            assert!(!is_decimal_number(bad), "{bad}");
        }
    }
}
