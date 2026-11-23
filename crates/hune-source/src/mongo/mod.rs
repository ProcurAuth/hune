//! The MongoDB data source (section 8b). It reads data for panel queries and never writes.
//!
//! The query language (7.27 to 7.36) and the value model (8b.16) live in `hune_core::mongo`. The
//! network client stays behind the trait [`MongoClient`]. The crate has no MongoDB dependency: the
//! driver is the decision `ID-MG2` and it is not in this build (8b.2).
//!
//! # Modules and the sections that each one implements
//!
//! | Module | Sections | Job |
//! |---|---|---|
//! | `iface` | 8b.5, 8b.12, 8b.16 | The trait [`MongoClient`] and its plain types. |
//! | `erased` | 8b.16 | The object-safe form of the trait and the [`MongoConnector`]. |
//! | `guard` | 8b.3, 8b.5 | The wire guard: refused keys at any depth, the lists of the block. |
//! | `errors` | 8b.8 | The error map by the code of the server. |
//! | `attrib` | 8b.11 | The `comment` document and `appName`. |
//! | `probe` | 8b.12, 8b.13, 8b.14 | The checks M0 to M4, the advice A1 and A2, the verdict. |
//! | `exec` | 8b.5 to 8b.7 | The steps of a query, the caps, the deadline, the kill. |
//! | `fake` | 8b.16 | A fake client that scripts the answers of a server, for the tests. |
//! | `source` | 8b.2, 8b.9, 8b.10, 8b.12, 8.11 | The type [`MongoSource`]. |
//!
//! # Status
//!
//! The logic of 8b.5 to 8b.12 is complete and generic over [`MongoClient`]. The unit tests run it
//! against [`FakeMongo`]. The build has no driver (8b.2): [`MongoSource::new`] makes a source that
//! stays `misconfigured` with [`NO_DRIVER_MESSAGE`]. The driver module, when the owner agrees, gives a
//! client type `C` and the build calls [`MongoSource::with_driver`].

mod attrib;
#[cfg(feature = "mongo-driver")]
mod driver;
mod erased;
pub mod errors;
mod exec;
mod fake;
pub mod guard;
pub(crate) mod iface;
#[cfg(test)]
#[path = "tests/ms_cases.rs"]
mod ms_cases;
pub mod probe;
mod source;

pub use attrib::APP_NAME;
#[cfg(feature = "mongo-driver")]
pub use driver::{MongoDriver, decode_document};
pub use erased::MongoConnector;
pub use fake::{FakeMongo, Reply};
pub use iface::{
    AuthInfo, AuthRole, AuthUser, Batch, CERTIFICATE_CODE_NAME, ConnectOptions, CursorHandle, MongoClient, MongoError,
    MongoErrorClass, Privilege, PrivilegeResource, ReadOp, ReadRequest, ServerInfo,
};
pub use source::{MongoSource, NO_DRIVER_MESSAGE};
