// SPDX-License-Identifier: MIT OR Apache-2.0
//! The bit writer of DEFLATE (RFC 1951, 3.1.1). The bits fill each byte from the least significant bit.

/// A writer of bits into a byte list.
pub(super) struct BitWriter {
    out: Vec<u8>,
    /// The bits that do not make a whole byte yet. At most 7 bits stay between two calls.
    acc: u64,
    /// The number of bits in `acc`.
    count: u32,
}

impl BitWriter {
    /// Makes an empty writer with room for `capacity` bytes.
    pub(super) fn with_capacity(capacity: usize) -> Self {
        BitWriter { out: Vec::with_capacity(capacity), acc: 0, count: 0 }
    }

    /// Adds the `n` low bits of `value`, the least significant bit first. `n` is 0 to 32.
    pub(super) fn bits(&mut self, value: u32, n: u32) {
        let n = n.min(32);
        let mask = if n == 32 { u64::from(u32::MAX) } else { (1u64 << n) - 1 };
        self.acc |= (u64::from(value) & mask) << self.count;
        self.count += n;
        while self.count >= 8 {
            self.out.push((self.acc & 0xff) as u8);
            self.acc >>= 8;
            self.count -= 8;
        }
    }

    /// Fills the current byte with zero bits, so that the next bit starts a new byte.
    pub(super) fn align(&mut self) {
        if self.count > 0 {
            self.out.push((self.acc & 0xff) as u8);
            self.acc = 0;
            self.count = 0;
        }
    }

    /// Adds whole bytes after a byte boundary.
    pub(super) fn bytes(&mut self, data: &[u8]) {
        self.align();
        self.out.extend_from_slice(data);
    }

    /// The number of bits already in the current byte: 0 to 7.
    pub(super) fn bit_offset(&self) -> u32 {
        self.count
    }

    /// Gives the bytes. The last byte gets zero bits at its end.
    pub(super) fn finish(mut self) -> Vec<u8> {
        self.align();
        self.out
    }
}
