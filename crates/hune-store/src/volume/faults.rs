// SPDX-License-Identifier: MIT OR Apache-2.0
//! Fault injection for the tests (16.7, SX-01). A build without the feature `conformance`
//! has an empty stub here. The stub never fails.

use rustix::io::Errno;

/// A place in the code of the volume backend where a test can inject a fault.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultPoint {
    /// Each write of bytes into a file.
    Write,
    /// Each `fsync` of a file or a directory.
    Fsync,
    /// After `<n>.json` of a version, before its metadata file (SV-17).
    HistoryJsonWritten,
    /// After the metadata file of a version, before its effect (SV-16, SV-25, SV-28).
    HistoryWritten,
    /// After the prepare of the effect, before its check or its rename (SV-19, SV-27).
    BeforeEffect,
    /// The `flock` call at start (SV-21).
    Flock,
    /// The start of each scan (a failed poll).
    Scan,
}

#[cfg(any(test, feature = "conformance"))]
mod real {
    use super::{Errno, FaultPoint};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};

    /// What a fault does when its point is reached.
    #[derive(Clone)]
    pub enum FaultAction {
        /// The call fails with this raw error number.
        Errno(i32),
        /// The call runs this function, then goes on. The function can change the volume.
        Run(Arc<dyn Fn() + Send + Sync>),
    }

    impl std::fmt::Debug for FaultAction {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                FaultAction::Errno(e) => write!(f, "Errno({e})"),
                FaultAction::Run(_) => f.write_str("Run(..)"),
            }
        }
    }

    #[derive(Default)]
    struct State {
        rules: Mutex<Vec<(FaultPoint, FaultAction)>>,
        fsyncs: AtomicU64,
    }

    /// The handle of the faults of one store. Clones share the state.
    #[derive(Clone, Default)]
    pub struct Faults(Arc<State>);

    impl std::fmt::Debug for Faults {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("Faults")
        }
    }

    impl Faults {
        /// Makes a handle with no fault.
        pub fn new() -> Self {
            Faults::default()
        }
        /// Adds a fault that fires one time at the next pass of `point`.
        pub fn inject(&self, point: FaultPoint, action: FaultAction) {
            if let Ok(mut r) = self.0.rules.lock() {
                r.push((point, action));
            }
        }
        /// The number of `fsync` calls so far.
        pub fn fsync_calls(&self) -> u64 {
            self.0.fsyncs.load(Ordering::SeqCst)
        }
        pub(crate) fn check(&self, point: FaultPoint) -> Result<(), Errno> {
            if point == FaultPoint::Fsync {
                self.0.fsyncs.fetch_add(1, Ordering::SeqCst);
            }
            let action = {
                let Ok(mut r) = self.0.rules.lock() else { return Ok(()) };
                match r.iter().position(|(p, _)| *p == point) {
                    Some(i) => r.remove(i).1,
                    None => return Ok(()),
                }
            };
            match action {
                FaultAction::Errno(e) => Err(Errno::from_raw_os_error(e)),
                FaultAction::Run(f) => {
                    f();
                    Ok(())
                }
            }
        }
    }
}

#[cfg(any(test, feature = "conformance"))]
pub use real::{FaultAction, Faults};

#[cfg(not(any(test, feature = "conformance")))]
mod stub {
    use super::{Errno, FaultPoint};

    /// The empty handle of a build without fault injection.
    #[derive(Debug, Clone, Default)]
    pub struct Faults;

    impl Faults {
        pub(crate) fn new() -> Self {
            Faults
        }
        #[inline]
        pub(crate) fn check(&self, _point: FaultPoint) -> Result<(), Errno> {
            Ok(())
        }
    }
}

#[cfg(not(any(test, feature = "conformance")))]
pub use stub::Faults;
