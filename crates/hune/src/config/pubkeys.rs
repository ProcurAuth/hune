//! Check `access.identity.public_keys` (11.9): PEM text or a JWK Set with 1 to 16 keys.
//!
//! The check has the structure rules of 11.9. It uses no signature library. It reads the
//! SPKI and the JWK members and checks the lengths. It checks that a P-256 point is on the
//! curve. It computes the RFC 7638 thumbprint for a key with no `kid`.

use serde::de::{Error as _, IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use sha2::{Digest, Sha256};

use super::b64;

/// The most keys in the text (13.25).
pub const MAX_KEYS: usize = 16;
/// The most bytes of base64 text in one PEM block (13.25).
pub const MAX_PEM_BASE64_BYTES: usize = 4096;

/// A key of the set, as `print-config` shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicKeyInfo {
    /// `Ed25519` or `ES256`.
    pub alg: &'static str,
    /// The key id.
    pub kid: String,
}

impl PublicKeyInfo {
    /// The text of `print-config`: `Ed25519 kid=<kid>`.
    pub fn display(&self) -> String {
        format!("{} kid={}", self.alg, self.kid)
    }
}

/// A key of the set with its SPKI PEM block. The PEP of the access layer reads the block with
/// `PublicKey::from_spki_pem` (11.9, 11.10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedKey {
    /// The type and the `kid` of the key.
    pub info: PublicKeyInfo,
    /// The key as one SPKI PEM block.
    pub spki_pem: String,
}

enum RawKey {
    Ed25519([u8; 32]),
    P256 { x: [u8; 32], y: [u8; 32] },
}

impl RawKey {
    fn alg(&self) -> &'static str {
        match self {
            RawKey::Ed25519(_) => "Ed25519",
            RawKey::P256 { .. } => "ES256",
        }
    }

    /// The key as a PEM block `PUBLIC KEY` with lines of 64 characters.
    fn spki_pem(&self) -> String {
        let mut der = Vec::new();
        match self {
            RawKey::Ed25519(x) => {
                der.extend_from_slice(&ED25519_SPKI_PREFIX);
                der.extend_from_slice(x);
            }
            RawKey::P256 { x, y } => {
                der.extend_from_slice(&P256_SPKI_PREFIX);
                der.extend_from_slice(x);
                der.extend_from_slice(y);
            }
        }
        let text = b64::encode_standard(&der);
        let mut out = String::from("-----BEGIN PUBLIC KEY-----\n");
        for line in text.as_bytes().chunks(64) {
            out.push_str(&String::from_utf8_lossy(line));
            out.push('\n');
        }
        out.push_str("-----END PUBLIC KEY-----\n");
        out
    }

    /// The RFC 7638 thumbprint: the SHA-256 of the required members in order, in base64url.
    fn thumbprint(&self) -> String {
        let json = match self {
            RawKey::Ed25519(x) => {
                format!("{{\"crv\":\"Ed25519\",\"kty\":\"OKP\",\"x\":\"{}\"}}", b64::encode_url(x))
            }
            RawKey::P256 { x, y } => format!(
                "{{\"crv\":\"P-256\",\"kty\":\"EC\",\"x\":\"{}\",\"y\":\"{}\"}}",
                b64::encode_url(x),
                b64::encode_url(y)
            ),
        };
        b64::encode_url(Sha256::digest(json.as_bytes()).as_slice())
    }
}

// ---- P-256 point check, with 256-bit numbers in four 64-bit limbs (little endian) ----

type U256 = [u64; 4];

const P: U256 = [0xFFFF_FFFF_FFFF_FFFF, 0x0000_0000_FFFF_FFFF, 0, 0xFFFF_FFFF_0000_0001];
const B: U256 = [0x3bce_3c3e_27d2_604b, 0x651d_06b0_cc53_b0f6, 0xb3eb_bd55_7698_86bc, 0x5ac6_35d8_aa3a_93e7];

fn from_be(bytes: &[u8; 32]) -> U256 {
    let mut out = [0u64; 4];
    for (i, limb) in out.iter_mut().enumerate() {
        let start = 24 - 8 * i;
        let mut word = [0u8; 8];
        word.copy_from_slice(&bytes[start..start + 8]);
        *limb = u64::from_be_bytes(word);
    }
    out
}

fn less(a: &U256, b: &U256) -> bool {
    for i in (0..4).rev() {
        if a[i] != b[i] {
            return a[i] < b[i];
        }
    }
    false
}

fn add_raw(a: &U256, b: &U256) -> (U256, bool) {
    let mut out = [0u64; 4];
    let mut carry = false;
    for i in 0..4 {
        let (s1, c1) = a[i].overflowing_add(b[i]);
        let (s2, c2) = s1.overflowing_add(u64::from(carry));
        out[i] = s2;
        carry = c1 || c2;
    }
    (out, carry)
}

fn sub_raw(a: &U256, b: &U256) -> (U256, bool) {
    let mut out = [0u64; 4];
    let mut borrow = false;
    for i in 0..4 {
        let (d1, b1) = a[i].overflowing_sub(b[i]);
        let (d2, b2) = d1.overflowing_sub(u64::from(borrow));
        out[i] = d2;
        borrow = b1 || b2;
    }
    (out, borrow)
}

fn add_mod(a: &U256, b: &U256) -> U256 {
    let (sum, carry) = add_raw(a, b);
    if carry || !less(&sum, &P) { sub_raw(&sum, &P).0 } else { sum }
}

fn sub_mod(a: &U256, b: &U256) -> U256 {
    let (diff, borrow) = sub_raw(a, b);
    if borrow { add_raw(&diff, &P).0 } else { diff }
}

fn mul_mod(a: &U256, b: &U256) -> U256 {
    let mut result = [0u64; 4];
    for limb in (0..4).rev() {
        for bit in (0..64).rev() {
            result = add_mod(&result, &result);
            if (b[limb] >> bit) & 1 == 1 {
                result = add_mod(&result, a);
            }
        }
    }
    result
}

/// True when (x, y) is a point of the curve P-256: both are below the prime, and
/// y^2 = x^3 - 3x + b.
fn p256_on_curve(x: &[u8; 32], y: &[u8; 32]) -> bool {
    let (x, y) = (from_be(x), from_be(y));
    if !less(&x, &P) || !less(&y, &P) {
        return false;
    }
    let x2 = mul_mod(&x, &x);
    let x3 = mul_mod(&x2, &x);
    let three_x = add_mod(&add_mod(&x, &x), &x);
    let rhs = add_mod(&sub_mod(&x3, &three_x), &B);
    mul_mod(&y, &y) == rhs
}

// ---- PEM ----

const ED25519_SPKI_PREFIX: [u8; 12] = [0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00];
const P256_SPKI_PREFIX: [u8; 27] = [
    0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a, 0x86, 0x48, 0xce,
    0x3d, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00, 0x04,
];

fn parse_spki(der: &[u8]) -> Result<RawKey, String> {
    if let Some(rest) = der.strip_prefix(&ED25519_SPKI_PREFIX) {
        let key: [u8; 32] = rest.try_into().map_err(|_| "an Ed25519 key of 32 bytes".to_string())?;
        return Ok(RawKey::Ed25519(key));
    }
    if let Some(rest) = der.strip_prefix(&P256_SPKI_PREFIX) {
        let point: [u8; 64] =
            rest.try_into().map_err(|_| "a P-256 key with an uncompressed point of 65 bytes".to_string())?;
        let mut x = [0u8; 32];
        let mut y = [0u8; 32];
        x.copy_from_slice(&point[..32]);
        y.copy_from_slice(&point[32..]);
        if !p256_on_curve(&x, &y) {
            return Err("a P-256 point on the curve".to_string());
        }
        return Ok(RawKey::P256 { x, y });
    }
    Err("an Ed25519 key or a P-256 key with an uncompressed point".to_string())
}

fn parse_pem(text: &str) -> Result<Vec<ParsedKey>, String> {
    const BEGIN: &str = "-----BEGIN PUBLIC KEY-----";
    const END: &str = "-----END PUBLIC KEY-----";
    let mut blocks: Vec<String> = Vec::new();
    let mut current: Option<String> = None;
    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        let line = line.trim();
        match &mut current {
            Some(body) => {
                if line == END {
                    blocks.push(current.take().unwrap_or_default());
                } else if line.starts_with("-----") {
                    return Err(format!("expected a PEM block PUBLIC KEY, found another boundary at line {number}"));
                } else {
                    body.push_str(line);
                    if body.len() > MAX_PEM_BASE64_BYTES {
                        return Err(format!(
                            "expected at most {MAX_PEM_BASE64_BYTES} bytes of base64 text in a PEM block, found more at line {number}"
                        ));
                    }
                }
            }
            None => {
                if line == BEGIN {
                    current = Some(String::new());
                } else if line.starts_with("-----BEGIN ") {
                    return Err(format!("expected a PEM block PUBLIC KEY, found another type at line {number}"));
                } else if !line.is_empty() {
                    return Err(format!(
                        "expected only empty lines outside the PEM blocks, found text at line {number}"
                    ));
                }
            }
        }
    }
    if current.is_some() {
        return Err("expected an end line for each PEM block, found a block with no end".to_string());
    }
    if blocks.is_empty() || blocks.len() > MAX_KEYS {
        return Err(format!("expected 1 to {MAX_KEYS} public keys, found {}", blocks.len()));
    }
    let mut keys = Vec::new();
    for (i, body) in blocks.iter().enumerate() {
        let der = b64::decode_standard(body)
            .ok_or_else(|| format!("expected base64 text in PEM block {}, found text that is not base64", i + 1))?;
        let raw =
            parse_spki(&der).map_err(|want| format!("expected {want} in PEM block {}, found another key", i + 1))?;
        keys.push(ParsedKey {
            info: PublicKeyInfo { alg: raw.alg(), kid: raw.thumbprint() },
            spki_pem: raw.spki_pem(),
        });
    }
    check_unique_kids(&keys)?;
    Ok(keys)
}

// ---- JWK Set ----

/// A JSON object whose members are kept in order. A duplicate member is an error.
struct UniqueMap(Vec<(String, serde_json::Value)>);

impl<'de> Deserialize<'de> for UniqueMap {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = UniqueMap;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a JSON object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<UniqueMap, A::Error> {
                let mut out: Vec<(String, serde_json::Value)> = Vec::new();
                while let Some((key, value)) = map.next_entry::<String, serde_json::Value>()? {
                    if out.iter().any(|(k, _)| *k == key) {
                        return Err(A::Error::custom(format!("duplicate member {key:?}")));
                    }
                    out.push((key, value));
                }
                Ok(UniqueMap(out))
            }
        }
        deserializer.deserialize_map(V)
    }
}

/// The JWK Set: the member `keys`, with a duplicate check on each object.
struct JwkSet(Option<Vec<UniqueMap>>);

impl<'de> Deserialize<'de> for JwkSet {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = JwkSet;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a JSON object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<JwkSet, A::Error> {
                let mut seen: Vec<String> = Vec::new();
                let mut keys = None;
                while let Some(name) = map.next_key::<String>()? {
                    if seen.contains(&name) {
                        return Err(A::Error::custom(format!("duplicate member {name:?}")));
                    }
                    if name == "keys" {
                        keys = Some(map.next_value::<Vec<UniqueMap>>()?);
                    } else {
                        map.next_value::<IgnoredAny>()?;
                    }
                    seen.push(name);
                }
                Ok(JwkSet(keys))
            }
        }
        deserializer.deserialize_map(V)
    }
}

const PRIVATE_MEMBERS: [&str; 8] = ["d", "p", "q", "dp", "dq", "qi", "oth", "k"];

fn member<'a>(map: &'a UniqueMap, name: &str) -> Option<&'a serde_json::Value> {
    map.0.iter().find(|(k, _)| k == name).map(|(_, v)| v)
}

fn string_member<'a>(map: &'a UniqueMap, name: &str, number: usize) -> Result<Option<&'a str>, String> {
    match member(map, name) {
        None => Ok(None),
        Some(serde_json::Value::String(s)) => Ok(Some(s.as_str())),
        Some(_) => Err(format!("expected a text for the member {name:?}, found another type in key {number}")),
    }
}

fn coordinate(map: &UniqueMap, name: &str, number: usize) -> Result<[u8; 32], String> {
    let text = string_member(map, name, number)?
        .ok_or_else(|| format!("expected the member {name:?}, found none in key {number}"))?;
    let bytes = b64::decode_url(text).ok_or_else(|| {
        format!("expected base64url text with no padding in {name:?}, found another text in key {number}")
    })?;
    bytes.try_into().map_err(|_| format!("expected 32 bytes in {name:?}, found another length in key {number}"))
}

fn parse_jwk(map: &UniqueMap, number: usize) -> Result<(RawKey, Option<String>), String> {
    for private in PRIVATE_MEMBERS {
        if member(map, private).is_some() {
            return Err(format!("expected a public key, found the private member {private:?} in key {number}"));
        }
    }
    let kty = string_member(map, "kty", number)?
        .ok_or_else(|| format!("expected the member \"kty\", found none in key {number}"))?;
    let crv = string_member(map, "crv", number)?;
    let alg = string_member(map, "alg", number)?;
    let raw = match kty {
        "OKP" => {
            if crv != Some("Ed25519") {
                return Err(format!(
                    "expected \"crv\" Ed25519 for a key of type OKP, found another value in key {number}"
                ));
            }
            if let Some(a) = alg
                && a != "Ed25519"
                && a != "EdDSA"
            {
                return Err(format!(
                    "expected \"alg\" Ed25519 or EdDSA for a key of type OKP, found {a:?} in key {number}"
                ));
            }
            RawKey::Ed25519(coordinate(map, "x", number)?)
        }
        "EC" => {
            if crv != Some("P-256") {
                return Err(format!(
                    "expected \"crv\" P-256 for a key of type EC, found another value in key {number}"
                ));
            }
            if let Some(a) = alg
                && a != "ES256"
            {
                return Err(format!("expected \"alg\" ES256 for a key of type EC, found {a:?} in key {number}"));
            }
            let x = coordinate(map, "x", number)?;
            let y = coordinate(map, "y", number)?;
            if !p256_on_curve(&x, &y) {
                return Err(format!("expected a P-256 point on the curve, found another point in key {number}"));
            }
            RawKey::P256 { x, y }
        }
        _ => return Err(format!("expected a key of type OKP or EC, found another \"kty\" in key {number}")),
    };
    let kid = match string_member(map, "kid", number)? {
        None => None,
        Some(kid) => {
            if kid.is_empty() || kid.len() > 256 || kid.chars().any(char::is_control) {
                return Err(format!(
                    "expected a \"kid\" of 1 to 256 bytes with no control character, found another text in key {number}"
                ));
            }
            Some(kid.to_string())
        }
    };
    Ok((raw, kid))
}

fn parse_jwks(text: &str) -> Result<Vec<ParsedKey>, String> {
    let mut deserializer = serde_json::Deserializer::from_str(text);
    let set = JwkSet::deserialize(&mut deserializer)
        .map_err(|e| format!("expected a JWK Set that is a JSON object, found invalid JSON: {e}"))?;
    deserializer
        .end()
        .map_err(|_| "expected a JWK Set with no data after the JSON object, found more data".to_string())?;
    let keys = set.0.ok_or_else(|| "expected a member \"keys\" with an array, found none".to_string())?;
    if keys.is_empty() || keys.len() > MAX_KEYS {
        return Err(format!("expected 1 to {MAX_KEYS} public keys, found {}", keys.len()));
    }
    let mut parsed = Vec::new();
    for (i, map) in keys.iter().enumerate() {
        let (raw, kid) = parse_jwk(map, i + 1)?;
        parsed.push(ParsedKey {
            info: PublicKeyInfo { alg: raw.alg(), kid: kid.unwrap_or_else(|| raw.thumbprint()) },
            spki_pem: raw.spki_pem(),
        });
    }
    check_unique_kids(&parsed)?;
    Ok(parsed)
}

fn check_unique_kids(keys: &[ParsedKey]) -> Result<(), String> {
    for (i, a) in keys.iter().enumerate() {
        if keys[..i].iter().any(|b| b.info.kid == a.info.kid) {
            return Err(format!("expected a different kid for each key, found the kid {:?} twice", a.info.kid));
        }
    }
    Ok(())
}

/// Check the text of `access.identity.public_keys` and give the type and the `kid` of each
/// key. The text is a JWK Set when its first character after spaces, tabs and line ends is
/// `{`. Else it is PEM (11.9). The error is a message of the form `expected <rule>, found
/// <detail>`.
pub fn parse_public_keys(text: &str) -> Result<Vec<PublicKeyInfo>, String> {
    parse_public_key_set(text).map(|keys| keys.into_iter().map(|k| k.info).collect())
}

/// Like `parse_public_keys`, and also give the SPKI PEM block of each key (11.9).
pub fn parse_public_key_set(text: &str) -> Result<Vec<ParsedKey>, String> {
    let first = text.trim_start_matches([' ', '\t', '\r', '\n']).chars().next();
    if first == Some('{') { parse_jwks(text) } else { parse_pem(text) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ED_PEM: &str = include_str!("../../../../tests/fixtures/config/ed25519.pub.pem");
    const P256_PEM: &str = include_str!("../../../../tests/fixtures/config/p256.pub.pem");
    const JWKS: &str = include_str!("../../../../tests/fixtures/config/jwks.json");

    #[test]
    fn jwk_set_with_two_keys() {
        let keys = parse_public_keys(JWKS).unwrap();
        assert_eq!(
            keys,
            vec![
                PublicKeyInfo { alg: "Ed25519", kid: "edge-2026-11".into() },
                PublicKeyInfo { alg: "ES256", kid: "edge-2026-05".into() }
            ]
        );
        assert_eq!(keys[0].display(), "Ed25519 kid=edge-2026-11");
    }

    #[test]
    fn pem_keys_get_the_thumbprint_as_kid() {
        // The expected values come from an independent computation with SHA-256 (RFC 7638).
        let ed = parse_public_keys(ED_PEM).unwrap();
        assert_eq!(ed[0].display(), "Ed25519 kid=mlz7DJFMSylgSGNaOTfcl_fJaLUDgRSufeOX_Ac06GU");
        let ec = parse_public_keys(P256_PEM).unwrap();
        assert_eq!(ec[0].display(), "ES256 kid=Yardv6N9aK5Er3R6oL1c-xMliI2pbeo2h4fpfV2ViN4");
        let both = format!("{ED_PEM}\n{P256_PEM}");
        assert_eq!(parse_public_keys(&both).unwrap().len(), 2);
    }

    #[test]
    fn jwk_without_kid_gets_the_thumbprint() {
        let jwks = r#"{"keys":[{"kty":"OKP","crv":"Ed25519","x":"fpThXDcUJ4ljJD9Giq0dZX3Qb46N4UeeJyrQSXy7y5Q"}]}"#;
        assert_eq!(parse_public_keys(jwks).unwrap()[0].kid, "mlz7DJFMSylgSGNaOTfcl_fJaLUDgRSufeOX_Ac06GU");
    }

    #[test]
    fn leading_space_before_json_is_allowed() {
        assert!(parse_public_keys(&format!(" \n\t{JWKS}")).is_ok());
    }

    fn err(text: &str) -> String {
        parse_public_keys(text).unwrap_err()
    }

    #[test]
    fn pem_refusals() {
        assert!(err("").starts_with("expected 1 to 16 public keys, found 0"));
        assert!(err(&format!("note\n{ED_PEM}")).contains("text at line 1"));
        assert!(err("-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----\n").contains("another type"));
        assert!(err(&ED_PEM.replace("MCow", "Proc-Type: 4,ENCRYPTED\nMCow")).contains("not base64"));
        assert!(err(&ED_PEM.replace("-----END PUBLIC KEY-----", "")).contains("no end"));
        // Another key: an RSA-like prefix is not accepted.
        assert!(err("-----BEGIN PUBLIC KEY-----\nMAAA\n-----END PUBLIC KEY-----\n").contains("found another key"));
        // Too many keys.
        let many = ED_PEM.repeat(17);
        assert!(err(&many).contains("found 17"));
        // More than 4096 bytes of base64.
        let long = format!("-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----\n", "A".repeat(4100));
        assert!(err(&long).contains("4096 bytes"));
        // The same key twice has the same kid.
        assert!(err(&ED_PEM.repeat(2)).contains("twice"));
    }

    #[test]
    fn pem_with_a_point_off_the_curve_is_refused() {
        // Flip the last byte of the Y coordinate of the P-256 key.
        let lines: Vec<&str> = P256_PEM.lines().collect();
        let body: String = lines[1..lines.len() - 1].concat();
        let mut der = b64::decode_standard(&body).unwrap();
        let last = der.len() - 1;
        der[last] ^= 1;
        // Re-encode with the standard alphabet by hand: use the url encoder and convert.
        let enc = b64::encode_url(&der).replace('-', "+").replace('_', "/");
        let pad = "=".repeat((4 - enc.len() % 4) % 4);
        let text = format!("-----BEGIN PUBLIC KEY-----\n{enc}{pad}\n-----END PUBLIC KEY-----\n");
        assert!(err(&text).contains("P-256 point on the curve"), "{}", err(&text));
    }

    #[test]
    fn jwk_refusals() {
        let ed = |extra: &str| {
            format!(
                r#"{{"keys":[{{"kty":"OKP","crv":"Ed25519","x":"fpThXDcUJ4ljJD9Giq0dZX3Qb46N4UeeJyrQSXy7y5Q"{extra}}}]}}"#
            )
        };
        assert!(parse_public_keys(&ed("")).is_ok());
        assert!(parse_public_keys(&ed(r#","alg":"EdDSA","use":"sig","key_ops":["verify"],"foo":1"#)).is_ok());
        assert!(err(&ed(r#","alg":"ES256""#)).contains("\"alg\" Ed25519 or EdDSA"));
        for private in ["d", "p", "q", "dp", "dq", "qi", "oth", "k"] {
            let e = err(&ed(&format!(",\"{private}\":\"AAAA\"")));
            assert!(e.contains("private member"), "{private}: {e}");
        }
        assert!(err(&ed(r#","kid":"""#)).contains("\"kid\" of 1 to 256 bytes"));
        assert!(err(&ed(r#","kid":1"#)).contains("a text for the member \"kid\""));
        assert!(err(&ed(r#","kid":"a","kid":"b""#)).contains("duplicate member"));
        assert!(err(&format!("{} x", ed(""))).contains("no data after"));
        assert!(err(r#"{"keys":[]}"#).contains("found 0"));
        assert!(err(r#"{"other":[]}"#).contains("member \"keys\""));
        assert!(err(r#"{"keys":[],"keys":[]}"#).contains("duplicate member"));
        assert!(err(r#"{"keys":[{"kty":"RSA","n":"AQAB","e":"AQAB"}]}"#).contains("OKP or EC"));
        assert!(err(r#"{"keys":[{"kty":"OKP","crv":"Ed448","x":"AAAA"}]}"#).contains("Ed25519"));
        assert!(err(r#"{"keys":[{"kty":"OKP","crv":"Ed25519","x":"AAAA"}]}"#).contains("32 bytes"));
        assert!(err(r#"{"keys":[{"kty":"OKP","crv":"Ed25519"}]}"#).contains("the member \"x\""));
        assert!(
            err(r#"{"keys":[{"kty":"OKP","crv":"Ed25519","x":"fpThXDcUJ4ljJD9Giq0dZX3Qb46N4UeeJyrQSXy7y5Q="}]}"#)
                .contains("no padding")
        );
        assert!(err(r#"{"keys":[{"crv":"Ed25519"}]}"#).contains("\"kty\""));
        assert!(err("{").contains("invalid JSON"));
        assert!(err(r#"{"keys":{}}"#).contains("invalid JSON"));
        let many = format!(
            r#"{{"keys":[{}]}}"#,
            vec![r#"{"kty":"OKP","crv":"Ed25519","x":"fpThXDcUJ4ljJD9Giq0dZX3Qb46N4UeeJyrQSXy7y5Q"}"#; 17].join(",")
        );
        assert!(err(&many).contains("found 17"));
    }

    #[test]
    fn same_kid_twice_is_refused() {
        let k = r#"{"kty":"OKP","crv":"Ed25519","x":"fpThXDcUJ4ljJD9Giq0dZX3Qb46N4UeeJyrQSXy7y5Q","kid":"a"}"#;
        let k2 = r#"{"kty":"OKP","crv":"Ed25519","x":"mlz7DJFMSylgSGNaOTfcl_fJaLUDgRSufeOX_Ac06GU","kid":"a"}"#;
        assert!(err(&format!(r#"{{"keys":[{k},{k2}]}}"#)).contains("twice"));
    }

    #[test]
    fn jwk_p256_off_the_curve_is_refused() {
        let jwks = r#"{"keys":[{"kty":"EC","crv":"P-256","x":"zLnzLhZKGQpAjaN3vyE01nakXnIMc1rEX05_bPP0qNw","y":"cIPJiE42vADZeEm9l9JxtbgM_AKlb8PZirWM27WP57w"}]}"#;
        assert!(err(jwks).contains("on the curve"));
        let alg = r#"{"keys":[{"kty":"EC","crv":"P-256","alg":"EdDSA","x":"zLnzLhZKGQpAjaN3vyE01nakXnIMc1rEX05_bPP0qNw","y":"cIPJiE42vADZeEm9l9JxtbgM_AKlb8PZirWM27WP57s"}]}"#;
        assert!(err(alg).contains("ES256"));
    }

    #[test]
    fn curve_arithmetic_rejects_values_not_below_the_prime() {
        let all_ff = [0xffu8; 32];
        assert!(!p256_on_curve(&all_ff, &all_ff));
        assert!(!p256_on_curve(&[0u8; 32], &[0u8; 32]));
    }
}
