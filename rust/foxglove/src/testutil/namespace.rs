use parking_lot::{Mutex, MutexGuard};

use crate::Namespace;

static GLOBAL_CONTEXT_TEST_LOCK: Mutex<()> = Mutex::new(());

/// A helper to synchronize tests that use the global context, and clear it afterwards.
#[doc(hidden)]
pub struct GlobalContextTest<'a>(#[allow(dead_code)] MutexGuard<'a, ()>);

impl GlobalContextTest<'_> {
    pub fn new() -> Self {
        Self(GLOBAL_CONTEXT_TEST_LOCK.lock())
    }
}

impl Drop for GlobalContextTest<'_> {
    fn drop(&mut self) {
        Namespace::get_default().clear();
    }
}

impl Default for GlobalContextTest<'_> {
    fn default() -> Self {
        Self::new()
    }
}
