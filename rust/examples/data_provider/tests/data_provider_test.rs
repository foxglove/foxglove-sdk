//! End-to-end tests for the data_provider example.
//!
//! These tests verify that:
//! 1. The manifest endpoint conforms to the data provider HTTP API JSON schema.
//! 2. Following the data URLs in the manifest returns valid MCAP whose schemas
//!    and channels match what is declared in the manifest.
//!
//! The tests launch the example binary as a subprocess and make real HTTP
//! requests against it, so no changes to the example code are needed.

use std::collections::HashSet;
use std::net::TcpStream;
use std::process::{Child, Stdio};
use std::sync::{Mutex, Once, OnceLock};
use std::time::Duration;

use foxglove::data_provider::{Manifest, StreamedSource, UpstreamSource};
use reqwest::blocking::Client;

const BASE_URL: &str = "http://127.0.0.1:8080";
const BIND_ADDR: &str = "127.0.0.1:8080";

// ---------------------------------------------------------------------------
// Shared fixtures
// ---------------------------------------------------------------------------

static START_SERVER: Once = Once::new();
static SERVER_CHILD: Mutex<Option<Child>> = Mutex::new(None);

/// Spawn the example binary exactly once and wait until it accepts connections.
fn ensure_server() {
    START_SERVER.call_once(|| {
        let bin = env!("CARGO_BIN_EXE_example_data_provider");
        let child = std::process::Command::new(bin)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("should be able to start example_data_provider binary");

        *SERVER_CHILD.lock().unwrap() = Some(child);

        for _ in 0..100 {
            if TcpStream::connect(BIND_ADDR).is_ok() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("example_data_provider should become ready within 5 s");
    });
}

/// The manifest response, cached as both raw JSON (for schema validation) and
/// as a deserialized [`Manifest`] (for typed assertions).
struct CachedManifest {
    json: serde_json::Value,
    typed: Manifest,
}

static MANIFEST: OnceLock<CachedManifest> = OnceLock::new();

/// Return the cached manifest, fetching it on first call.
fn manifest() -> &'static CachedManifest {
    MANIFEST.get_or_init(|| {
        ensure_server();
        let resp = Client::new()
            .get(manifest_url())
            .bearer_auth("test-token")
            .send()
            .expect("manifest request should succeed");
        assert_eq!(resp.status(), 200, "manifest endpoint should return 200");
        let json: serde_json::Value = resp.json().expect("manifest response should be valid JSON");
        let typed: Manifest = serde_json::from_value(json.clone())
            .expect("manifest should deserialize into typed Manifest");
        CachedManifest { json, typed }
    })
}

fn manifest_url() -> String {
    format!(
        "{BASE_URL}/v1/manifest?flightId=TEST123\
         &startTime=2024-01-01T00:00:00Z\
         &endTime=2024-01-01T00:00:05Z"
    )
}

fn resolve_data_url(url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("{BASE_URL}{url}")
    }
}

/// Extract the [`StreamedSource`] from an [`UpstreamSource`], panicking on
/// static-file sources (which this example does not produce).
fn streamed(source: &UpstreamSource) -> &StreamedSource {
    match source {
        UpstreamSource::Streamed(s) => s,
        other => panic!("source should be Streamed, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 1. Manifest conforms to the data provider HTTP API
// ---------------------------------------------------------------------------

#[test]
fn manifest_matches_json_schema() {
    let m = manifest();

    let schema: serde_json::Value =
        serde_json::from_str(include_str!("data_provider_manifest_schema.json"))
            .expect("schema file should be valid JSON");
    let validator = jsonschema::options()
        .with_draft(jsonschema::Draft::Draft7)
        .build(&schema)
        .expect("schema should compile");

    let errors: Vec<String> = validator
        .iter_errors(&m.json)
        .map(|e| format!("  - {e}"))
        .collect();
    assert!(
        errors.is_empty(),
        "manifest should conform to the JSON schema:\n{}",
        errors.join("\n")
    );
}

#[test]
fn manifest_schema_ids_are_consistent() {
    let m = manifest();

    for source in &m.typed.sources {
        let s = streamed(source);
        let schema_ids: HashSet<_> = s.schemas.iter().map(|s| s.id).collect();
        assert_eq!(
            schema_ids.len(),
            s.schemas.len(),
            "schema IDs should be unique within a source"
        );
        for topic in &s.topics {
            if let Some(sid) = topic.schema_id {
                assert!(
                    schema_ids.contains(&sid),
                    "topic '{}' references schemaId {sid} which should exist in schemas",
                    topic.name,
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 2. MCAP data is valid and matches the manifest
// ---------------------------------------------------------------------------

#[test]
fn mcap_data_matches_manifest() {
    let m = manifest();
    let client = Client::new();

    for source in &m.typed.sources {
        let s = streamed(source);
        let full_url = resolve_data_url(&s.url);

        let resp = client
            .get(&full_url)
            .bearer_auth("test-token")
            .send()
            .expect("data request should succeed");
        assert_eq!(resp.status(), 200, "data endpoint should return 200");

        let mcap_bytes = resp.bytes().expect("should be able to read response body");
        assert!(!mcap_bytes.is_empty(), "MCAP response should not be empty");

        // --- MCAP is structurally valid ----------------------------------

        let summary = mcap::Summary::read(&mcap_bytes[..])
            .expect("MCAP should be readable")
            .expect("MCAP should contain a summary");

        let stats = summary.stats.as_ref().expect("MCAP should have stats");
        assert!(stats.message_count > 0, "MCAP should contain messages");

        // --- Schemas in MCAP match the manifest --------------------------

        for ms in &s.schemas {
            let mcap_schema = summary
                .schemas
                .get(&ms.id.get())
                .unwrap_or_else(|| panic!("manifest schema {} should exist in MCAP", ms.id));
            assert_eq!(
                mcap_schema.name, ms.name,
                "MCAP schema name should match manifest"
            );
            assert_eq!(
                mcap_schema.encoding, ms.encoding,
                "MCAP schema encoding should match manifest"
            );
            assert_eq!(
                mcap_schema.data.as_ref(),
                ms.data.as_ref(),
                "MCAP schema data should match manifest"
            );
        }

        // --- Channels in MCAP match the manifest topics ------------------

        for channel in summary.channels.values() {
            let mt = s
                .topics
                .iter()
                .find(|t| t.name == channel.topic)
                .unwrap_or_else(|| {
                    panic!(
                        "MCAP channel '{}' should have a corresponding manifest topic",
                        channel.topic
                    )
                });
            if let Some(expected_sid) = mt.schema_id {
                let schema = channel.schema.as_ref().unwrap_or_else(|| {
                    panic!("MCAP channel '{}' should have a schema", channel.topic)
                });
                assert_eq!(
                    schema.id,
                    expected_sid.get(),
                    "MCAP channel '{}' schema id should match manifest",
                    channel.topic
                );
            }
        }

        // --- Every message is on a known topic with matching encoding ----

        let mut seen_topics = HashSet::new();
        for message in mcap::MessageStream::new(&mcap_bytes[..])
            .expect("should be able to create message stream")
        {
            let message = message.expect("should be able to read MCAP message");
            let topic = &message.channel.topic;
            seen_topics.insert(topic.clone());

            let mt = s
                .topics
                .iter()
                .find(|t| t.name == *topic)
                .unwrap_or_else(|| {
                    panic!("MCAP topic '{topic}' should have a corresponding manifest topic")
                });
            assert_eq!(
                message.channel.message_encoding, mt.message_encoding,
                "MCAP message encoding for topic '{topic}' should match manifest"
            );
        }

        for mt in &s.topics {
            assert!(
                seen_topics.contains(&mt.name),
                "manifest topic '{}' should have messages in MCAP",
                mt.name
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 3. Auth enforcement
// ---------------------------------------------------------------------------

#[test]
fn manifest_requires_auth() {
    ensure_server();
    let status = Client::new().get(manifest_url()).send().unwrap().status();
    assert_eq!(status, 401, "manifest without auth should return 401");
}

#[test]
fn data_requires_auth() {
    ensure_server();
    let status = Client::new()
        .get(format!(
            "{BASE_URL}/v1/data?flightId=TEST123\
             &startTime=2024-01-01T00:00:00Z\
             &endTime=2024-01-01T00:00:05Z"
        ))
        .send()
        .unwrap()
        .status();
    assert_eq!(status, 401, "data without auth should return 401");
}
