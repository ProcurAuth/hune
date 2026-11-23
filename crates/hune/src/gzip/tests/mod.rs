//! Tests of the gzip encoder: the format, the edge cases, seeded random inputs and the embedded files.
//! The module `external` decodes the output with the system `gzip -d` and with the zlib of Node.js.

mod external;
mod measure;

use super::*;

/// A seeded generator (xorshift64*): the same seed gives the same inputs on each run.
pub(super) struct Rng(u64);

impl Rng {
    pub(super) fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    pub(super) fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    pub(super) fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// The inputs of the random tests: each kind of data that changes the choice of the encoder.
pub(super) fn random_inputs(seed: u64) -> Vec<Vec<u8>> {
    let mut rng = Rng::new(seed);
    let mut inputs = Vec::new();
    let words: [&[u8]; 12] = [
        b"const ",
        b"function ",
        b"return ",
        b"panel",
        b"(",
        b") {\n",
        b"  ",
        b"=> ",
        b"\"value\"",
        b"0.5",
        b";\n",
        b"// a comment about the frame\n",
    ];
    for kind in 0..6 {
        let len = match rng.below(4) {
            0 => rng.below(64),
            1 => rng.below(4096),
            2 => rng.below(70_000),
            _ => 60_000 + rng.below(80_000),
        };
        let mut data = Vec::with_capacity(len);
        match kind {
            // Bytes with no structure: the stored form wins.
            0 => (0..len).for_each(|_| data.push(rng.next() as u8)),
            // A small alphabet: short matches and skewed literals.
            1 => (0..len).for_each(|_| data.push(b"abcd"[rng.below(4)])),
            // Text from a word list: long matches at many distances.
            2 => {
                while data.len() < len {
                    data.extend_from_slice(words[rng.below(words.len())]);
                }
                data.truncate(len);
            }
            // Long runs of one byte.
            3 => {
                while data.len() < len {
                    let b = rng.next() as u8;
                    let run = 1 + rng.below(600);
                    data.extend(std::iter::repeat_n(b, run));
                }
                data.truncate(len);
            }
            // A block of text, then random bytes, then the same text: matches far back and a change of type.
            4 => {
                let mut part = Vec::new();
                while part.len() < len / 3 {
                    part.extend_from_slice(words[rng.below(words.len())]);
                }
                data.extend_from_slice(&part);
                (0..len / 3).for_each(|_| data.push(rng.next() as u8));
                data.extend_from_slice(&part);
            }
            // Repeats at distances near the window size.
            _ => {
                let unit: Vec<u8> = (0..32_760 + rng.below(16)).map(|_| rng.next() as u8).collect();
                data.extend_from_slice(&unit);
                data.extend_from_slice(&unit[..unit.len().min(len)]);
            }
        }
        inputs.push(data);
    }
    inputs
}

/// The fixed edge cases: empty, one byte, the stored limits and each byte value.
pub(super) fn edge_inputs() -> Vec<Vec<u8>> {
    let mut inputs = vec![Vec::new(), vec![0u8], vec![0xff], b"ab".to_vec(), b"abc".to_vec(), vec![b'x'; 258]];
    inputs.push((0..=255u8).collect());
    inputs.push((0..=255u8).cycle().take(70_000).collect());
    let mut rng = Rng::new(7);
    for len in [65_534usize, 65_535, 65_536, 65_537, 131_071] {
        inputs.push((0..len).map(|_| rng.next() as u8).collect());
    }
    inputs.push(vec![0u8; 300_000]);
    inputs
}

#[test]
fn crc32_of_the_check_string() {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    assert_eq!(crc32(b""), 0);
}

#[test]
fn the_member_has_the_header_and_the_trailer_of_rfc_1952() {
    let data = b"hune static file";
    let gz = gzip(data).unwrap();
    assert_eq!(&gz[..10], &HEADER);
    let n = gz.len();
    assert_eq!(&gz[n - 8..n - 4], &crc32(data).to_le_bytes());
    assert_eq!(&gz[n - 4..], &(data.len() as u32).to_le_bytes());
    // The empty input: the header, the one fixed block 03 00, the CRC 0 and the size 0.
    let empty = gzip(b"").unwrap();
    assert_eq!(empty.len(), 20);
    assert_eq!(&empty[10..], &[0x03, 0x00, 0, 0, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn the_output_is_deterministic() {
    for data in random_inputs(11).iter().chain(edge_inputs().iter()) {
        assert_eq!(gzip(data), gzip(data));
    }
}

#[test]
fn an_input_above_the_limit_is_refused() {
    assert!(gzip(&vec![0u8; MAX_INPUT_BYTES + 1]).is_none());
}

#[test]
fn the_output_is_small_for_data_that_repeats_and_bounded_for_random_data() {
    // Random bytes: the stored form. A block holds about 16384 bytes and costs 5 or 6 bytes more.
    let mut rng = Rng::new(3);
    let noise: Vec<u8> = (0..200_000).map(|_| rng.next() as u8).collect();
    let gz = gzip(&noise).unwrap();
    assert!(gz.len() <= noise.len() + noise.len() / 1000 + 64, "{}", gz.len());
    let zeros = gzip(&vec![0u8; 300_000]).unwrap();
    assert!(zeros.len() < 1_000, "{}", zeros.len());
}
