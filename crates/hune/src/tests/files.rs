// SPDX-License-Identifier: MIT OR Apache-2.0
//! File writes for unit tests: thin wrappers of `std::fs`.

use std::path::Path;

/// Writes `bytes` to `path`. Panics on an error, because this runs only in tests.
pub fn put(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) {
    std::fs::write(path, bytes).expect("write a test file");
}

/// Makes the directory `path` and its parents. Panics on an error.
pub fn mkdirs(path: impl AsRef<Path>) {
    std::fs::create_dir_all(path).expect("make a test directory");
}

/// Copies the file `from` to `to`. Panics on an error.
pub fn duplicate(from: impl AsRef<Path>, to: impl AsRef<Path>) {
    std::fs::copy(from, to).expect("copy a test file");
}
