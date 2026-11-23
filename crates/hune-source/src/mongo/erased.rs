// SPDX-License-Identifier: MIT OR Apache-2.0
//! The object-safe form of the client interface, and the connector of a source.
//!
//! The trait [`MongoClient`] has methods that return `impl Future`, so it is not object-safe. The
//! source keeps its client behind a pointer, because the probe replaces the client at each good run.
//! [`ErasedClient`] is the same set of operations with boxed futures. Each [`MongoClient`] has it.
//!
//! A [`MongoConnector`] makes a client from the [`ConnectOptions`]. The driver module gives
//! [`MongoConnector::of_type`]. The tests give [`MongoConnector::from_fn`] with a fake client.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::mongo::iface::{
    AuthInfo, Batch, ConnectOptions, CursorHandle, MongoClient, MongoError, ReadRequest, ServerInfo,
};

/// A boxed future that the source can move to another task.
pub(crate) type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// [`MongoClient`] without the constructor, with boxed futures.
pub(crate) trait ErasedClient: Send + Sync {
    fn server_info(&self) -> BoxFut<'_, Result<ServerInfo, MongoError>>;
    fn connection_status(&self) -> BoxFut<'_, Result<AuthInfo, MongoError>>;
    fn list_collections<'a>(&'a self, database: &'a str) -> BoxFut<'a, Result<Vec<String>, MongoError>>;
    fn read(&self, request: ReadRequest) -> BoxFut<'_, Result<Batch, MongoError>>;
    fn get_more<'a>(&'a self, cursor: &'a mut CursorHandle) -> BoxFut<'a, Result<Batch, MongoError>>;
    fn kill_cursor(&self, cursor: CursorHandle) -> BoxFut<'_, ()>;
    fn finish(&self, cursor: Option<CursorHandle>) -> BoxFut<'_, ()>;
}

impl<C: MongoClient> ErasedClient for C {
    fn server_info(&self) -> BoxFut<'_, Result<ServerInfo, MongoError>> {
        Box::pin(MongoClient::server_info(self))
    }

    fn connection_status(&self) -> BoxFut<'_, Result<AuthInfo, MongoError>> {
        Box::pin(MongoClient::connection_status(self))
    }

    fn list_collections<'a>(&'a self, database: &'a str) -> BoxFut<'a, Result<Vec<String>, MongoError>> {
        Box::pin(MongoClient::list_collections(self, database))
    }

    fn read(&self, request: ReadRequest) -> BoxFut<'_, Result<Batch, MongoError>> {
        Box::pin(MongoClient::read(self, request))
    }

    fn get_more<'a>(&'a self, cursor: &'a mut CursorHandle) -> BoxFut<'a, Result<Batch, MongoError>> {
        Box::pin(MongoClient::get_more(self, cursor))
    }

    fn kill_cursor(&self, cursor: CursorHandle) -> BoxFut<'_, ()> {
        Box::pin(MongoClient::kill_cursor(self, cursor))
    }

    fn finish(&self, cursor: Option<CursorHandle>) -> BoxFut<'_, ()> {
        Box::pin(MongoClient::finish(self, cursor))
    }
}

/// The function of a connector: options in, a client out.
type ConnectFn = dyn Fn(ConnectOptions) -> BoxFut<'static, Result<Arc<dyn ErasedClient>, MongoError>> + Send + Sync;

/// Makes the client of a source from its options. The source calls it at each probe: a probe that
/// passes replaces the client, and the old client closes its connections when it drops.
#[derive(Clone)]
pub struct MongoConnector {
    connect: Arc<ConnectFn>,
}

impl std::fmt::Debug for MongoConnector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MongoConnector")
    }
}

impl MongoConnector {
    /// Makes a connector that calls [`MongoClient::connect`] of the type `C`. The driver module uses it.
    pub fn of_type<C: MongoClient + 'static>() -> MongoConnector {
        MongoConnector::from_fn(|options| C::connect(options))
    }

    /// Makes a connector from a function that gives a client. The tests use it with a fake client.
    pub fn from_fn<C, F, Fut>(connect: F) -> MongoConnector
    where
        C: MongoClient + 'static,
        F: Fn(ConnectOptions) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<C, MongoError>> + Send + 'static,
    {
        MongoConnector {
            connect: Arc::new(move |options| {
                let fut = connect(options);
                Box::pin(async move {
                    let client = fut.await?;
                    Ok(Arc::new(client) as Arc<dyn ErasedClient>)
                })
            }),
        }
    }

    pub(crate) fn connect(
        &self,
        options: ConnectOptions,
    ) -> BoxFut<'static, Result<Arc<dyn ErasedClient>, MongoError>> {
        (self.connect)(options)
    }
}
