// SPDX-License-Identifier: MIT OR Apache-2.0
//! SC-44 and SC-47 between the volume and PostgreSQL, in both directions (5.34, 5.37, 16.14).
#![cfg(feature = "conformance")]

mod pg_support;

use hune_store::conformance::transfer;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs PostgreSQL: tests/ci/store-pg.sh"]
async fn sc_44_volume_to_postgres() {
    let (_g, dir) = pg_support::scratch();
    transfer::sc_44(&pg_support::VolumeBackend::new(), pg_support::backend(), &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs PostgreSQL: tests/ci/store-pg.sh"]
async fn sc_44_postgres_to_volume() {
    let (_g, dir) = pg_support::scratch();
    transfer::sc_44(pg_support::backend(), &pg_support::VolumeBackend::new(), &dir).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs PostgreSQL: tests/ci/store-pg.sh"]
async fn sc_44_postgres_to_postgres() {
    let (_g, dir) = pg_support::scratch();
    transfer::sc_44(pg_support::backend(), pg_support::backend(), &dir).await;
}

/// SC-47: volume to PostgreSQL to volume.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs PostgreSQL: tests/ci/store-pg.sh"]
async fn sc_47_volume_postgres_volume() {
    let (_g, dir) = pg_support::scratch();
    transfer::sc_47(&pg_support::VolumeBackend::new(), pg_support::backend(), &dir).await;
}

/// SC-47: PostgreSQL to volume to PostgreSQL.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs PostgreSQL: tests/ci/store-pg.sh"]
async fn sc_47_postgres_volume_postgres() {
    let (_g, dir) = pg_support::scratch();
    transfer::sc_47(pg_support::backend(), &pg_support::VolumeBackend::new(), &dir).await;
}
