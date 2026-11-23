// SPDX-License-Identifier: MIT OR Apache-2.0
//! In-process tests of the server: real HTTP/1.1 requests on `127.0.0.1`.

mod adhoc_filters;
mod annotation_query;
mod api;
mod authzen;
mod authzen_pdp;
mod bounded_checks;
mod bucket_panels;
mod core;
mod f27_interval_constant;
mod f28_more_transforms;
mod f41_open_variables;
mod fake_ch;
mod geomap;
pub(crate) mod harness;
mod inspect;
#[cfg(feature = "mongo-driver")]
mod mongo_real;
mod mongo_source;
mod pg_env;
mod pg_real;
mod pg_serve;
mod pg_source;
mod queries;
mod query;
mod query_params;
mod real_ch;
mod repeat;
mod restore_once;
mod same_cost;
mod same_refusal;
mod static_gzip;
mod telemetry;
mod text_panel;
mod timeline_panels;
