// SPDX-License-Identifier: MIT OR Apache-2.0
//! Helpers for the unit tests of the configuration.

use std::io::Write;
use std::path::Path;

/// Write a file for a test. The tests write only in temporary directories.
pub fn write_file(path: impl AsRef<Path>, content: impl AsRef<[u8]>) -> std::io::Result<()> {
    let mut file = std::fs::File::options().write(true).create(true).truncate(true).open(path.as_ref())?;
    file.write_all(content.as_ref())
}

thread_local! {
    static MONGO_DRIVER: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

/// True when the test build acts as a build with the MongoDB driver. The default is true.
pub fn mongo_driver() -> bool {
    MONGO_DRIVER.with(std::cell::Cell::get)
}

/// Make the thread of the test act as a build with or without the MongoDB driver.
pub fn set_mongo_driver(on: bool) {
    MONGO_DRIVER.with(|c| c.set(on));
}
