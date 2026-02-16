//! End-to-end tests for the data_provider example.
//!
//! These tests verify that:
//! 1. The manifest endpoint conforms to the data provider HTTP API JSON schema.
//! 2. Following the data URLs in the manifest returns valid MCAP whose schemas
//!    and channels match what is declared in the manifest.
//!
//! This file uses `harness = false` with [`libtest_mimic`] so that `main` owns
//! the server child process. The child is spawned with
//! [`tokio::process::Command::kill_on_drop`], so it is killed reliably on
//! both normal return and panic unwinding.

use std::collections::{HashMap, HashSet};
use std::net::TcpStream;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use foxglove::data_provider::{Manifest, StreamedSource, UpstreamSource};
use libtest_mimic::{Arguments, Trial};
use reqwest::blocking::Client;

const BASE_URL: &str = "http://127.0.0.1:8080";
const BIND_ADDR: &str = "127.0.0.1:8080";

// ---------------------------------------------------------------------------
// Server lifecycle
// ---------------------------------------------------------------------------

/// A running server whose child process is killed on drop.
struct Server {
    _child: tokio::process::Child,
    _runtime: tokio::runtime::Runtime,
}

/// Spawn the example binary and wait until it accepts connections.
fn start_server() -> Server {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("should be able to build tokio runtime");

    let child = runtime.block_on(async {
        tokio::process::Command::new(env!("CARGO_BIN_EXE_example_data_provider"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .expect("should be able to start example_data_provider binary")
    });

    for _ in 0..100 {
        if TcpStream::connect(BIND_ADDR).is_ok() {
            return Server {
                _child: child,
                _runtime: runtime,
            };
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("example_data_provider should become ready within 5 s");
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

fn streamed(source: &UpstreamSource) -> &StreamedSource {
    match source {
        UpstreamSource::Streamed(s) => s,
        other => panic!("source should be Streamed, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Checks
// ---------------------------------------------------------------------------

fn manifest_matches_json_schema(json: &serde_json::Value) {
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("data_provider_manifest_schema.json"))
            .expect("schema file should be valid JSON");
    let validator = jsonschema::options()
        .with_draft(jsonschema::Draft::Draft7)
        .build(&schema)
        .expect("schema should compile");

    let errors: Vec<String> = validator
        .iter_errors(json)
        .map(|e| format!("  - {e}"))
        .collect();
    assert!(
        errors.is_empty(),
        "manifest should conform to the JSON schema:\n{}",
        errors.join("\n")
    );
}

fn manifest_schema_ids_are_consistent(manifest: &Manifest) {
    for source in &manifest.sources {
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

fn mcap_data_matches_manifest(client: &Client, manifest: &Manifest) {
    for source in &manifest.sources {
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

        // Index manifest entries by name / id for O(1) lookups.
        let topics_by_name: HashMap<&str, &foxglove::data_provider::Topic> =
            s.topics.iter().map(|t| (t.name.as_str(), t)).collect();
        let schemas_by_id: HashMap<u16, &foxglove::data_provider::Schema> =
            s.schemas.iter().map(|s| (s.id.get(), s)).collect();

        // --- Every message's channel should match a manifest topic --------
        //
        // For each message we check that:
        //   - the topic is declared in the manifest,
        //   - the channel's message encoding matches, and
        //   - if a schema is declared, the schema's name, encoding, and data
        //     match the manifest (not just the id).
        //
        // `seen_topics` tracks which manifest topics actually had messages,
        // so we can verify at the end that no manifest topic went unseen.

        let mut seen_topics = HashSet::new();
        for message in mcap::MessageStream::new(&mcap_bytes[..])
            .expect("should be able to create message stream")
        {
            let message = message.expect("should be able to read MCAP message");
            let channel = &message.channel;
            let topic = channel.topic.as_str();
            seen_topics.insert(topic.to_owned());

            let mt = topics_by_name.get(topic).unwrap_or_else(|| {
                panic!("MCAP message on topic '{topic}' should have a corresponding manifest entry")
            });

            assert_eq!(
                channel.message_encoding, mt.message_encoding,
                "MCAP channel encoding for topic '{topic}' should match manifest"
            );

            if let Some(expected_sid) = mt.schema_id {
                let mcap_schema = channel.schema.as_ref().unwrap_or_else(|| {
                    panic!("MCAP channel for topic '{topic}' should have a schema")
                });
                let manifest_schema = schemas_by_id.get(&expected_sid.get()).unwrap_or_else(|| {
                    panic!(
                        "manifest schemaId {} for topic '{topic}' should exist in schemas",
                        expected_sid
                    )
                });
                assert_eq!(
                    mcap_schema.name, manifest_schema.name,
                    "MCAP schema name for topic '{topic}' should match manifest"
                );
                assert_eq!(
                    mcap_schema.encoding, manifest_schema.encoding,
                    "MCAP schema encoding for topic '{topic}' should match manifest"
                );
                assert_eq!(
                    mcap_schema.data.as_ref(),
                    manifest_schema.data.as_ref(),
                    "MCAP schema data for topic '{topic}' should match manifest"
                );
            }
        }

        for name in topics_by_name.keys() {
            assert!(
                seen_topics.contains(*name),
                "manifest topic '{name}' should have messages in MCAP"
            );
        }
    }
}

fn auth_required(client: &Client) {
    let status = client.get(manifest_url()).send().unwrap().status();
    assert_eq!(status, 401, "manifest without auth should return 401");

    let status = client
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

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    let args = Arguments::from_args();

    let _server = start_server();
    let client = Client::new();

    // Fetch the manifest once.
    let resp = client
        .get(manifest_url())
        .bearer_auth("test-token")
        .send()
        .expect("manifest request should succeed");
    assert_eq!(resp.status(), 200, "manifest endpoint should return 200");
    let json: Arc<serde_json::Value> =
        Arc::new(resp.json().expect("manifest response should be valid JSON"));
    let manifest: Arc<Manifest> = Arc::new(
        serde_json::from_value((*json).clone())
            .expect("manifest should deserialize into typed Manifest"),
    );

    let tests = vec![
        {
            let json = Arc::clone(&json);
            Trial::test("manifest_matches_json_schema", move || {
                manifest_matches_json_schema(&json);
                Ok(())
            })
        },
        {
            let manifest = Arc::clone(&manifest);
            Trial::test("manifest_schema_ids_are_consistent", move || {
                manifest_schema_ids_are_consistent(&manifest);
                Ok(())
            })
        },
        {
            let client = client.clone();
            let manifest = Arc::clone(&manifest);
            Trial::test("mcap_data_matches_manifest", move || {
                mcap_data_matches_manifest(&client, &manifest);
                Ok(())
            })
        },
        {
            let client = client.clone();
            Trial::test("auth_required", move || {
                auth_required(&client);
                Ok(())
            })
        },
    ];

    libtest_mimic::run(&args, tests).exit();
    // _server is dropped after run() returns — kill_on_drop kills the child.
}
