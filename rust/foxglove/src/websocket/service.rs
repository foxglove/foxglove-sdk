//! Websocket services.

use std::collections::HashMap;
use std::fmt::Display;
use std::future::Future;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

mod handler;
mod request;
mod response;
mod schema;
#[cfg(test)]
mod tests;

use handler::{AsyncHandlerFn, BlockingHandlerFn, HandlerFn};
pub use handler::{Handler, SyncHandler};
pub use request::Request;
pub use response::Responder;
pub use schema::ServiceSchema;

/// A service ID, which uniquely identifies a service hosted by the server.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Deserialize, Serialize)]
pub(crate) struct ServiceId(u32);

impl ServiceId {
    /// Creates a new service ID.
    pub fn new(id: u32) -> Self {
        Self(id)
    }

    /// Allocates the next service ID.
    pub fn next() -> Self {
        static NEXT_ID: AtomicU32 = AtomicU32::new(1);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        assert_ne!(id, 0, "ServiceId overflowed");
        Self(id)
    }
}

impl From<ServiceId> for u32 {
    fn from(id: ServiceId) -> u32 {
        id.0
    }
}

impl Display for ServiceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A service call ID, which uniquely identifies an outstanding call for a particular client.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Deserialize, Serialize)]
pub struct CallId(u32);

impl CallId {
    /// Creates a new service ID.
    pub fn new(id: u32) -> Self {
        Self(id)
    }
}

impl From<CallId> for u32 {
    fn from(id: CallId) -> u32 {
        id.0
    }
}

impl Display for CallId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A builder for a websocket service.
#[must_use]
#[derive(Debug)]
pub struct ServiceBuilder {
    id: ServiceId,
    name: String,
    schema: ServiceSchema,
}
impl ServiceBuilder {
    /// Creates a new builder for a websocket service.
    fn new(name: impl Into<String>, schema: ServiceSchema) -> Self {
        Self {
            id: ServiceId::next(),
            name: name.into(),
            schema,
        }
    }

    /// Allow overriding the ID for deterministic tests.
    #[cfg(test)]
    pub(crate) fn with_id(mut self, id: ServiceId) -> Self {
        self.id = id;
        self
    }

    /// Configures a handler and returns the constructed [`Service`].
    pub fn handler<H: Handler + 'static>(self, handler: H) -> Service {
        Service {
            id: self.id,
            name: self.name,
            schema: self.schema,
            handler: Arc::new(handler),
        }
    }

    /// Configures a handler function and returns the constructed [`Service`].
    ///
    /// Refer to [`SyncHandler::call`] for a description of the `call` function.
    pub fn handler_fn<F, T, E>(self, call: F) -> Service
    where
        F: Fn(Request) -> Result<T, E> + Send + Sync + 'static,
        T: AsRef<[u8]> + 'static,
        E: Display + 'static,
    {
        self.handler(HandlerFn(call))
    }

    /// Configures a blocking handler function and returns the constructed [`Service`].
    ///
    /// The handler is invoked on a blocking thread with [`tokio::task::spawn_blocking`].
    ///
    /// Refer to [`SyncHandler::call`] for a description of the `call` function.
    pub fn blocking_handler_fn<F, T, E>(self, call: F) -> Service
    where
        F: Fn(Request) -> Result<T, E> + Send + Sync + 'static,
        T: AsRef<[u8]> + 'static,
        E: Display + 'static,
    {
        self.handler(BlockingHandlerFn(Arc::new(call)))
    }

    /// Configures an async handler function and returns the constructed [`Service`].
    ///
    /// The handler is invoked as a new async task with [`tokio::spawn`].
    ///
    /// Refer to [`SyncHandler::call`] for a description of the `call` function.
    pub fn async_handler_fn<F, Fut, T, E>(self, call: F) -> Service
    where
        F: Fn(Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<T, E>> + Send + 'static,
        T: AsRef<[u8]> + 'static,
        E: Display + Send + 'static,
    {
        self.handler(AsyncHandlerFn(Arc::new(call)))
    }
}

/// A websocket service.
#[must_use]
pub struct Service {
    id: ServiceId,
    name: String,
    schema: ServiceSchema,
    handler: Arc<dyn Handler>,
}

impl std::fmt::Debug for Service {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Service")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("schema", &self.schema)
            .finish_non_exhaustive()
    }
}

impl Service {
    /// Creates a new builder for a websocket service.
    pub fn builder(name: impl Into<String>, schema: ServiceSchema) -> ServiceBuilder {
        ServiceBuilder::new(name, schema)
    }

    /// Returns the service's ID.
    pub(crate) fn id(&self) -> ServiceId {
        self.id
    }

    /// Returns the service's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the service schema.
    pub fn schema(&self) -> &ServiceSchema {
        &self.schema
    }

    /// The declared request encoding.
    pub(crate) fn request_encoding(&self) -> Option<&str> {
        self.schema().request().map(|rs| rs.encoding.as_str())
    }

    /// The declared response encoding.
    pub(crate) fn response_encoding(&self) -> Option<&str> {
        self.schema().response().map(|rs| rs.encoding.as_str())
    }

    /// Invokes the service call implementation.
    pub(crate) fn call(&self, request: Request, responder: Responder) {
        self.handler.call(request, responder);
    }
}

#[derive(Default, Debug)]
pub(crate) struct ServiceMap {
    id: HashMap<ServiceId, Arc<Service>>,
    name: HashMap<String, ServiceId>,
}

impl ServiceMap {
    /// Constructs a service map from an iterable of services.
    ///
    /// Panics if service IDs and names are not unique.
    pub fn from_iter(services: impl IntoIterator<Item = Service>) -> Self {
        let iter = services.into_iter();
        let size = iter.size_hint().0;
        let id = HashMap::with_capacity(size);
        let name = HashMap::with_capacity(size);
        let mut map = Self { id, name };
        for service in iter {
            map.insert(service);
        }
        map
    }

    /// Inserts a service into the map.
    ///
    /// Panics if the service ID or name is not unique.
    pub fn insert(&mut self, service: Service) {
        let prev = self.name.insert(service.name().to_string(), service.id());
        assert!(prev.is_none());
        let prev = self.id.insert(service.id(), Arc::new(service));
        assert!(prev.is_none());
    }

    /// Removes a service by name.
    pub fn remove_by_name(&mut self, name: impl AsRef<str>) -> Option<Arc<Service>> {
        if let Some(id) = self.name.remove(name.as_ref()) {
            self.id.remove(&id)
        } else {
            None
        }
    }

    /// Returns true if the map contains a service with the provided name.
    pub fn contains_name(&self, name: impl AsRef<str>) -> bool {
        self.name.contains_key(name.as_ref())
    }

    /// Returns true if the map contains a service with the provided ID.
    pub fn contains_id(&self, id: ServiceId) -> bool {
        self.id.contains_key(&id)
    }

    /// Returns an iterator over services.
    pub fn values(&self) -> impl Iterator<Item = &Arc<Service>> {
        self.id.values()
    }

    /// Looks up a service by ID.
    pub fn get_by_id(&self, id: ServiceId) -> Option<Arc<Service>> {
        self.id.get(&id).cloned()
    }
}
