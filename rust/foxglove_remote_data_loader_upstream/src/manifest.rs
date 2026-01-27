use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_constant::ConstBool;
use serde_with::{base64::Base64, serde_as};
use std::num::NonZeroU16;

/// Manifest of upstream sources.
#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    #[serde(default)]
    pub name: Option<String>,
    pub sources: Vec<UpstreamSource>,
}

/// A data source from an upstream manifest.
///
/// Sources can be either static files (supporting range requests for random access)
/// or streamed sources that must be read sequentially.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", untagged)]
pub enum UpstreamSource {
    /// A static file that supports HTTP range requests.
    StaticFile {
        url: String,
        /// Marker indicating range request support (always true).
        #[allow(unused)]
        support_range_requests: ConstBool<true>,
    },
    /// A streamed source that must be read sequentially.
    Streamed(StreamedSource),
}

/// A URL data source which does not support range requests.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamedSource {
    /// URL to fetch the data from. Can be absolute or relative.
    /// If `id` is absent, this must uniquely identify the data.
    pub url: String,
    /// Identifier for the data source. If present, this must be unique.
    #[serde(default)]
    pub id: Option<String>,
    /// Topics present in the data.
    pub topics: Vec<Topic>,
    /// Schemas present in the data.
    pub schemas: Vec<Schema>,
    /// Earliest timestamp of any message in the data.
    pub start_time: DateTime<Utc>,
    /// Latest timestamp of any message in the data.
    pub end_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct Topic {
    pub name: String,
    pub message_encoding: String,
    pub schema_id: Option<NonZeroU16>,
}

#[serde_as]
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct Schema {
    pub id: NonZeroU16,
    pub name: String,
    pub encoding: String,
    #[serde_as(as = "Base64")]
    pub data: Box<[u8]>,
}
