use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::sink::SmallSinkVec;
use crate::{FoxgloveError, Sink};

pub(crate) const ERROR_LOGGING_MESSAGE: &str = "error logging message";

#[derive(Default)]
pub(crate) struct LogSinkSet(ArcSwap<SmallSinkVec>);

impl LogSinkSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if the set is empty.
    pub fn is_empty(&self) -> bool {
        self.0.load().is_empty()
    }

    /// Returns the number of sinks in the set.
    #[cfg(all(test, feature = "live_visualization"))]
    pub fn len(&self) -> usize {
        self.0.load().len()
    }

    /// Replaces the set of sinks in the set.
    pub fn store(&self, sinks: SmallSinkVec) {
        self.0.store(Arc::new(sinks));
    }

    /// Iterate over all the sinks in the set, calling the given function on each,
    /// logging any errors via tracing::warn!().
    pub fn for_each<F>(&self, mut f: F)
    where
        F: FnMut(&Arc<dyn Sink>) -> Result<(), FoxgloveError>,
    {
        for sink in self.0.load().iter() {
            if let Err(err) = f(sink) {
                tracing::warn!("{ERROR_LOGGING_MESSAGE}: {:?}", err);
            }
        }
    }

    /// Clears the set.
    pub fn clear(&self) {
        self.0.store(Arc::default());
    }
}
