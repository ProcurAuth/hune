// SPDX-License-Identifier: MIT OR Apache-2.0
//! The common tests of 5.34 on the volume backend (5.33, item 4). SC-44 and SC-47 need
//! PostgreSQL. The tests `transfer_*` in `transfer.rs` run the same round trips on volumes.
#![cfg(feature = "conformance")]

use std::sync::Mutex;

use hune_store::AnyStore;
use hune_store::conformance::{Backend, Fixture, Hooks, common};
use hune_store::volume::testing::VolumeHooks;
use hune_store::volume::{Faults, VolumeOptions, VolumeStore};

struct VolumeBackend {
    dirs: Mutex<Vec<tempfile::TempDir>>,
}

impl VolumeBackend {
    fn new() -> Self {
        VolumeBackend { dirs: Mutex::new(Vec::new()) }
    }
}

impl Backend for VolumeBackend {
    async fn new_store(&self, history_keep: u32) -> Fixture {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = std::fs::canonicalize(dir.path()).expect("canonical root");
        self.dirs.lock().unwrap().push(dir);
        let faults = Faults::new();
        let opts = VolumeOptions {
            root: root.clone(),
            read_only: false,
            rescan_interval: std::time::Duration::from_secs(1),
            history_keep,
        };
        let store = VolumeStore::open_with_faults(opts, faults.clone()).await.expect("open");
        Fixture { store: AnyStore::Volume(store), hooks: Hooks::Volume(VolumeHooks::new(&root, faults)) }
    }
}

macro_rules! sc_tests {
    ($($name:ident),* $(,)?) => {
        $(
            #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
            async fn $name() {
                common::$name(&VolumeBackend::new()).await;
            }
        )*
    };
}

sc_tests!(
    sc_01, sc_02, sc_03, sc_04, sc_05, sc_06, sc_07, sc_08, sc_09, sc_10, sc_11, sc_12, sc_13, sc_14, sc_15, sc_16,
    sc_17, sc_18, sc_19, sc_20, sc_21, sc_22, sc_23, sc_24, sc_25, sc_26, sc_27, sc_28, sc_29, sc_30, sc_31, sc_32,
    sc_33, sc_34, sc_35, sc_36, sc_37, sc_38, sc_39, sc_40, sc_41, sc_42, sc_43, sc_45, sc_46, sc_48, sc_49, sc_50,
    sc_51, sc_52,
);
