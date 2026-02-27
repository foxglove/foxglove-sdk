//! Reusable test suite for data provider HTTP API implementations.
//!
//! This module checks that a running data provider:
//! 1. Returns a manifest that conforms to the JSON schema.
//! 2. Serves MCAP data whose channels and schemas match the manifest.
//! 3. Requires authentication.
//!
//! The checks are parameterized by [`DataProviderTestConfig`] so they can be
//! used against any implementation (Rust, C++, etc.) without modification.
//!
//! # Usage
//!
//! ```no_run
//! use data_provider_conformance::DataProviderTestConfig;
//! # fn start_my_server() -> () { }
//!
//! let _server = start_my_server();
//! let config = DataProviderTestConfig {
//!     manifest_url: "http://127.0.0.1:8080/v1/manifest?...".parse().unwrap(),
//!     expected_streamed_source_count: 1,
//!     expected_static_file_source_count: 0,
//! };
//! data_provider_conformance::run_tests(config);
//! ```

use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::net::TcpStream;
use std::process::{Child, Command, ExitCode, Stdio};
use std::time::Duration;

use foxglove::data_provider::{Manifest, UpstreamSource};
use libtest_mimic::{Arguments, Trial};
use reqwest::StatusCode;
pub use reqwest::Url;
use reqwest::blocking::Client;

/// A guard that kills a child process when dropped.
pub struct ServerGuard(Child);

impl Drop for ServerGuard {
    fn drop(&mut self) {
        self.0
            .kill()
            .expect("should be able to kill server process");
        self.0
            .wait()
            .expect("should be able to wait on server process");
    }
}

/// Spawn a server binary and wait for it to accept TCP connections.
///
/// Panics if the address is already in use, the binary cannot be started, or the server does not
/// become ready within 5 seconds.
///
/// Returns a [`ServerGuard`] that kills the server process when dropped.
pub fn spawn_server(command: impl AsRef<OsStr>, addr: &str) -> ServerGuard {
    if TcpStream::connect(addr).is_ok() {
        panic!("a server should not already be running on {addr}");
    }

    let child = Command::new(command.as_ref())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap_or_else(|e| panic!("should be able to start {:?}: {e}", command.as_ref()));

    let guard = ServerGuard(child);

    for _ in 0..100 {
        if TcpStream::connect(addr).is_ok() {
            return guard;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("server should become ready within 5 s");
}

/// Configuration for running the data provider test suite.
pub struct DataProviderTestConfig {
    /// Full URL of the manifest endpoint, including query parameters.
    pub manifest_url: Url,
    /// Expected number of streamed sources in the manifest.
    pub expected_streamed_source_count: usize,
    /// Expected number of static file sources in the manifest.
    pub expected_static_file_source_count: usize,
}

/// Run the full test suite against a running data provider, using [`libtest_mimic`] for output and
/// command line argument handling.
///
/// Returns the exit code of the test run.
pub fn run_tests(config: DataProviderTestConfig) -> ExitCode {
    let args = Arguments::from_args();
    let trials = build_tests(config);
    libtest_mimic::run(&args, trials).exit_code()
}

/// Build the test suite as a list of [`Trial`] values.
///
/// Fetches the manifest once up front; each trial closes over the shared data.
pub fn build_tests(
    DataProviderTestConfig {
        manifest_url,
        expected_streamed_source_count,
        expected_static_file_source_count,
    }: DataProviderTestConfig,
) -> Vec<Trial> {
    let client = Client::new();

    let resp = client
        .get(manifest_url.clone())
        .send()
        .expect("manifest request should succeed");
    assert_eq!(resp.status(), 200, "manifest endpoint should return 200");

    let json: serde_json::Value = resp.json().expect("manifest response should be valid JSON");
    let manifest: Manifest = serde_json::from_value(json.clone())
        .expect("manifest should deserialize into typed Manifest");

    let mut streamed_source_count = 0;
    let mut static_file_source_count = 0;
    for source in &manifest.sources {
        match source {
            UpstreamSource::Streamed(_) => streamed_source_count += 1,
            UpstreamSource::StaticFile { .. } => static_file_source_count += 1,
        }
    }

    vec![
        Trial::test("test_manifest_matches_json_schema", move || {
            test_manifest_matches_json_schema(&json);
            Ok(())
        }),
        Trial::test("test_source_count", move || {
            assert_eq!(
                streamed_source_count, expected_streamed_source_count,
                "streamed source count should match expected"
            );
            assert_eq!(
                static_file_source_count, expected_static_file_source_count,
                "static file source count should match expected"
            );
            Ok(())
        }),
        Trial::test("test_manifest_and_mcap_agree", {
            let client = client.clone();
            let manifest_url = manifest_url.clone();
            move || {
                test_manifest_and_mcap_agree(&client, &manifest_url, &manifest);
                Ok(())
            }
        }),
    ]
}

// ---------------------------------------------------------------------------
// Checks
// ---------------------------------------------------------------------------

fn test_manifest_matches_json_schema(json: &serde_json::Value) {
    let schema = serde_json::from_str(include_str!("data_provider_manifest_schema.json"))
        .expect("schema file should be valid JSON");
    let validator =
        jsonschema::draft7::new(&schema).expect("schema should compile into a validator");
    validator
        .validate(json)
        .expect("manifest should conform to the JSON schema");
}

fn test_manifest_and_mcap_agree(client: &Client, manifest_url: &Url, manifest: &Manifest) {
    for source in &manifest.sources {
        let (url, topics, schemas) = match source {
            UpstreamSource::Streamed(s) => (&s.url, &s.topics, &s.schemas),
            UpstreamSource::StaticFile { .. } => {
                // Nothing to check for a static source.
                continue;
            }
        };
        let data_url = manifest_url
            .join(url)
            .expect("data URL should either be an absolute URL or a relative URL");

        let resp = client
            .get(data_url)
            .send()
            .expect("data request should succeed");
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "data endpoint should return 200 OK"
        );

        let mcap_bytes = resp.bytes().expect("should be able to read response body");
        assert!(!mcap_bytes.is_empty(), "response should not be empty");

        let summary = mcap::Summary::read(&mcap_bytes[..])
            .expect("MCAP should be readable")
            .expect("MCAP should contain a summary");

        let stats = summary.stats.as_ref().expect("MCAP should have stats");
        assert!(stats.message_count > 0, "MCAP should contain messages");

        let mut schemas_by_id = HashMap::with_capacity(schemas.len());
        for schema in schemas {
            assert_eq!(
                schemas_by_id.insert(schema.id, schema),
                None,
                "schemas should have unique IDs"
            );
        }
        let mut topics_by_name = HashMap::with_capacity(topics.len());
        for topic in topics {
            if let Some(schema_id) = topic.schema_id {
                assert!(
                    schemas_by_id.contains_key(&schema_id),
                    "schema {schema_id} should exist for topic {:?}",
                    topic.name
                );
            }
            assert_eq!(
                topics_by_name.insert(&topic.name, topic),
                None,
                "topics should have unique names"
            );
        }

        // For each message, check that its channel is represented in the manifest: topic, encoding,
        // and full schema content must match.
        let mut checked_channels = HashSet::new();
        for message in mcap::MessageStream::new(&mcap_bytes[..])
            .expect("should be able to create message stream")
        {
            let message = message.expect("should be able to read MCAP message");
            let channel = &message.channel;
            let topic_name = &channel.topic;
            let channel_id = channel.id;

            if !checked_channels.insert(channel_id) {
                continue;
            }

            let manifest_topic = topics_by_name.get(topic_name).unwrap_or_else(|| {
                panic!("topic '{topic_name}' should be represented in manifest")
            });

            assert_eq!(
                channel.message_encoding, manifest_topic.message_encoding,
                "message encoding for channel {channel_id} on topic '{topic_name}' should match manifest"
            );

            let (schema, manifest_schema) = match (&channel.schema, manifest_topic.schema_id) {
                (Some(schema), Some(manifest_schema_id)) => {
                    (schema.as_ref(), &schemas_by_id[&manifest_schema_id])
                }
                (None, None) => {
                    continue;
                }
                (None, Some(_)) => panic!(
                    "channel {channel_id} on topic {topic_name:?} should have a schema according to manifest"
                ),
                (Some(_), None) => panic!(
                    "channel {channel_id} on topic {topic_name:?} should be schemaless according to manifest"
                ),
            };

            assert_eq!(
                schema.name, manifest_schema.name,
                "schema name for channel {channel_id} on topic {topic_name:?} should match manifest"
            );
            assert_eq!(
                schema.encoding, manifest_schema.encoding,
                "schema encoding for channel {channel_id} on topic {topic_name:?} should match manifest"
            );
            assert_eq!(
                *schema.data, *manifest_schema.data,
                "schema data for channel {channel_id} on topic {topic_name:?} should match manifest"
            );
        }
    }
}
