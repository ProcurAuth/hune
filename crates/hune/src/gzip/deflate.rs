// SPDX-License-Identifier: MIT OR Apache-2.0
//! The DEFLATE format (RFC 1951): blocks of the three types, with the smallest type for each block.
//!
//! The LZ77 step gives at most `BLOCK_TOKENS` tokens for one block. For each block, the writer computes the
//! exact size in bits of a stored block, a block with the fixed codes and a block with dynamic codes. It
//! writes the smallest of the three. The output depends only on the input.

use super::bits::BitWriter;
use super::huffman::{code_lengths, reversed_codes};
use super::lz77::{Lz77, MAX_MATCH, MIN_MATCH, Token};

/// The most tokens in one block. zlib uses the same value at its default memory level.
const BLOCK_TOKENS: usize = 16_384;
/// The longest stored block (RFC 1951, 3.2.4).
const MAX_STORED: usize = 65_535;
/// The literal and length alphabet that a block uses: 256 literals, the end of block and 29 length codes.
const LIT_SYMBOLS: usize = 286;
/// The distance alphabet that a block uses.
const DIST_SYMBOLS: usize = 30;
/// The end of a block.
const END_OF_BLOCK: usize = 256;
/// The longest code of the literal, length and distance alphabets.
const MAX_CODE_BITS: u8 = 15;
/// The longest code of the code length alphabet.
const MAX_CL_BITS: u8 = 7;

/// The first length of each length code 257 to 285, and its number of extra bits (RFC 1951, 3.2.5).
const LEN_BASE: [u16; 29] =
    [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEN_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
/// The first distance of each distance code 0 to 29, and its number of extra bits (RFC 1951, 3.2.5).
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145,
    8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] =
    [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];
/// The order of the code length codes in the header of a dynamic block (RFC 1951, 3.2.7).
const CL_ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

/// The index of the length code (0 for code 257) of each match length. Entries below 3 are not used.
const LEN_INDEX: [u8; MAX_MATCH + 1] = {
    let mut table = [0u8; MAX_MATCH + 1];
    let mut code = 0;
    let mut len = MIN_MATCH;
    while len <= MAX_MATCH {
        while code + 1 < LEN_BASE.len() && LEN_BASE[code + 1] as usize <= len {
            code += 1;
        }
        table[len] = code as u8;
        len += 1;
    }
    table
};

/// The index of the length code of a match length (3 to 258).
fn len_index(len: usize) -> usize {
    usize::from(LEN_INDEX[len.min(MAX_MATCH)])
}

/// The distance code of a distance (1 to 32768).
fn dist_index(dist: usize) -> usize {
    DIST_BASE.partition_point(|&b| usize::from(b) <= dist).saturating_sub(1)
}

/// The code lengths of the fixed codes (RFC 1951, 3.2.6).
fn fixed_lengths() -> (Vec<u8>, Vec<u8>) {
    let mut lit = vec![8u8; 288];
    lit[144..256].fill(9);
    lit[256..280].fill(7);
    (lit, vec![5u8; DIST_SYMBOLS])
}

/// One code: the code lengths and the codes, with their bits in reverse order.
struct Code {
    lengths: Vec<u8>,
    codes: Vec<u16>,
}

impl Code {
    fn new(lengths: Vec<u8>) -> Code {
        let codes = reversed_codes(&lengths);
        Code { lengths, codes }
    }

    fn put(&self, w: &mut BitWriter, symbol: usize) {
        w.bits(u32::from(self.codes[symbol]), u32::from(self.lengths[symbol]));
    }

    /// The bits of the symbols with the frequencies `freq`, without the extra bits.
    fn cost(&self, freq: &[u32]) -> u64 {
        freq.iter().zip(&self.lengths).map(|(&f, &l)| u64::from(f) * u64::from(l)).sum()
    }
}

/// The frequencies of the symbols of one block.
struct Frequencies {
    lit: [u32; LIT_SYMBOLS],
    dist: [u32; DIST_SYMBOLS],
}

impl Frequencies {
    fn of(tokens: &[Token]) -> Frequencies {
        let mut f = Frequencies { lit: [0; LIT_SYMBOLS], dist: [0; DIST_SYMBOLS] };
        for t in tokens {
            match *t {
                Token::Literal(b) => f.lit[usize::from(b)] += 1,
                Token::Match { len, dist } => {
                    f.lit[END_OF_BLOCK + 1 + len_index(usize::from(len))] += 1;
                    f.dist[dist_index(usize::from(dist))] += 1;
                }
            }
        }
        f.lit[END_OF_BLOCK] += 1;
        f
    }

    /// The extra bits of the lengths and the distances. They are the same for the fixed and dynamic codes.
    fn extra_bits(&self) -> u64 {
        let len: u64 = LEN_EXTRA.iter().enumerate().map(|(i, &e)| u64::from(self.lit[257 + i]) * u64::from(e)).sum();
        let dist: u64 = DIST_EXTRA.iter().zip(&self.dist).map(|(&e, &f)| u64::from(f) * u64::from(e)).sum();
        len + dist
    }
}

/// The header of a dynamic block (RFC 1951, 3.2.7).
struct DynamicHeader {
    hlit: usize,
    hdist: usize,
    hclen: usize,
    /// The code lengths of the two alphabets in run-length form: (symbol 0 to 18, value of the extra bits).
    runs: Vec<(u8, u8)>,
    cl: Code,
    cl_freq: [u32; 19],
}

/// Writes the code lengths in the run-length form of RFC 1951 3.2.7: 16 repeats the last length 3 to 6
/// times, 17 writes 3 to 10 zeros and 18 writes 11 to 138 zeros. zlib writes the same form.
fn run_lengths(lengths: &[u8], out: &mut Vec<(u8, u8)>) {
    let mut i = 0;
    while i < lengths.len() {
        let cur = lengths[i];
        let run = lengths[i..].iter().take_while(|&&l| l == cur).count();
        let mut left = run;
        if cur == 0 {
            while left >= 11 {
                let k = left.min(138);
                out.push((18, (k - 11) as u8));
                left -= k;
            }
            if left >= 3 {
                out.push((17, (left - 3) as u8));
                left = 0;
            }
        } else {
            out.push((cur, 0));
            left -= 1;
            while left >= 3 {
                let k = left.min(6);
                out.push((16, (k - 3) as u8));
                left -= k;
            }
        }
        for _ in 0..left {
            out.push((cur, 0));
        }
        i += run;
    }
}

/// The number of extra bits of a code length symbol.
fn cl_extra(symbol: u8) -> u32 {
    match symbol {
        16 => 2,
        17 => 3,
        18 => 7,
        _ => 0,
    }
}

impl DynamicHeader {
    fn new(lit: &[u8], dist: &[u8]) -> DynamicHeader {
        let used = |lengths: &[u8], least: usize| lengths.iter().rposition(|&l| l != 0).map_or(0, |i| i + 1).max(least);
        let hlit = used(lit, 257);
        let hdist = used(dist, 1);
        let mut runs = Vec::new();
        run_lengths(&lit[..hlit], &mut runs);
        run_lengths(&dist[..hdist], &mut runs);
        let mut cl_freq = [0u32; 19];
        for &(s, _) in &runs {
            cl_freq[usize::from(s)] += 1;
        }
        let cl = Code::new(code_lengths(&cl_freq, MAX_CL_BITS));
        let hclen = CL_ORDER.iter().rposition(|&s| cl.lengths[s] != 0).map_or(0, |i| i + 1).max(4);
        DynamicHeader { hlit, hdist, hclen, runs, cl, cl_freq }
    }

    /// The size of the header in bits, after the three bits of the block header.
    fn bits(&self) -> u64 {
        let runs: u64 = self.cl.cost(&self.cl_freq);
        let extra: u64 = self.runs.iter().map(|&(s, _)| u64::from(cl_extra(s))).sum();
        5 + 5 + 4 + 3 * self.hclen as u64 + runs + extra
    }

    fn write(&self, w: &mut BitWriter) {
        w.bits((self.hlit - 257) as u32, 5);
        w.bits((self.hdist - 1) as u32, 5);
        w.bits((self.hclen - 4) as u32, 4);
        for &s in &CL_ORDER[..self.hclen] {
            w.bits(u32::from(self.cl.lengths[s]), 3);
        }
        for &(s, extra) in &self.runs {
            self.cl.put(w, usize::from(s));
            w.bits(u32::from(extra), cl_extra(s));
        }
    }
}

/// The size in bits of the stored form of `len` bytes, when the writer is at `offset` bits in its byte.
fn stored_bits(len: usize, offset: u32) -> u64 {
    let chunks = len.div_ceil(MAX_STORED).max(1) as u64;
    // Each chunk: 3 bits of header, the bits to the byte boundary, LEN and NLEN. The first chunk starts at
    // `offset`; each later chunk starts at a byte boundary, so its padding is 5 bits.
    let first_pad = u64::from((8 - (offset + 3) % 8) % 8);
    3 + first_pad + 32 + (chunks - 1) * (3 + 5 + 32) + 8 * len as u64
}

/// Writes the tokens of one block with a code.
fn write_tokens(w: &mut BitWriter, tokens: &[Token], lit: &Code, dist: &Code) {
    for t in tokens {
        match *t {
            Token::Literal(b) => lit.put(w, usize::from(b)),
            Token::Match { len, dist: d } => {
                let (len, d) = (usize::from(len), usize::from(d));
                let li = len_index(len);
                lit.put(w, END_OF_BLOCK + 1 + li);
                w.bits((len - usize::from(LEN_BASE[li])) as u32, u32::from(LEN_EXTRA[li]));
                let di = dist_index(d);
                dist.put(w, di);
                w.bits((d - usize::from(DIST_BASE[di])) as u32, u32::from(DIST_EXTRA[di]));
            }
        }
    }
    lit.put(w, END_OF_BLOCK);
}

/// Writes one block: `raw` is the input that `tokens` hold, and `last` sets the bit BFINAL.
fn write_block(w: &mut BitWriter, tokens: &[Token], raw: &[u8], last: bool, fixed: &(Code, Code)) {
    let freq = Frequencies::of(tokens);
    let extra = freq.extra_bits();
    let lit = Code::new(code_lengths(&freq.lit, MAX_CODE_BITS));
    let dist = Code::new(code_lengths(&freq.dist, MAX_CODE_BITS));
    let header = DynamicHeader::new(&lit.lengths, &dist.lengths);
    let dynamic_bits = 3 + header.bits() + lit.cost(&freq.lit) + dist.cost(&freq.dist) + extra;
    let fixed_bits = 3 + fixed.0.cost(&freq.lit) + fixed.1.cost(&freq.dist) + extra;
    let stored = stored_bits(raw.len(), w.bit_offset());
    let bfinal = u32::from(last);
    if stored < fixed_bits.min(dynamic_bits) {
        let mut chunks = raw.chunks(MAX_STORED).peekable();
        if chunks.peek().is_none() {
            write_stored(w, &[], bfinal);
        }
        while let Some(chunk) = chunks.next() {
            let final_chunk = last && chunks.peek().is_none();
            write_stored(w, chunk, u32::from(final_chunk));
        }
    } else if fixed_bits <= dynamic_bits {
        w.bits(bfinal, 1);
        w.bits(1, 2);
        write_tokens(w, tokens, &fixed.0, &fixed.1);
    } else {
        w.bits(bfinal, 1);
        w.bits(2, 2);
        header.write(w);
        write_tokens(w, tokens, &lit, &dist);
    }
}

/// Writes one stored block of at most 65535 bytes.
fn write_stored(w: &mut BitWriter, chunk: &[u8], bfinal: u32) {
    let len = u16::try_from(chunk.len()).unwrap_or(u16::MAX);
    w.bits(bfinal, 1);
    w.bits(0, 2);
    w.align();
    let mut head = [0u8; 4];
    head[..2].copy_from_slice(&len.to_le_bytes());
    head[2..].copy_from_slice(&(!len).to_le_bytes());
    w.bytes(&head);
    w.bytes(&chunk[..usize::from(len)]);
}

/// Compresses an input into one DEFLATE stream. The input has fewer than `u32::MAX` bytes.
pub(super) fn deflate(data: &[u8]) -> Vec<u8> {
    let (fixed_lit, fixed_dist) = fixed_lengths();
    let fixed = (Code::new(fixed_lit), Code::new(fixed_dist));
    let mut w = BitWriter::with_capacity(data.len() / 3 + 64);
    let mut lz = Lz77::new(data);
    let mut tokens = Vec::with_capacity(BLOCK_TOKENS);
    let mut start = 0;
    loop {
        tokens.clear();
        let done = lz.next_block(&mut tokens, BLOCK_TOKENS);
        let end = lz.consumed();
        write_block(&mut w, &tokens, &data[start..end], done, &fixed);
        start = end;
        if done {
            break;
        }
    }
    w.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_code_tables_follow_rfc_1951() {
        assert_eq!(len_index(3), 0);
        assert_eq!(len_index(10), 7);
        assert_eq!(len_index(11), 8);
        assert_eq!(len_index(12), 8);
        assert_eq!(len_index(227), 27);
        assert_eq!(len_index(257), 27);
        assert_eq!(len_index(258), 28);
        for len in MIN_MATCH..=MAX_MATCH {
            let i = len_index(len);
            let base = usize::from(LEN_BASE[i]);
            assert!(base <= len && len - base < (1 << LEN_EXTRA[i]), "{len}");
        }
        assert_eq!(dist_index(1), 0);
        assert_eq!(dist_index(4), 3);
        assert_eq!(dist_index(5), 4);
        assert_eq!(dist_index(6), 4);
        assert_eq!(dist_index(24577), 29);
        assert_eq!(dist_index(32768), 29);
        for d in 1..=32768usize {
            let i = dist_index(d);
            let base = usize::from(DIST_BASE[i]);
            assert!(base <= d && d - base < (1 << DIST_EXTRA[i]), "{d}");
        }
    }

    #[test]
    fn the_run_length_form() {
        let mut out = Vec::new();
        run_lengths(&[0; 140], &mut out);
        assert_eq!(out, vec![(18, 127), (0, 0), (0, 0)]);
        out.clear();
        run_lengths(&[5, 5, 5, 5, 5, 5, 5, 5, 0, 0, 0], &mut out);
        assert_eq!(out, vec![(5, 0), (16, 3), (5, 0), (17, 0)]);
        out.clear();
        run_lengths(&[3, 3, 0, 0], &mut out);
        assert_eq!(out, vec![(3, 0), (3, 0), (0, 0), (0, 0)]);
    }

    #[test]
    fn the_empty_input_is_one_fixed_block() {
        // BFINAL 1, BTYPE 01, the end of block (7 zero bits): the bytes 03 00, as zlib writes them.
        assert_eq!(deflate(b""), vec![0x03, 0x00]);
    }

    #[test]
    fn the_stored_size() {
        assert_eq!(stored_bits(0, 0), 3 + 5 + 32);
        assert_eq!(stored_bits(10, 5), 3 + 32 + 80);
        assert_eq!(stored_bits(65_536, 0), 3 + 5 + 32 + 40 + 8 * 65_536);
    }
}
