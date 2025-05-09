//! Types implementing well-known Foxglove schemas
//!
//! Using these types when possible will allow for richer visualizations
//! and a better experience in the Foxglove App.
//!
//! They're encoded as compact, binary protobuf messages,
//! and can be conveniently used with the [`Channel`](crate::Channel) API.

pub(crate) mod descriptors;
#[allow(missing_docs)]
#[rustfmt::skip]
mod foxglove;
#[rustfmt::skip]
mod impls;

use std::sync::LazyLock;

use prost_reflect::DescriptorPool;

pub use self::foxglove::*;
pub use crate::schemas_wkt::{Duration, Timestamp};

static DESCRIPTOR_POOL: LazyLock<DescriptorPool> = LazyLock::new(|| {
    DescriptorPool::decode(include_bytes!("schemas/data/file_descriptor_set.bin").as_ref()).unwrap()
});
