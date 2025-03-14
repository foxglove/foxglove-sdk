use crate::{FoxgloveError, Sink};
use parking_lot::RwLock;
use std::sync::Arc;

pub(crate) const ERROR_LOGGING_MESSAGE: &str = "error logging message";

// Future optimization: lock-free chain of struct {
//    array: [MAX_SINKS_PER_CHANNEL]AtomicPtr<Arc<dyn LogSink>>,
//    next: AtomicPtr<Self>,
// }

pub(crate) struct LogSinkSet(RwLock<Vec<Arc<dyn Sink>>>);

impl LogSinkSet {
    pub const fn new() -> Self {
        Self(RwLock::new(Vec::new()))
    }

    /// Returns true if the set is empty.
    #[inline(always)]
    pub(crate) fn is_empty(&self) -> bool {
        // It's too expensive to lock the whole set just to check if it's empty, for now we don't implement this
        false
    }

    /// Add a sink to the set. Returns false if the sink was already in the set.
    pub fn add_sink(&self, sink: Arc<dyn Sink>) -> bool {
        let mut sinks = self.0.write();
        // Check if the sink is already in the set.
        if sinks.iter().any(|s| Arc::ptr_eq(s, &sink)) {
            return false;
        }
        sinks.push(sink);
        true
    }

    /// Remove a sink from the set. Returns true if the sink was removed.
    pub fn remove_sink(&self, sink: &Arc<dyn Sink>) -> bool {
        let mut sinks = self.0.write();
        let len_before = sinks.len();
        sinks.retain(|s| !Arc::ptr_eq(s, sink));
        sinks.len() < len_before
    }

    /// Iterate over all the sinks in the set, calling the given function on each,
    /// logging any errors via tracing::warn!().
    pub fn for_each<F>(&self, mut f: F)
    where
        F: FnMut(&Arc<dyn Sink>) -> Result<(), FoxgloveError>,
    {
        let sinks = self.0.read();
        for sink in sinks.iter() {
            if let Err(err) = f(sink) {
                tracing::warn!("{ERROR_LOGGING_MESSAGE}: {:?}", err);
            }
        }
    }

    pub fn clear(&self) {
        self.0.write().clear();
    }
}
