//! The Huffman codes of DEFLATE (RFC 1951, 3.2.2): code lengths with a length limit, and canonical codes.
//!
//! Each code that this module makes is complete: the Kraft sum of its lengths is exactly 1. Every inflater
//! accepts a complete code. zlib refuses an incomplete code of code lengths, so this rule is necessary.

/// The value of "no parent" in the tree of `code_lengths`.
const NO_PARENT: usize = usize::MAX;

/// Gives the code length of each symbol for the frequencies `freq`. No length is more than `limit`.
///
/// A symbol with the frequency 0 gets the length 0. When fewer than two symbols have a frequency, symbol 0
/// or symbol 1 gets the weight 1, so two symbols at least have a code (zlib does the same). The result is an
/// optimal code when no length goes above `limit`. Otherwise it is the code of the procedure of JPEG
/// (ITU-T T.81, annex K.3), which is complete and nearly optimal. `freq` has 2 to 512 symbols, and `limit`
/// is 7 or 15 for DEFLATE.
pub(super) fn code_lengths(freq: &[u32], limit: u8) -> Vec<u8> {
    let mut weight: Vec<u64> = freq.iter().map(|&f| u64::from(f)).collect();
    for s in 0..weight.len().min(2) {
        if weight.iter().filter(|&&w| w > 0).count() < 2 && weight[s] == 0 {
            weight[s] = 1;
        }
    }
    // The leaves in the order of (weight, symbol). This order makes the result deterministic.
    let mut leaves: Vec<(u64, usize)> =
        weight.iter().enumerate().filter(|&(_, &w)| w > 0).map(|(s, &w)| (w, s)).collect();
    leaves.sort_unstable();
    let mut lengths = vec![0u8; freq.len()];
    let m = leaves.len();
    if m < 2 {
        // Only an alphabet of fewer than two symbols comes here. DEFLATE has none.
        return lengths;
    }
    let depths = tree_depths(&leaves);
    let max_depth = depths.iter().copied().max().unwrap_or(0);
    let mut count = vec![0u32; max_depth.max(usize::from(limit)) + 1];
    for &d in &depths {
        count[d] += 1;
    }
    let limit = usize::from(limit);
    if max_depth > limit && !limit_counts(&mut count, limit) {
        count = flat_counts(m, count.len());
    }
    // The shortest codes go to the most frequent symbols.
    let mut order = leaves.iter().rev().map(|&(_, s)| s);
    for (len, &n) in count.iter().enumerate().skip(1) {
        for _ in 0..n {
            if let Some(s) = order.next() {
                lengths[s] = u8::try_from(len).unwrap_or(0);
            }
        }
    }
    lengths
}

/// Builds the Huffman tree of the leaves (the lightest leaf first) with two queues, and gives the depth of each
/// leaf, in the order of `leaves`.
fn tree_depths(leaves: &[(u64, usize)]) -> Vec<usize> {
    let m = leaves.len();
    let total = 2 * m - 1;
    let mut weight: Vec<u64> = leaves.iter().map(|&(w, _)| w).collect();
    weight.resize(total, 0);
    let mut parent = vec![NO_PARENT; total];
    // `next_leaf` reads the leaves. `next_inner` reads the inner nodes, which come in increasing weight.
    let (mut next_leaf, mut next_inner) = (0usize, m);
    for node in m..total {
        let mut take = || {
            let leaf_first = next_leaf < m && (next_inner >= node || weight[next_leaf] <= weight[next_inner]);
            if leaf_first {
                next_leaf += 1;
                next_leaf - 1
            } else {
                next_inner += 1;
                next_inner - 1
            }
        };
        let (a, b) = (take(), take());
        weight[node] = weight[a].saturating_add(weight[b]);
        parent[a] = node;
        parent[b] = node;
    }
    // A parent always has a higher index than its children, so one pass from the root gives each depth.
    let mut depth = vec![0usize; total];
    for i in (0..total - 1).rev() {
        depth[i] = depth.get(parent[i]).map_or(0, |d| d + 1);
    }
    depth.truncate(m);
    depth
}

/// Puts each leaf that is deeper than `limit` at a lower depth, with the procedure of JPEG (ITU-T T.81, figure K.3).
///
/// `count[d]` is the number of leaves at depth `d`. Each step takes two leaves at the deepest level and one
/// leaf at a level `j` above it. It puts one leaf at the level above the deepest one, and two leaves at
/// level `j + 1`. The Kraft sum and the number of leaves do not change. Gives false for a table that is not
/// a complete code.
fn limit_counts(count: &mut [u32], limit: usize) -> bool {
    let max = count.len().saturating_sub(1);
    for i in (limit + 1..=max).rev() {
        while count[i] > 0 {
            if count[i] < 2 || i < 2 {
                return false;
            }
            let mut j = i - 2;
            while j > 0 && count[j] == 0 {
                j -= 1;
            }
            if count[j] == 0 {
                return false;
            }
            count[i] -= 2;
            count[i - 1] += 1;
            count[j + 1] += 2;
            count[j] -= 1;
        }
    }
    true
}

/// A complete code of `m` leaves with two lengths only: `k - 1` and `k`, where `k` is the smallest value with
/// `2^k >= m`. It is a safe replacement when `limit_counts` fails. That never occurs for a complete code.
fn flat_counts(m: usize, size: usize) -> Vec<u32> {
    let k = m.next_power_of_two().trailing_zeros() as usize;
    let mut count = vec![0u32; size.max(k + 1)];
    let short = (1usize << k) - m;
    count[k.saturating_sub(1)] += u32::try_from(short).unwrap_or(0);
    count[k] += u32::try_from(m - short).unwrap_or(0);
    count
}

/// Gives the canonical code of each symbol (RFC 1951, 3.2.2), with its bits in reverse order. The bit writer
/// sends the least significant bit first, and DEFLATE sends a Huffman code from its most significant bit.
pub(super) fn reversed_codes(lengths: &[u8]) -> Vec<u16> {
    let mut count = [0u16; 16];
    for &len in lengths {
        if len > 0 {
            count[usize::from(len.min(15))] += 1;
        }
    }
    let mut next = [0u16; 16];
    let mut code = 0u16;
    for bits in 1..16 {
        code = code.wrapping_add(count[bits - 1]) << 1;
        next[bits] = code;
    }
    lengths
        .iter()
        .map(|&len| {
            if len == 0 {
                return 0;
            }
            let len = usize::from(len.min(15));
            let c = next[len];
            next[len] = next[len].wrapping_add(1);
            c.reverse_bits() >> (16 - len)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Kraft sum of a code, in units of `2^-15`.
    fn kraft(lengths: &[u8]) -> u64 {
        lengths.iter().filter(|&&l| l > 0).map(|&l| 1u64 << (15 - l)).sum()
    }

    #[test]
    fn the_codes_are_complete_and_respect_the_limit() {
        // Fibonacci frequencies make the deepest tree for a number of symbols.
        let mut fib = vec![1u32, 1];
        while fib.len() < 30 {
            let n = fib.len();
            fib.push(fib[n - 1] + fib[n - 2]);
        }
        for limit in [7u8, 15] {
            let lengths = code_lengths(&fib, limit);
            assert!(lengths.iter().all(|&l| l > 0 && l <= limit), "{lengths:?}");
            assert_eq!(kraft(&lengths), 1 << 15);
        }
        // One symbol, no symbol, and two symbols.
        assert_eq!(code_lengths(&[0, 0, 5, 0], 15), vec![1, 0, 1, 0]);
        assert_eq!(code_lengths(&[0, 0, 0], 15), vec![1, 1, 0]);
        assert_eq!(code_lengths(&[3, 0, 0], 7), vec![1, 1, 0]);
        assert_eq!(code_lengths(&[9, 1], 7), vec![1, 1]);
    }

    #[test]
    fn an_optimal_code_for_a_known_case() {
        // The weights 1, 1, 2, 4 give the lengths 3, 3, 2, 1.
        assert_eq!(code_lengths(&[1, 1, 2, 4], 15), vec![3, 3, 2, 1]);
        // With a limit of 2 bits, four symbols get 2 bits each.
        assert_eq!(code_lengths(&[1, 1, 2, 4], 2), vec![2, 2, 2, 2]);
    }

    #[test]
    fn the_canonical_codes_of_rfc_1951() {
        // The example of RFC 1951, 3.2.2: lengths (3, 3, 3, 3, 3, 2, 4, 4) give
        // 010, 011, 100, 101, 110, 00, 1110, 1111.
        let codes = reversed_codes(&[3, 3, 3, 3, 3, 2, 4, 4]);
        let want: [(u16, u32); 8] =
            [(0b010, 3), (0b011, 3), (0b100, 3), (0b101, 3), (0b110, 3), (0b00, 2), (0b1110, 4), (0b1111, 4)];
        for (c, (w, len)) in codes.iter().zip(want) {
            assert_eq!(c.reverse_bits() >> (16 - len), w);
        }
    }

    #[test]
    fn the_flat_code_is_complete() {
        for m in 2..=300usize {
            let count = flat_counts(m, 16);
            let leaves: u32 = count.iter().sum();
            assert_eq!(leaves as usize, m);
            let sum: u64 = count.iter().enumerate().map(|(d, &n)| u64::from(n) << (20 - d)).sum();
            assert_eq!(sum, 1 << 20, "{m}");
        }
    }
}
