//! The LZ77 step of DEFLATE (RFC 1951, 4): hash chains and the lazy match of zlib level 6.
//!
//! The memory is fixed: two tables of 32768 positions. The time for each byte has a limit: a search reads
//! at most `MAX_CHAIN` positions of a chain. Each match that this module gives is checked byte by byte
//! against the input, so a stale entry of a chain can only make a match shorter, never wrong.

/// The shortest match of DEFLATE.
pub(super) const MIN_MATCH: usize = 3;
/// The longest match of DEFLATE.
pub(super) const MAX_MATCH: usize = 258;
/// The size of the window, and of the chain table.
const WINDOW: usize = 32768;
const WINDOW_MASK: usize = WINDOW - 1;
/// The longest distance that a match uses. One less than the window keeps the chain table exact.
const MAX_DIST: usize = WINDOW - 1;
/// The size of the hash table: 15 bits from the three bytes at a position.
const HASH_BITS: u32 = 15;
const HASH_MASK: u32 = (1 << HASH_BITS) - 1;
/// The value of an empty entry of the tables.
const NIL: u32 = u32::MAX;

/// The parameters of zlib level 6.
/// A match of this length or more makes the next search four times shorter.
const GOOD_LENGTH: usize = 8;
/// A match of this length or more is not followed by a lazy search.
const MAX_LAZY: usize = 16;
/// A match of this length or more stops the search.
const NICE_LENGTH: usize = 128;
/// The most positions that one search reads.
const MAX_CHAIN: usize = 128;
/// A match of three bytes at a distance above this value costs more than three literals.
const TOO_FAR: usize = 4096;

/// One symbol of the LZ77 output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Token {
    /// One byte of the input.
    Literal(u8),
    /// A copy of `len` bytes (3 to 258) from the position `dist` bytes before (1 to 32767).
    Match { len: u16, dist: u16 },
}

/// The match finder and the state of the lazy match. One value reads one input from start to end.
pub(super) struct Lz77<'a> {
    data: &'a [u8],
    /// The last position of each hash value.
    head: Vec<u32>,
    /// For a position `p`, the position before it with the same hash value, at `p % WINDOW`.
    prev: Vec<u32>,
    /// The next position to look at.
    pos: usize,
    /// The number of input bytes that the given tokens hold.
    consumed: usize,
    /// True when the byte at `pos - 1` waits for a decision: a literal or the start of a match.
    have_prev: bool,
    /// The match at `pos - 1`, when `have_prev` is true. A length below `MIN_MATCH` is no match.
    prev_len: usize,
    prev_dist: usize,
}

fn hash3(data: &[u8], p: usize) -> usize {
    let h = (u32::from(data[p]) << 10) ^ (u32::from(data[p + 1]) << 5) ^ u32::from(data[p + 2]);
    (h & HASH_MASK) as usize
}

impl<'a> Lz77<'a> {
    /// Makes the state for an input. The input has fewer than `u32::MAX` bytes.
    pub(super) fn new(data: &'a [u8]) -> Self {
        Lz77 {
            data,
            head: vec![NIL; 1 << HASH_BITS],
            prev: vec![NIL; WINDOW],
            pos: 0,
            consumed: 0,
            have_prev: false,
            prev_len: 0,
            prev_dist: 0,
        }
    }

    /// The number of input bytes that the tokens until now hold.
    pub(super) fn consumed(&self) -> usize {
        self.consumed
    }

    /// Adds the position `p` to the chains, when three bytes start at it.
    fn insert(&mut self, p: usize) {
        if p + MIN_MATCH > self.data.len() {
            return;
        }
        let h = hash3(self.data, p);
        self.prev[p & WINDOW_MASK] = self.head[h];
        self.head[h] = u32::try_from(p).unwrap_or(NIL);
    }

    /// Finds the longest match at `pos` that is longer than `floor`. Gives `(0, 0)` when there is none.
    fn longest_match(&self, pos: usize, floor: usize) -> (usize, usize) {
        let data = self.data;
        let max_len = (data.len() - pos).min(MAX_MATCH);
        if max_len < MIN_MATCH || floor >= max_len {
            return (0, 0);
        }
        let nice = NICE_LENGTH.min(max_len);
        let mut chain = if floor >= GOOD_LENGTH { MAX_CHAIN / 4 } else { MAX_CHAIN };
        let (mut best_len, mut best_dist) = (floor, 0usize);
        let mut cand = self.head[hash3(data, pos)];
        while cand != NIL && chain > 0 {
            let c = cand as usize;
            if c >= pos || pos - c > MAX_DIST {
                break;
            }
            // `best_len < nice <= max_len`, so both reads are inside the input.
            if data[c + best_len] == data[pos + best_len] {
                let len =
                    data[c..c + max_len].iter().zip(&data[pos..pos + max_len]).take_while(|(a, b)| a == b).count();
                if len > best_len {
                    best_len = len;
                    best_dist = pos - c;
                    if len >= nice {
                        break;
                    }
                }
            }
            let next = self.prev[c & WINDOW_MASK];
            if next == NIL || next >= cand {
                break;
            }
            cand = next;
            chain -= 1;
        }
        if best_dist == 0 { (0, 0) } else { (best_len, best_dist) }
    }

    /// Adds tokens to `out` until `out` holds `max` tokens or the input ends. Gives true when the input
    /// ends: then the tokens hold each byte of the input. This is the loop `deflate_slow` of zlib.
    pub(super) fn next_block(&mut self, out: &mut Vec<Token>, max: usize) -> bool {
        let n = self.data.len();
        while out.len() < max {
            if self.pos >= n {
                if self.have_prev {
                    out.push(Token::Literal(self.data[self.pos - 1]));
                    self.consumed += 1;
                    self.have_prev = false;
                }
                return true;
            }
            let pos = self.pos;
            let (mut len, mut dist) = (0usize, 0usize);
            if pos + MIN_MATCH <= n && (!self.have_prev || self.prev_len < MAX_LAZY) {
                let floor = if self.have_prev { self.prev_len.max(MIN_MATCH - 1) } else { MIN_MATCH - 1 };
                (len, dist) = self.longest_match(pos, floor);
                if len == MIN_MATCH && dist > TOO_FAR {
                    (len, dist) = (0, 0);
                }
            }
            self.insert(pos);
            if self.have_prev && self.prev_len >= MIN_MATCH && len <= self.prev_len {
                // The match at `pos - 1` is not shorter than the match at `pos`: send it.
                out.push(Token::Match {
                    len: u16::try_from(self.prev_len).unwrap_or(0),
                    dist: u16::try_from(self.prev_dist).unwrap_or(0),
                });
                self.consumed += self.prev_len;
                let end = pos - 1 + self.prev_len;
                for p in pos + 1..end {
                    self.insert(p);
                }
                self.pos = end;
                self.have_prev = false;
                self.prev_len = 0;
                self.prev_dist = 0;
            } else if self.have_prev {
                // The byte at `pos - 1` is a literal. The match at `pos` waits for the next position.
                out.push(Token::Literal(self.data[pos - 1]));
                self.consumed += 1;
                self.prev_len = len;
                self.prev_dist = dist;
                self.pos += 1;
            } else {
                self.have_prev = true;
                self.prev_len = len;
                self.prev_dist = dist;
                self.pos += 1;
            }
        }
        self.pos >= n && !self.have_prev
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Makes the input again from the tokens, as an inflater does.
    fn expand(tokens: &[Token]) -> Vec<u8> {
        let mut out = Vec::new();
        for t in tokens {
            match *t {
                Token::Literal(b) => out.push(b),
                Token::Match { len, dist } => {
                    let start = out.len() - usize::from(dist);
                    for k in 0..usize::from(len) {
                        out.push(out[start + k]);
                    }
                }
            }
        }
        out
    }

    fn tokens_of(data: &[u8], block: usize) -> Vec<Token> {
        let mut lz = Lz77::new(data);
        let mut all = Vec::new();
        loop {
            let mut part = Vec::new();
            let done = lz.next_block(&mut part, block);
            assert!(part.len() <= block);
            all.extend(part);
            assert_eq!(lz.consumed(), expand(&all).len());
            if done {
                break;
            }
        }
        all
    }

    #[test]
    fn the_tokens_give_the_input_again() {
        let mut text = Vec::new();
        for i in 0..2000u32 {
            text.extend_from_slice(format!("line {} of the text, value {}\n", i % 37, i * 7 % 101).as_bytes());
        }
        let zeros = vec![0u8; 100_000];
        let cases: [&[u8]; 7] = [b"", b"a", b"ab", b"abc", b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", &text, &zeros];
        for data in cases {
            for block in [1, 7, 16_384] {
                let tokens = tokens_of(data, block);
                assert_eq!(expand(&tokens), data);
                for t in &tokens {
                    if let Token::Match { len, dist } = *t {
                        assert!((3..=258).contains(&len) && (1..=32767).contains(&dist), "{t:?}");
                    }
                }
            }
        }
        // A long run is mostly matches of 258 bytes.
        let run = tokens_of(&vec![7u8; 10_000], 16_384);
        assert!(run.len() < 60, "{}", run.len());
    }
}
