//! Reusable test suite for data provider HTTP API implementations.
//!
//! Enabled by the `test-harness` feature. This module checks that a running
//! data provider:
//! 1. Returns a manifest that conforms to the JSON schema.
//! 2. Serves MCAP data whose channels and schemas match the manifest.
//! 3. Requires authentication.
//!
//! The checks are parameterized by [`DataProviderTestConfig`] so they can be
//! used against any implementation (Rust, C++, etc.) without modification.
//!
//! # Usage
//!
//! ```ignore
//! use example_data_provider::DataProviderTestConfig;
//!
//! let _server = start_my_server();
//! let config = DataProviderTestConfig {
//!     base_url: "http://127.0.0.1:8080".into(),
//!     manifest_url: "http://127.0.0.1:8080/v1/manifest?...".into(),
//!     bearer_token: "my-token".into(),
//! };
//! example_data_provider::run_tests(&config);
//! ```

#![cfg(feature = "test-harness")]

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use foxglove::data_provider::{Manifest, StreamedSource, UpstreamSource};
use libtest_mimic::{Arguments, Trial};
use reqwest::blocking::Client;

/// Configuration for running the data provider test suite.
pub struct DataProviderTestConfig {
    /// Base URL of the server (e.g. `http://127.0.0.1:8080`), used to resolve
    /// relative data URLs that appear in the manifest.
    pub base_url: String,
    /// Full URL of the manifest endpoint, including query parameters.
    pub manifest_url: String,
    /// Bearer token for authenticated requests.
    pub bearer_token: String,
}

/// Run the full test suite against a running data provider, using
/// [`libtest_mimic`] for output and argument handling.
///
/// This parses `std::env::args()` for the standard test flags (`--filter`,
/// `--list`, etc.) and calls [`std::process::exit`] with the appropriate code.
pub fn run_tests(config: &DataProviderTestConfig) {
    let args = Arguments::from_args();
    let trials = build_tests(config);
    libtest_mimic::run(&args, trials).exit();
}

/// Build the test suite as a list of [`Trial`] values.
///
/// Fetches the manifest once up front; each trial closes over the shared data.
pub fn build_tests(config: &DataProviderTestConfig) -> Vec<Trial> {
    let client = Client::new();

    let resp = client
        .get(&config.manifest_url)
        .bearer_auth(&config.bearer_token)
        .send()
        .expect("manifest request should succeed");
    assert_eq!(resp.status(), 200, "manifest endpoint should return 200");

    let json: Arc<serde_json::Value> =
        Arc::new(resp.json().expect("manifest response should be valid JSON"));
    let manifest: Arc<Manifest> = Arc::new(
        serde_json::from_value((*json).clone())
            .expect("manifest should deserialize into typed Manifest"),
    );

    let manifest_url = config.manifest_url.clone();
    let base_url = config.base_url.clone();
    let bearer_token = config.bearer_token.clone();

    vec![
        {
            let json = Arc::clone(&json);
            Trial::test("manifest_matches_json_schema", move || {
                check_manifest_matches_json_schema(&json);
                Ok(())
            })
        },
        {
            let manifest = Arc::clone(&manifest);
            Trial::test("manifest_schema_ids_are_consistent", move || {
                check_manifest_schema_ids_are_consistent(&manifest);
                Ok(())
            })
        },
        {
            let client = client.clone();
            let manifest = Arc::clone(&manifest);
            let base_url = base_url.clone();
            let bearer_token = bearer_token.clone();
            Trial::test("mcap_data_matches_manifest", move || {
                check_mcap_data_matches_manifest(&client, &base_url, &bearer_token, &manifest);
                Ok(())
            })
        },
        {
            let client = client.clone();
            Trial::test("auth_required", move || {
                check_auth_required(&client, &manifest_url);
                Ok(())
            })
        },
    ]
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn resolve_data_url(base_url: &str, data_url: &str) -> url::Url {
    let base = url::Url::parse(base_url).expect("base_url should be a valid URL");
    base.join(data_url)
        .expect("data URL from manifest should be resolvable against base_url")
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

fn check_manifest_matches_json_schema(json: &serde_json::Value) {
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

fn check_manifest_schema_ids_are_consistent(manifest: &Manifest) {
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

fn check_mcap_data_matches_manifest(
    client: &Client,
    base_url: &str,
    bearer_token: &str,
    manifest: &Manifest,
) {
    for source in &manifest.sources {
        let s = streamed(source);
        let data_url = resolve_data_url(base_url, &s.url);

        let resp = client
            .get(data_url)
            .bearer_auth(bearer_token)
            .send()
            .expect("data request should succeed");
        assert_eq!(resp.status(), 200, "data endpoint should return 200");

        let mcap_bytes = resp.bytes().expect("should be able to read response body");
        assert!(!mcap_bytes.is_empty(), "MCAP response should not be empty");

        let summary = mcap::Summary::read(&mcap_bytes[..])
            .expect("MCAP should be readable")
            .expect("MCAP should contain a summary");

        let stats = summary.stats.as_ref().expect("MCAP should have stats");
        assert!(stats.message_count > 0, "MCAP should contain messages");

        let topics_by_name: HashMap<&str, &foxglove::data_provider::Topic> =
            s.topics.iter().map(|t| (t.name.as_str(), t)).collect();
        let schemas_by_id: HashMap<u16, &foxglove::data_provider::Schema> =
            s.schemas.iter().map(|s| (s.id.get(), s)).collect();

        // For each message, check that its channel is represented in the
        // manifest: topic, encoding, and full schema content must match.
        for message in mcap::MessageStream::new(&mcap_bytes[..])
            .expect("should be able to create message stream")
        {
            let message = message.expect("should be able to read MCAP message");
            let channel = &message.channel;
            let topic = channel.topic.as_str();

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
    }
}

fn check_auth_required(client: &Client, manifest_url: &str) {
    let status = client.get(manifest_url).send().unwrap().status();
    assert_eq!(status, 401, "manifest without auth should return 401");
}
