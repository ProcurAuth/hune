// SPDX-License-Identifier: MIT OR Apache-2.0
//! The optional block `geomap` (13.12c): the tile source of the panel type `geomap`.
//!
//! The block is off by default. Without it, the page contacts no tile host, and the content security policy is the
//! header of 12.8, byte for byte. With it, stage 2 parses `tile_url` one time into its parts (13.13). The source of
//! `img-src` comes from these parts only (9.4, 12.8). Thus a character outside the grammar, for example `;`, `,`, a
//! blank or a quote mark, never reaches the header.

/// The fewest bytes of `geomap.tile_url`.
pub const TILE_URL_MIN_BYTES: usize = 16;
/// The most bytes of `geomap.tile_url`.
pub const TILE_URL_MAX_BYTES: usize = 512;
/// The most characters of `geomap.attribution`.
pub const ATTRIBUTION_MAX_CHARS: usize = 200;
/// The highest value of `geomap.max_zoom`.
pub const MAX_ZOOM_BOUND: u64 = 22;
/// The sources of `img-src` that each answer has (12.8). The block adds one source after them.
pub const IMG_SRC_BASE: &str = "'self' data:";
/// The three placeholders of a tile URL. Each one occurs exactly once, in the path.
const PLACEHOLDERS: [&str; 3] = ["{z}", "{x}", "{y}"];

/// The rule of `geomap.tile_url`, as an error message gives it (13.13).
pub const TILE_URL_RULE: &str = "an https URL of 16 to 512 bytes: a lower-case DNS name with a dot (no IP address, no user info), an optional port, a path of A-Z a-z 0-9 . _ ~ - and / that holds {z}, {x} and {y} once each, and an optional query of A-Z a-z 0-9 . _ ~ = & -";
/// The rule of `geomap.attribution`, as an error message gives it (13.13).
pub const ATTRIBUTION_RULE: &str = "a text of 1 to 200 characters with no control character, no bidirectional control, no zero-width character and no line or paragraph separator";

/// A tile URL template that passed the grammar of 13.13. The parts are the parts of the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileUrl {
    /// The whole template, as the file gives it. The session gives it to the page (6.10).
    pub text: String,
    /// The host: a lower-case DNS name with a dot.
    pub host: String,
    /// The port, when the text has one.
    pub port: Option<u16>,
    /// The path, from the first `/` to the `?` or to the end. It holds the three placeholders.
    pub path: String,
    /// The fixed query after the `?`, when the text has one.
    pub query: Option<String>,
}

impl TileUrl {
    /// The origin of the tile host: `https://`, the host and the port.
    pub fn origin(&self) -> String {
        match self.port {
            Some(port) => format!("https://{}:{port}", self.host),
            None => format!("https://{}", self.host),
        }
    }

    /// The source that `img-src` gets: the origin and the path up to the last `/` before the first placeholder.
    /// The path of a source that ends with `/` is a prefix (CSP Level 3), so the source permits each tile of the
    /// template and no other path of the host.
    pub fn csp_source(&self) -> String {
        let first = PLACEHOLDERS.iter().filter_map(|p| self.path.find(p)).min().unwrap_or(self.path.len());
        let cut = self.path[..first].rfind('/').map_or(1, |i| i + 1);
        format!("{}{}", self.origin(), &self.path[..cut])
    }
}

/// The typed block `geomap` (13.12c). The configuration holds `None` when the file has no block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeomapConfig {
    /// The parsed `tile_url`.
    pub tile_url: TileUrl,
    /// The text of `attribution`. The page shows it with textContent.
    pub attribution: String,
    /// The value of `max_zoom`: 0 to 22.
    pub max_zoom: u8,
}

impl GeomapConfig {
    /// The value of the directive `img-src` with the tile source (12.8). `print-config` and the start log show it.
    pub fn img_src(&self) -> String {
        format!("{IMG_SRC_BASE} {}", self.tile_url.csp_source())
    }
}

/// True for a byte of the path outside the placeholders: `A-Z a-z 0-9 . _ ~ -` and `/`.
fn path_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'-' | b'/')
}

/// True for a byte of the query: `A-Z a-z 0-9 . _ ~ = & -`.
fn query_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'~' | b'=' | b'&' | b'-')
}

/// Checks the host: a lower-case DNS name of 1 to 253 bytes with one dot or more. A label has 1 to 63 bytes of
/// `a-z 0-9 -`, with no `-` at its start or at its end. A host whose last label holds digits only is an IPv4
/// address, or it has a top-level label that does not exist: both are refused.
fn check_host(host: &str) -> Result<(), &'static str> {
    if host.starts_with('[') {
        return Err("the host is an IP address");
    }
    if host.is_empty() || host.len() > 253 || !host.contains('.') {
        return Err("the host is not a lower-case DNS name with a dot");
    }
    let mut last = "";
    for label in host.split('.') {
        let bytes = label.as_bytes();
        let ok = !bytes.is_empty()
            && bytes.len() <= 63
            && bytes.iter().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
            && bytes[0] != b'-'
            && bytes[bytes.len() - 1] != b'-';
        if !ok {
            return Err("the host is not a lower-case DNS name with a dot");
        }
        last = label;
    }
    if last.bytes().all(|b| b.is_ascii_digit()) {
        return Err("the host is an IP address");
    }
    Ok(())
}

/// Reads a port: 1 to 65535, in decimal, with no zero at the start.
fn parse_port(text: &str) -> Result<u16, &'static str> {
    const BAD: &str = "the port is not a whole number from 1 to 65535 with no zero at the start";
    if text.is_empty() || text.len() > 5 || !text.bytes().all(|b| b.is_ascii_digit()) || text.starts_with('0') {
        return Err(BAD);
    }
    match text.parse::<u32>() {
        Ok(n @ 1..=65535) => u16::try_from(n).map_err(|_| BAD),
        _ => Err(BAD),
    }
}

/// Checks the path: it starts with `/`, it holds `{z}`, `{x}` and `{y}` exactly once each, its other bytes are
/// [`path_byte`], and it has no empty segment and no segment `.` or `..`.
fn check_path(path: &str) -> Result<(), &'static str> {
    let bytes = path.as_bytes();
    let mut counts = [0usize; 3];
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => match PLACEHOLDERS.iter().position(|p| path[i..].starts_with(p)) {
                Some(k) => {
                    counts[k] += 1;
                    i += 3;
                }
                None => return Err("the path has a brace that is not {z}, {x} or {y}"),
            },
            b'}' => return Err("the path has a brace that is not {z}, {x} or {y}"),
            b'%' => return Err("the path has a percent sign"),
            b if path_byte(b) => i += 1,
            _ => return Err("the path has a character outside A-Z a-z 0-9 . _ ~ - and /"),
        }
    }
    if counts != [1, 1, 1] {
        return Err("{z}, {x} and {y} must each occur exactly once");
    }
    if path[1..].split('/').any(|s| s.is_empty() || s == "." || s == "..") {
        return Err("the path has an empty segment, or a segment . or ..");
    }
    Ok(())
}

/// Parses `geomap.tile_url` with the grammar of 13.13. The error is the reason, for the message of stage 2.
///
/// The work is linear in the length, and the length has a limit of 512 bytes before any other step.
pub fn parse_tile_url(text: &str) -> Result<TileUrl, &'static str> {
    if !(TILE_URL_MIN_BYTES..=TILE_URL_MAX_BYTES).contains(&text.len()) {
        return Err("the length is not 16 to 512 bytes");
    }
    let Some(rest) = text.strip_prefix("https://") else {
        return Err("the scheme is not https");
    };
    if text.contains('#') {
        return Err("the URL has a fragment");
    }
    let Some(slash) = rest.find('/') else {
        return Err("the URL has no path");
    };
    let (authority, tail) = rest.split_at(slash);
    if authority.contains('@') {
        return Err("the URL has user info");
    }
    if authority.contains('{') || authority.contains('}') {
        return Err("the host has a placeholder");
    }
    let (host, port) = match authority.split_once(':') {
        Some((h, p)) => (h, Some(parse_port(p)?)),
        None => (authority, None),
    };
    check_host(host)?;
    let (path, query) = match tail.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (tail, None),
    };
    check_path(path)?;
    if let Some(q) = query
        && (q.is_empty() || !q.bytes().all(query_byte))
    {
        return Err("the query is empty, or it has a character outside A-Z a-z 0-9 . _ ~ = & -");
    }
    Ok(TileUrl {
        text: text.to_owned(),
        host: host.to_owned(),
        port,
        path: path.to_owned(),
        query: query.map(str::to_owned),
    })
}

/// True for a character that `geomap.attribution` refuses: the list of 7.37, item 9. These are the control
/// characters (U+0000 to U+001F, U+007F to U+009F), the bidirectional controls (U+202A to U+202E, U+2066 to U+2069),
/// the zero-width characters and marks (U+200B to U+200F, U+061C), U+2028, U+2029 and U+FEFF.
fn refused_char(c: char) -> bool {
    matches!(c,
        '\u{0}'..='\u{1f}'
        | '\u{7f}'..='\u{9f}'
        | '\u{202a}'..='\u{202e}'
        | '\u{2066}'..='\u{2069}'
        | '\u{200b}'..='\u{200f}'
        | '\u{61c}'
        | '\u{2028}'
        | '\u{2029}'
        | '\u{feff}')
}

/// Checks `geomap.attribution` (13.13): 1 to 200 characters, and no character of [`refused_char`].
pub fn check_attribution(text: &str) -> Result<(), &'static str> {
    let count = text.chars().count();
    if !(1..=ATTRIBUTION_MAX_CHARS).contains(&count) {
        return Err("the length is not 1 to 200 characters");
    }
    if text.chars().any(refused_char) {
        return Err("the text has a refused character");
    }
    Ok(())
}

/// The message of the warning W9 (13.15).
pub fn w9_message(tile: &TileUrl) -> String {
    format!(
        "geomap.tile_url is set: the browser of each viewer contacts {}. That host sees the address of the viewer and the area of the map.",
        tile.origin()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = "https://tiles.example.org/osm/{z}/{x}/{y}.png";

    #[test]
    fn a_valid_template_gives_its_parts_and_its_source() {
        let t = parse_tile_url(OK).unwrap();
        assert_eq!(t.host, "tiles.example.org");
        assert_eq!(t.port, None);
        assert_eq!(t.path, "/osm/{z}/{x}/{y}.png");
        assert_eq!(t.query, None);
        assert_eq!(t.origin(), "https://tiles.example.org");
        assert_eq!(t.csp_source(), "https://tiles.example.org/osm/");
        for (text, source) in [
            ("https://a.example:8443/{z}/{x}/{y}", "https://a.example:8443/"),
            ("https://a.example/t/z{z}/x{x}/y{y}.png", "https://a.example/t/"),
            ("https://a.example/v1.0/{y}/{x}/{z}.jpg?key=abc&s=1", "https://a.example/v1.0/"),
            ("https://xn--bcher-kva.example/a/b/{z}-{x}-{y}.png", "https://xn--bcher-kva.example/a/b/"),
            ("https://a.example/a{z}b/{x}/{y}", "https://a.example/"),
            ("https://t-1.a-b.example/..../{z}/{x}/{y}", "https://t-1.a-b.example/..../"),
        ] {
            let t = parse_tile_url(text).unwrap_or_else(|e| panic!("{text}: {e}"));
            assert_eq!(t.csp_source(), source, "{text}");
        }
    }

    #[test]
    fn each_form_outside_the_grammar_is_refused() {
        let long = format!("https://a.example/{}{{z}}/{{x}}/{{y}}", "a".repeat(512));
        for text in [
            "",
            "https://a.b/{z}",
            &long,
            "http://tiles.example.org/{z}/{x}/{y}.png",
            "HTTPS://tiles.example.org/{z}/{x}/{y}.png",
            "//tiles.example.org/{z}/{x}/{y}.png",
            "https://tiles.example.org",
            "https://tiles.example.org?{z}{x}{y}",
            "https://user@tiles.example.org/{z}/{x}/{y}.png",
            "https://user:pw@tiles.example.org/{z}/{x}/{y}.png",
            "https://192.168.1.10/{z}/{x}/{y}.png",
            "https://127.0.0.1:8443/{z}/{x}/{y}.png",
            "https://[::1]/{z}/{x}/{y}.png",
            "https://[2001:db8::1]:443/{z}/{x}/{y}.png",
            "https://localhost/{z}/{x}/{y}.png",
            "https://Tiles.Example.org/{z}/{x}/{y}.png",
            "https://tiles.example.org./{z}/{x}/{y}.png",
            "https://-a.example.org/{z}/{x}/{y}.png",
            "https://a-.example.org/{z}/{x}/{y}.png",
            "https://a..example.org/{z}/{x}/{y}.png",
            "https://a_b.example.org/{z}/{x}/{y}.png",
            "https://a.example.123/{z}/{x}/{y}.png",
            "https://tiles.example.org:0/{z}/{x}/{y}.png",
            "https://tiles.example.org:08443/{z}/{x}/{y}.png",
            "https://tiles.example.org:65536/{z}/{x}/{y}.png",
            "https://tiles.example.org:/{z}/{x}/{y}.png",
            "https://tiles.example.org:a/{z}/{x}/{y}.png",
            "https://{s}.tiles.example.org/{z}/{x}/{y}.png",
            "https://tiles.example.org/{s}/{z}/{x}/{y}.png",
            "https://tiles.example.org/{z}/{x}/{-y}.png",
            "https://tiles.example.org/{z}/{x}/{y}{r}.png",
            "https://tiles.example.org/{z}/{x}/{y}/{z}.png",
            "https://tiles.example.org/{z}/{x}.png",
            "https://tiles.example.org/{Z}/{x}/{y}.png",
            "https://tiles.example.org/{z}/{x}/{y}}.png",
            "https://tiles.example.org/{{z}/{x}/{y}.png",
            "https://tiles.example.org/a%20b/{z}/{x}/{y}.png",
            "https://tiles.example.org//{z}/{x}/{y}.png",
            "https://tiles.example.org/./{z}/{x}/{y}.png",
            "https://tiles.example.org/../{z}/{x}/{y}.png",
            "https://tiles.example.org/{z}/{x}/{y}/",
            "https://tiles.example.org/{z}/{x}/{y}.png#frag",
            "https://tiles.example.org/{z}/{x}/{y}.png?",
            "https://tiles.example.org/{z}/{x}/{y}.png?k={z}",
            "https://tiles.example.org/{z}/{x}/{y}.png?a=b c",
            "https://tiles.example.org/{z}/{x}/{y}.png?a=b;c",
            "https://tiles.example.org/{z}/{x}/{y}.png?a=b,c",
            "https://tiles.example.org/{z}/{x}/{y}.png?a='b'",
            "https://tiles.example.org/{z}/{x}/{y}.png?a=\"b\"",
            "https://tiles.example.org/{z}/{x}/{y}.png?a=%41",
            "https://tiles.example.org/{z}/{x}/{y}.png?a=b?c",
            "https://tiles.example.org/{z};/{x}/{y}.png",
            "https://tiles.example.org/{z},/{x}/{y}.png",
            "https://tiles.example.org/a b/{z}/{x}/{y}.png",
            "https://tiles.example.org/'self'/{z}/{x}/{y}.png",
            "https://tiles.example.org/\"/{z}/{x}/{y}.png",
            "https://tiles.example.org/a\\b/{z}/{x}/{y}.png",
            "https://tiles.example.org/\u{e9}/{z}/{x}/{y}.png",
            "https://t\u{e9}les.example.org/{z}/{x}/{y}.png",
            "https://tiles.example.org/a\nb/{z}/{x}/{y}.png",
            "https://tiles.example.org /{z}/{x}/{y}.png",
            "https://tiles.example.org;script-src */{z}/{x}/{y}",
            "https://tiles.example.org,https://evil.example/{z}/{x}/{y}",
            "https://*.example.org/{z}/{x}/{y}.png",
            "https://tiles.example.org/*/{z}/{x}/{y}.png",
            "javascript://tiles.example.org/{z}/{x}/{y}",
            "data:text/html,{z}{x}{y}aaaaaaaaaaaaaaaaaa",
        ] {
            assert!(parse_tile_url(text).is_err(), "{text:?} must be refused");
        }
    }

    #[test]
    fn the_length_limits_are_16_and_512_bytes() {
        // The shortest valid text has 21 bytes, so the lower limit refuses only texts that fail the grammar too.
        let shortest = "https://a.e/{z}{x}{y}";
        assert_eq!(shortest.len(), 21);
        assert!(parse_tile_url(shortest).is_ok());
        assert_eq!(parse_tile_url("https://a.b/{z}"), Err("the length is not 16 to 512 bytes"));
        let base = "https://a.example/";
        let tail = "/{z}/{x}/{y}";
        let fill = TILE_URL_MAX_BYTES - base.len() - tail.len();
        let at_max = format!("{base}{}{tail}", "a".repeat(fill));
        assert_eq!(at_max.len(), TILE_URL_MAX_BYTES);
        assert!(parse_tile_url(&at_max).is_ok());
        let over = format!("{base}{}{tail}", "a".repeat(fill + 1));
        assert_eq!(over.len(), TILE_URL_MAX_BYTES + 1);
        assert_eq!(parse_tile_url(&over), Err("the length is not 16 to 512 bytes"));
    }

    #[test]
    fn the_source_never_holds_a_character_of_the_policy_syntax() {
        // Each accepted text gives a source of ASCII letters, digits, `. _ ~ - / :` only: no `;`, `,`, blank or quote.
        // A seeded walk puts each byte of 0x20 to 0x7E at each place of a valid text.
        let base = "https://ab.example:8443/x_y~z-1/{z}/{x}/{y}.png?k=v&w=1";
        let mut accepted = 0;
        for at in 8..base.len() {
            for b in 0x20u8..0x7f {
                let mut bytes = base.as_bytes().to_vec();
                bytes[at] = b;
                let Ok(text) = String::from_utf8(bytes) else { continue };
                if let Ok(t) = parse_tile_url(&text) {
                    accepted += 1;
                    let src = t.csp_source();
                    assert!(src.bytes().all(|b| b.is_ascii_alphanumeric() || b".-_~/:".contains(&b)), "{src}");
                    assert!(t.text.bytes().all(|b| !b" ;,'\"\\".contains(&b)), "{}", t.text);
                }
            }
        }
        assert!(accepted > 100, "{accepted}");
        assert!(parse_tile_url("https://a.example:8443/x_y~z-1/{z}/{x}/{y}.png?k=a;b").is_err());
    }

    #[test]
    fn the_attribution_rule() {
        for ok in ["Tiles: example.org", "\u{a9} Contributors", &"\u{e9}".repeat(200), "a", "<b>text</b> & more"] {
            assert!(check_attribution(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            &"a".repeat(201),
            "a\tb",
            "a\nb",
            "a\u{7f}",
            "a\u{85}",
            "a\u{202e}b",
            "a\u{2066}b",
            "a\u{200b}b",
            "a\u{200f}b",
            "a\u{61c}b",
            "a\u{2028}b",
            "a\u{2029}b",
            "\u{feff}a",
        ] {
            assert!(check_attribution(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn the_warning_names_the_origin_only() {
        let t = parse_tile_url("https://tiles.example.org:8443/osm/{z}/{x}/{y}.png?key=public").unwrap();
        let w = w9_message(&t);
        assert!(w.contains("https://tiles.example.org:8443."), "{w}");
        assert!(!w.contains("key=public"), "{w}");
    }

    #[test]
    fn the_work_is_linear_in_the_length() {
        // The worst text of the limit: 512 bytes of braces and slashes. Each one stops at once or moves on.
        let text = format!("https://a.example/{}", "/{".repeat(247));
        let t0 = std::time::Instant::now();
        for _ in 0..1000 {
            let _ = parse_tile_url(&text);
        }
        assert!(t0.elapsed() < std::time::Duration::from_secs(2));
    }
}
