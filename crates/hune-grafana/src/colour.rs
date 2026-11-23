//! Colours (4.19): the check and the nearest token for a Grafana colour that format 1 refuses.

use hune_core::document::is_valid_colour;

use crate::item::{Ctx, Fallback};
use crate::walk::Node;

/// The rows are the hues. The columns are `super-light`, `light`, the hue, `semi-dark` and `dark` (9.16).
const PALETTE: [(&str, [u32; 5]); 6] = [
    ("red", [0xFFA6B0, 0xFF7383, 0xF2495C, 0xE02F44, 0xC4162A]),
    ("orange", [0xFFCB7D, 0xFFB357, 0xFF9830, 0xFF780A, 0xFA6400]),
    ("yellow", [0xFFF899, 0xFFEE52, 0xFADE2A, 0xF2CC0C, 0xE0B400]),
    ("green", [0xC8F2C2, 0x96D98D, 0x73BF69, 0x56A64B, 0x37872D]),
    ("blue", [0xC0D8FF, 0x8AB8FF, 0x5794F2, 0x3274D9, 0x1F60C4]),
    ("purple", [0xDEB6F2, 0xCA95E5, 0xB877D9, 0xA352CC, 0x8F3BB8]),
];

fn token_name(hue: &str, column: usize) -> String {
    match column {
        0 => format!("super-light-{hue}"),
        1 => format!("light-{hue}"),
        2 => hue.to_owned(),
        3 => format!("semi-dark-{hue}"),
        _ => format!("dark-{hue}"),
    }
}

/// Reads a colour text to red, green and blue. It accepts the forms that Grafana writes.
fn parse_rgb(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let digit = |i: usize| u8::from_str_radix(&hex[i..=i], 16).ok().map(|d| d * 17);
        let pair = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        return match hex.len() {
            3 | 4 => Some((digit(0)?, digit(1)?, digit(2)?)),
            6 | 8 => Some((pair(0)?, pair(2)?, pair(4)?)),
            _ => None,
        };
    }
    let lower = s.to_ascii_lowercase();
    let inner = lower.strip_prefix("rgba(").or_else(|| lower.strip_prefix("rgb(")).and_then(|r| r.strip_suffix(')'));
    if let Some(inner) = inner {
        let parts: Vec<f64> = inner.split(',').filter_map(|p| p.trim().parse::<f64>().ok()).collect();
        if parts.len() < 3 {
            return None;
        }
        let channel = |v: f64| v.round().clamp(0.0, 255.0) as u8;
        return Some((channel(parts[0]), channel(parts[1]), channel(parts[2])));
    }
    match lower.as_str() {
        "white" => Some((255, 255, 255)),
        "black" => Some((0, 0, 0)),
        "gray" | "grey" => Some((128, 128, 128)),
        _ => None,
    }
}

/// Gives the token of the palette that is the nearest to a colour text, or `None`.
pub(crate) fn nearest_token(s: &str) -> Option<String> {
    let (r, g, b) = parse_rgb(s)?;
    let mut best: Option<(i64, String)> = None;
    for (hue, shades) in PALETTE {
        for (column, hex) in shades.iter().enumerate() {
            let (tr, tg, tb) = (i64::from(hex >> 16 & 0xFF), i64::from(hex >> 8 & 0xFF), i64::from(hex & 0xFF));
            let d = (tr - i64::from(r)).pow(2) + (tg - i64::from(g)).pow(2) + (tb - i64::from(b)).pow(2);
            if best.as_ref().is_none_or(|(bd, _)| d < *bd) {
                best = Some((d, token_name(hue, column)));
            }
        }
    }
    best.map(|(_, name)| name)
}

/// The CSS basic colour names that are not tokens of 4.19. The names `red`, `green`, `blue`, `yellow`,
/// `purple` and `orange` are tokens, so they do not need a row here.
const CSS_NAMES: [(&str, &str); 14] = [
    ("white", "#ffffff"),
    ("black", "#000000"),
    ("gray", "#808080"),
    ("grey", "#808080"),
    ("silver", "#c0c0c0"),
    ("maroon", "#800000"),
    ("fuchsia", "#ff00ff"),
    ("magenta", "#ff00ff"),
    ("lime", "#00ff00"),
    ("olive", "#808000"),
    ("navy", "#000080"),
    ("teal", "#008080"),
    ("aqua", "#00ffff"),
    ("cyan", "#00ffff"),
];

/// Gives the exact hex colour (4.19, form 2) of a CSS basic colour name, or `None`.
fn css_hex(s: &str) -> Option<&'static str> {
    let name = s.trim();
    CSS_NAMES.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, hex)| *hex)
}

/// Reads a colour member. A CSS basic colour name gives its exact hex colour.
/// Another colour that 4.19 refuses is `changed` to the nearest token.
/// A text that is no colour gives an `unsupported` item with `fallback`, and `None`.
pub(crate) fn colour(ctx: &mut Ctx, node: &Node, fallback: Fallback) -> Option<String> {
    let Some(text) = node.str() else {
        ctx.unsupported(&node.ptr, Some(node.v), "the colour must be a text", fallback);
        return None;
    };
    if is_valid_colour(text) {
        ctx.ok();
        return Some(text.to_owned());
    }
    if let Some(hex) = css_hex(text) {
        ctx.ok();
        return Some(hex.to_owned());
    }
    match nearest_token(text) {
        Some(token) => {
            ctx.changed(
                &node.ptr,
                Some(node.v),
                &format!("the colour is not a form of 4.19: the nearest token is {token}"),
            );
            Some(token)
        }
        None => {
            ctx.unsupported(
                &node.ptr,
                Some(node.v),
                "the colour is not a token, a hex colour or an rgb colour",
                fallback,
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_tokens() {
        assert_eq!(nearest_token("#F2495C").as_deref(), Some("red"));
        assert_eq!(nearest_token("rgb(115,191,105)").as_deref(), Some("green"));
        assert!(nearest_token("#fff").is_some_and(|t| t.starts_with("super-light-")));
        assert!(nearest_token("not a colour").is_none());
    }

    #[test]
    fn css_names_give_exact_hex() {
        assert_eq!(css_hex("gray"), Some("#808080"));
        assert_eq!(css_hex("Grey"), Some("#808080"));
        assert_eq!(css_hex(" WHITE "), Some("#ffffff"));
        assert_eq!(css_hex("black"), Some("#000000"));
        assert_eq!(css_hex("navy"), Some("#000080"));
        // The tokens and unknown names are not in the table.
        assert_eq!(css_hex("red"), None);
        assert_eq!(css_hex("dark-red"), None);
        assert_eq!(css_hex("rebeccapurple"), None);
        for (_, hex) in CSS_NAMES {
            assert!(is_valid_colour(hex), "{hex}");
        }
    }
}
