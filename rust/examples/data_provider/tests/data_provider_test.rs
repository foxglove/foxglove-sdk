//! End-to-end tests for the data_provider example.
//!
//! These tests verify that:
//! 1. The manifest endpoint conforms to the data provider HTTP API JSON schema.
//! 2. Following the data URLs in the manifest returns valid MCAP whose schemas
//!    and channels match what is declared in the manifest.
//!
//! The tests launch the example binary as a subprocess and make real HTTP
//! requests against it, so no restructuring of the example code is needed.

use std::collections::{HashMap, HashSet};
use std::net::TcpStream;
use std::process::{Child, Stdio};
use std::sync::{Mutex, Once};
use std::time::Duration;

use base64::Engine;
use serde_json::Value;

const BASE_URL: &str = "http://127.0.0.1:8080";
const BIND_ADDR: &str = "127.0.0.1:8080";

static START_SERVER: Once = Once::new();
static SERVER_CHILD: Mutex<Option<Child>> = Mutex::new(None);

/// Spawn the example binary exactly once and wait until it accepts connections.
///
/// The child process is stored in a global so the OS can reap it when the test
/// process exits. This keeps every test in the file cheap (no per-test
/// startup/shutdown) while requiring zero changes to the example itself.
fn ensure_server() {
    START_SERVER.call_once(|| {
        let bin = env!("CARGO_BIN_EXE_example_data_provider");
        let child = std::process::Command::new(bin)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to start example_data_provider binary");

        *SERVER_CHILD.lock().unwrap() = Some(child);

        // Poll until the server accepts TCP connections.
        for _ in 0..100 {
            if TcpStream::connect(BIND_ADDR).is_ok() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("example_data_provider did not become ready within 5 s");
    });
}

/// Build a manifest request URL with test parameters.
fn manifest_url() -> String {
    format!(
        "{BASE_URL}/v1/manifest?flightId=TEST123\
         &startTime=2024-01-01T00:00:00Z\
         &endTime=2024-01-01T00:00:05Z"
    )
}

/// Fetch the manifest as raw JSON.
async fn fetch_manifest_json() -> Value {
    let client = reqwest::Client::new();
    let resp = client
        .get(manifest_url())
        .header("Authorization", "Bearer test-token")
        .send()
        .await
        .expect("manifest request failed");

    assert_eq!(resp.status(), 200, "manifest endpoint should return 200");
    resp.json().await.expect("response should be valid JSON")
}

/// Load the JSON schema from the co-located file.
fn load_manifest_schema() -> Value {
    serde_json::from_str(include_str!("data_provider_manifest_schema.json"))
        .expect("schema file should be valid JSON")
}

/// Resolve a (possibly relative) URL from a manifest source against the base.
fn resolve_data_url(url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("{BASE_URL}{url}")
    }
}

// ---------------------------------------------------------------------------
// 1. Manifest conforms to the data provider HTTP API JSON schema
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn manifest_matches_json_schema() {
    ensure_server();
    let body = fetch_manifest_json().await;
    let schema_value = load_manifest_schema();

    // The jsonschema crate may internally spawn a blocking tokio runtime when
    // resolving meta-schemas, which conflicts with the test runtime. Run the
    // validation on a blocking thread to avoid the conflict.
    let errors = tokio::task::spawn_blocking(move || {
        let validator = jsonschema::options()
            .with_draft(jsonschema::Draft::Draft7)
            .build(&schema_value)
            .expect("schema should compile successfully");

        validator
            .iter_errors(&body)
            .map(|e| format!("  - {e}"))
            .collect::<Vec<String>>()
    })
    .await
    .expect("spawn_blocking failed");

    assert!(
        errors.is_empty(),
        "Manifest does not conform to the JSON schema:\n{}",
        errors.join("\n")
    );
}

#[tokio::test]
async fn manifest_schema_ids_are_consistent() {
    ensure_server();
    let body = fetch_manifest_json().await;

    for source in body["sources"].as_array().unwrap() {
        let schema_ids: Vec<u64> = source["schemas"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["id"].as_u64().unwrap())
            .collect();

        let unique: HashSet<u64> = schema_ids.iter().copied().collect();
        assert_eq!(
            unique.len(),
            schema_ids.len(),
            "schema IDs must be unique within a source"
        );

        for topic in source["topics"].as_array().unwrap() {
            if let Some(sid) = topic.get("schemaId").and_then(|v| v.as_u64()) {
                assert!(
                    unique.contains(&sid),
                    "topic '{}' references schemaId {sid} which is not in schemas",
                    topic["name"].as_str().unwrap_or("<unknown>"),
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 2. MCAP data is valid and matches the manifest
// ---------------------------------------------------------------------------

#[tokio::test]
async fn data_url_returns_valid_mcap() {
    ensure_server();
    let manifest = fetch_manifest_json().await;
    let client = reqwest::Client::new();

    for source in manifest["sources"].as_array().unwrap() {
        let full_url = resolve_data_url(source["url"].as_str().unwrap());

        let resp = client
            .get(&full_url)
            .header("Authorization", "Bearer test-token")
            .send()
            .await
            .expect("data request failed");

        assert_eq!(
            resp.status(),
            200,
            "data endpoint should return 200 for {full_url}"
        );

        let mcap_bytes = resp.bytes().await.expect("failed to read data body");
        assert!(!mcap_bytes.is_empty(), "MCAP response should not be empty");

        let summary = mcap::Summary::read(&mcap_bytes[..])
            .expect("MCAP data should be readable")
            .expect("MCAP should contain a summary section");

        let stats = summary.stats.expect("MCAP should have stats");
        assert!(stats.message_count > 0, "MCAP should contain messages");
    }
}

#[tokio::test]
async fn mcap_channels_match_manifest_topics() {
    ensure_server();
    let manifest = fetch_manifest_json().await;
    let client = reqwest::Client::new();

    for source in manifest["sources"].as_array().unwrap() {
        let full_url = resolve_data_url(source["url"].as_str().unwrap());

        let mcap_bytes = client
            .get(&full_url)
            .header("Authorization", "Bearer test-token")
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();

        // Manifest topic name -> (messageEncoding, schemaId).
        let manifest_topics: HashMap<String, (String, Option<u64>)> = source["topics"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| {
                let name = t["name"].as_str().unwrap().to_string();
                let enc = t["messageEncoding"].as_str().unwrap().to_string();
                let sid = t.get("schemaId").and_then(|v| v.as_u64());
                (name, (enc, sid))
            })
            .collect();

        // Manifest schema id -> (name, encoding, decoded data).
        let manifest_schemas: HashMap<u64, (String, String, Vec<u8>)> = source["schemas"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                let id = s["id"].as_u64().unwrap();
                let name = s["name"].as_str().unwrap().to_string();
                let enc = s["encoding"].as_str().unwrap().to_string();
                let data = base64::engine::general_purpose::STANDARD
                    .decode(s["data"].as_str().unwrap())
                    .unwrap();
                (id, (name, enc, data))
            })
            .collect();

        let stream =
            mcap::MessageStream::new(&mcap_bytes[..]).expect("failed to create message stream");

        let mut seen_topics: HashSet<String> = HashSet::new();

        for message in stream {
            let message = message.expect("failed to read MCAP message");
            let topic = message.channel.topic.as_str();
            seen_topics.insert(topic.to_string());

            let (expected_enc, expected_sid) = manifest_topics
                .get(topic)
                .unwrap_or_else(|| panic!("MCAP topic '{topic}' not found in manifest"));

            assert_eq!(
                &message.channel.message_encoding, expected_enc,
                "message encoding mismatch for topic '{topic}'"
            );

            if let Some(manifest_sid) = expected_sid {
                let schema = message.channel.schema.as_ref().unwrap_or_else(|| {
                    panic!(
                        "MCAP channel '{topic}' has no schema but manifest declares schemaId {manifest_sid}"
                    )
                });

                let (exp_name, exp_enc, exp_data) = manifest_schemas
                    .get(manifest_sid)
                    .unwrap_or_else(|| {
                        panic!("manifest schemaId {manifest_sid} not found in schemas array")
                    });

                assert_eq!(
                    &schema.name, exp_name,
                    "schema name mismatch for topic '{topic}'"
                );
                assert_eq!(
                    &schema.encoding, exp_enc,
                    "schema encoding mismatch for topic '{topic}'"
                );
                assert_eq!(
                    schema.data.as_ref(),
                    exp_data.as_slice(),
                    "schema data mismatch for topic '{topic}'"
                );
            }
        }

        // Every topic declared in the manifest should appear in the MCAP data.
        for topic_name in manifest_topics.keys() {
            assert!(
                seen_topics.contains(topic_name),
                "manifest topic '{topic_name}' not found in MCAP messages"
            );
        }
    }
}

#[tokio::test]
async fn mcap_schemas_match_manifest_schemas() {
    ensure_server();
    let manifest = fetch_manifest_json().await;
    let client = reqwest::Client::new();

    for source in manifest["sources"].as_array().unwrap() {
        let full_url = resolve_data_url(source["url"].as_str().unwrap());

        let mcap_bytes = client
            .get(&full_url)
            .header("Authorization", "Bearer test-token")
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();

        let summary = mcap::Summary::read(&mcap_bytes[..])
            .unwrap()
            .expect("should have summary");

        // Manifest schemas: id -> (name, encoding, decoded data).
        let manifest_schemas: HashMap<u64, (&str, &str, Vec<u8>)> = source["schemas"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                let id = s["id"].as_u64().unwrap();
                let name = s["name"].as_str().unwrap();
                let enc = s["encoding"].as_str().unwrap();
                let data = base64::engine::general_purpose::STANDARD
                    .decode(s["data"].as_str().unwrap())
                    .unwrap();
                (id, (name, enc, data))
            })
            .collect();

        for (mid, (m_name, m_enc, m_data)) in &manifest_schemas {
            let mcap_schema =
                summary
                    .schemas
                    .get(&(*mid as u16))
                    .unwrap_or_else(|| {
                        panic!("manifest schema id {mid} not found in MCAP schemas")
                    });

            assert_eq!(
                mcap_schema.name, *m_name,
                "schema name mismatch for id {mid}"
            );
            assert_eq!(
                mcap_schema.encoding, *m_enc,
                "schema encoding mismatch for id {mid}"
            );
            assert_eq!(
                mcap_schema.data.as_ref(),
                m_data.as_slice(),
                "schema data mismatch for id {mid}"
            );
        }

        // Manifest topic name -> schemaId.
        let manifest_topics: HashMap<&str, Option<u64>> = source["topics"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| {
                (
                    t["name"].as_str().unwrap(),
                    t.get("schemaId").and_then(|v| v.as_u64()),
                )
            })
            .collect();

        for channel in summary.channels.values() {
            let manifest_sid =
                manifest_topics
                    .get(channel.topic.as_str())
                    .unwrap_or_else(|| {
                        panic!(
                            "MCAP channel topic '{}' not found in manifest topics",
                            channel.topic
                        )
                    });

            if let Some(expected_sid) = manifest_sid {
                let schema = channel.schema.as_ref().unwrap_or_else(|| {
                    panic!(
                        "MCAP channel '{}' has no schema but manifest declares schemaId {expected_sid}",
                        channel.topic
                    )
                });
                assert_eq!(
                    schema.id as u64, *expected_sid,
                    "channel '{}' schema id mismatch",
                    channel.topic
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 3. Auth enforcement
// ---------------------------------------------------------------------------

#[tokio::test]
async fn manifest_requires_auth() {
    ensure_server();
    let client = reqwest::Client::new();

    let resp = client
        .get(manifest_url())
        .send()
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        401,
        "manifest without auth should return 401"
    );
}

#[tokio::test]
async fn data_requires_auth() {
    ensure_server();
    let client = reqwest::Client::new();

    let resp = client
        .get(format!(
            "{BASE_URL}/v1/data?flightId=TEST123\
             &startTime=2024-01-01T00:00:00Z\
             &endTime=2024-01-01T00:00:05Z"
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 401, "data without auth should return 401");
}
