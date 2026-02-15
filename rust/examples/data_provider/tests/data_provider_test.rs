//! Integration tests for the data_provider example.
//!
//! These tests verify that:
//! 1. The manifest endpoint conforms to the data provider HTTP API JSON schema.
//! 2. Following the data URLs in the manifest returns valid MCAP whose schemas
//!    and channels match what is declared in the manifest.

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;

use base64::Engine;
use serde_json::Value;

/// Load the JSON schema from the co-located file.
fn load_manifest_schema() -> Value {
    let schema_bytes = include_str!("data_provider_manifest_schema.json");
    serde_json::from_str(schema_bytes).expect("schema file should be valid JSON")
}

/// Start the example server on an OS-assigned port and return its base URL.
async fn start_server() -> String {
    let app = example_data_provider::app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind");
    let addr: SocketAddr = listener.local_addr().expect("failed to get local addr");

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    format!("http://{addr}")
}

/// Fetch the manifest for a test flight from the given base URL.
async fn fetch_manifest(base_url: &str) -> reqwest::Response {
    let client = reqwest::Client::new();
    client
        .get(format!(
            "{base_url}/v1/manifest?flightId=TEST123\
             &startTime=2024-01-01T00:00:00Z\
             &endTime=2024-01-01T00:00:05Z"
        ))
        .header("Authorization", "Bearer test-token")
        .send()
        .await
        .expect("manifest request failed")
}

// ---------------------------------------------------------------------------
// 1. Manifest conforms to the JSON schema
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn manifest_matches_json_schema() {
    let base_url = start_server().await;
    let resp = fetch_manifest(&base_url).await;

    assert_eq!(resp.status(), 200, "manifest endpoint should return 200");

    let body: Value = resp.json().await.expect("response should be valid JSON");
    let schema_value = load_manifest_schema();

    // Build the validator on a blocking thread because the jsonschema crate may
    // internally spawn a blocking runtime (for meta-schema retrieval) that
    // conflicts with the test's tokio runtime.
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
async fn manifest_has_required_fields() {
    let base_url = start_server().await;
    let resp = fetch_manifest(&base_url).await;
    let body: Value = resp.json().await.unwrap();

    // `sources` must be present and non-empty.
    let sources = body["sources"]
        .as_array()
        .expect("sources should be an array");
    assert!(!sources.is_empty(), "sources should not be empty");

    for source in sources {
        // Each streamed source must have topics, schemas, startTime, endTime.
        assert!(source["url"].is_string(), "source should have a url");
        assert!(source["topics"].is_array(), "source should have topics");
        assert!(source["schemas"].is_array(), "source should have schemas");
        assert!(
            source["startTime"].is_string(),
            "source should have startTime"
        );
        assert!(
            source["endTime"].is_string(),
            "source should have endTime"
        );

        // Every topic must have name and messageEncoding.
        for topic in source["topics"].as_array().unwrap() {
            assert!(topic["name"].is_string(), "topic must have a name");
            assert!(
                topic["messageEncoding"].is_string(),
                "topic must have messageEncoding"
            );
        }

        // Every schema must have id, name, encoding, data.
        for schema in source["schemas"].as_array().unwrap() {
            assert!(schema["id"].is_number(), "schema must have an id");
            assert!(schema["name"].is_string(), "schema must have a name");
            assert!(
                schema["encoding"].is_string(),
                "schema must have an encoding"
            );
            assert!(schema["data"].is_string(), "schema must have data");

            // Verify that the data field is valid base64.
            let data_str = schema["data"].as_str().unwrap();
            base64::engine::general_purpose::STANDARD
                .decode(data_str)
                .expect("schema data should be valid base64");
        }
    }
}

#[tokio::test]
async fn manifest_schema_ids_are_consistent() {
    let base_url = start_server().await;
    let resp = fetch_manifest(&base_url).await;
    let body: Value = resp.json().await.unwrap();

    let sources = body["sources"].as_array().unwrap();
    for source in sources {
        let schemas = source["schemas"].as_array().unwrap();
        let schema_ids: Vec<u64> = schemas
            .iter()
            .map(|s| s["id"].as_u64().unwrap())
            .collect();

        // Schema IDs should be unique within a source.
        let unique: HashSet<u64> = schema_ids.iter().copied().collect();
        assert_eq!(
            unique.len(),
            schema_ids.len(),
            "schema IDs must be unique within a source"
        );

        // Every schemaId referenced by a topic must exist in the schemas array.
        let topics = source["topics"].as_array().unwrap();
        for topic in topics {
            if let Some(schema_id) = topic.get("schemaId").and_then(|v| v.as_u64()) {
                assert!(
                    unique.contains(&schema_id),
                    "topic '{}' references schemaId {} which is not in schemas",
                    topic["name"].as_str().unwrap_or("<unknown>"),
                    schema_id
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
    let base_url = start_server().await;
    let resp = fetch_manifest(&base_url).await;
    let manifest: Value = resp.json().await.unwrap();

    let sources = manifest["sources"].as_array().unwrap();
    let client = reqwest::Client::new();

    for source in sources {
        let data_url = source["url"].as_str().expect("source should have a url");

        // The example returns relative URLs; resolve them against the base.
        let full_url = if data_url.starts_with("http://") || data_url.starts_with("https://") {
            data_url.to_string()
        } else {
            format!("{base_url}{data_url}")
        };

        let data_resp = client
            .get(&full_url)
            .header("Authorization", "Bearer test-token")
            .send()
            .await
            .expect("data request failed");

        assert_eq!(
            data_resp.status(),
            200,
            "data endpoint should return 200 for {full_url}"
        );

        let mcap_bytes = data_resp.bytes().await.expect("failed to read data body");
        assert!(!mcap_bytes.is_empty(), "MCAP response should not be empty");

        // Parse the MCAP and verify it is structurally valid.
        let summary = mcap::Summary::read(&mcap_bytes[..])
            .expect("MCAP data should be readable")
            .expect("MCAP should contain a summary section");

        let stats = summary.stats.expect("MCAP should have stats");
        assert!(stats.message_count > 0, "MCAP should contain messages");
    }
}

#[tokio::test]
async fn mcap_channels_match_manifest_topics() {
    let base_url = start_server().await;
    let resp = fetch_manifest(&base_url).await;
    let manifest: Value = resp.json().await.unwrap();

    let sources = manifest["sources"].as_array().unwrap();
    let client = reqwest::Client::new();

    for source in sources {
        let data_url = source["url"].as_str().unwrap();
        let full_url = if data_url.starts_with("http://") || data_url.starts_with("https://") {
            data_url.to_string()
        } else {
            format!("{base_url}{data_url}")
        };

        let data_resp = client
            .get(&full_url)
            .header("Authorization", "Bearer test-token")
            .send()
            .await
            .unwrap();

        let mcap_bytes = data_resp.bytes().await.unwrap();

        // Build lookup of manifest topics: name -> (messageEncoding, schemaId).
        let manifest_topics: HashMap<String, (String, Option<u64>)> = source["topics"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| {
                let name = t["name"].as_str().unwrap().to_string();
                let encoding = t["messageEncoding"].as_str().unwrap().to_string();
                let schema_id = t.get("schemaId").and_then(|v| v.as_u64());
                (name, (encoding, schema_id))
            })
            .collect();

        // Build lookup of manifest schemas: id -> (name, encoding, data).
        let manifest_schemas: HashMap<u64, (String, String, Vec<u8>)> = source["schemas"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                let id = s["id"].as_u64().unwrap();
                let name = s["name"].as_str().unwrap().to_string();
                let encoding = s["encoding"].as_str().unwrap().to_string();
                let data = base64::engine::general_purpose::STANDARD
                    .decode(s["data"].as_str().unwrap())
                    .unwrap();
                (id, (name, encoding, data))
            })
            .collect();

        // Read all messages from the MCAP and verify each channel matches the manifest.
        let stream =
            mcap::MessageStream::new(&mcap_bytes[..]).expect("failed to create message stream");

        let mut seen_topics: HashSet<String> = HashSet::new();

        for message in stream {
            let message = message.expect("failed to read message");
            let topic = message.channel.topic.as_str();
            seen_topics.insert(topic.to_string());

            // The topic must exist in the manifest.
            let (expected_encoding, expected_schema_id) = manifest_topics
                .get(topic)
                .unwrap_or_else(|| panic!("MCAP topic '{topic}' not found in manifest"));

            // Message encoding must match.
            assert_eq!(
                &message.channel.message_encoding, expected_encoding,
                "message encoding mismatch for topic '{topic}'"
            );

            // If the manifest declares a schemaId, verify the MCAP channel's schema matches.
            if let Some(manifest_schema_id) = expected_schema_id {
                let schema = message.channel.schema.as_ref().unwrap_or_else(|| {
                    panic!(
                        "MCAP channel '{topic}' has no schema but manifest declares schemaId {manifest_schema_id}"
                    )
                });

                let (expected_name, expected_schema_encoding, expected_data) = manifest_schemas
                    .get(manifest_schema_id)
                    .unwrap_or_else(|| {
                        panic!("manifest schemaId {manifest_schema_id} not found in schemas array")
                    });

                assert_eq!(
                    &schema.name, expected_name,
                    "schema name mismatch for topic '{topic}'"
                );
                assert_eq!(
                    &schema.encoding, expected_schema_encoding,
                    "schema encoding mismatch for topic '{topic}'"
                );
                assert_eq!(
                    schema.data.as_ref(),
                    expected_data.as_slice(),
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
    let base_url = start_server().await;
    let resp = fetch_manifest(&base_url).await;
    let manifest: Value = resp.json().await.unwrap();

    let sources = manifest["sources"].as_array().unwrap();
    let client = reqwest::Client::new();

    for source in sources {
        let data_url = source["url"].as_str().unwrap();
        let full_url = if data_url.starts_with("http://") || data_url.starts_with("https://") {
            data_url.to_string()
        } else {
            format!("{base_url}{data_url}")
        };

        let data_resp = client
            .get(&full_url)
            .header("Authorization", "Bearer test-token")
            .send()
            .await
            .unwrap();

        let mcap_bytes = data_resp.bytes().await.unwrap();

        let summary = mcap::Summary::read(&mcap_bytes[..])
            .unwrap()
            .expect("should have summary");

        // Manifest schemas: id -> (name, encoding, data).
        let manifest_schemas: HashMap<u64, (&str, &str, Vec<u8>)> = source["schemas"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                let id = s["id"].as_u64().unwrap();
                let name = s["name"].as_str().unwrap();
                let encoding = s["encoding"].as_str().unwrap();
                let data = base64::engine::general_purpose::STANDARD
                    .decode(s["data"].as_str().unwrap())
                    .unwrap();
                (id, (name, encoding, data))
            })
            .collect();

        // For each schema declared in the manifest, verify a matching MCAP schema exists.
        for (manifest_id, (m_name, m_encoding, m_data)) in &manifest_schemas {
            let mcap_schema =
                summary
                    .schemas
                    .get(&(*manifest_id as u16))
                    .unwrap_or_else(|| {
                        panic!("manifest schema id {manifest_id} not found in MCAP schemas")
                    });

            assert_eq!(
                mcap_schema.name, *m_name,
                "schema name mismatch for id {manifest_id}"
            );
            assert_eq!(
                mcap_schema.encoding, *m_encoding,
                "schema encoding mismatch for id {manifest_id}"
            );
            assert_eq!(
                mcap_schema.data.as_ref(),
                m_data.as_slice(),
                "schema data mismatch for id {manifest_id}"
            );
        }

        // Manifest topics: name -> schemaId.
        let manifest_topics: HashMap<&str, Option<u64>> = source["topics"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| {
                let name = t["name"].as_str().unwrap();
                let schema_id = t.get("schemaId").and_then(|v| v.as_u64());
                (name, schema_id)
            })
            .collect();

        // For each channel in the MCAP, verify it corresponds to a manifest topic.
        for channel in summary.channels.values() {
            let manifest_schema_id =
                manifest_topics
                    .get(channel.topic.as_str())
                    .unwrap_or_else(|| {
                        panic!(
                            "MCAP channel topic '{}' not found in manifest topics",
                            channel.topic
                        )
                    });

            if let Some(expected_schema_id) = manifest_schema_id {
                // The channel references a schema; verify the schema_id matches through the schema
                // attached to the channel.
                let schema = channel.schema.as_ref().unwrap_or_else(|| {
                    panic!(
                        "MCAP channel '{}' has no schema but manifest declares schemaId {expected_schema_id}",
                        channel.topic
                    )
                });

                assert_eq!(
                    schema.id as u64, *expected_schema_id,
                    "channel '{}' schema id mismatch",
                    channel.topic
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 3. Edge cases and error handling
// ---------------------------------------------------------------------------

#[tokio::test]
async fn manifest_requires_auth() {
    let base_url = start_server().await;
    let client = reqwest::Client::new();

    // Request without Authorization header should be rejected.
    let resp = client
        .get(format!(
            "{base_url}/v1/manifest?flightId=TEST123\
             &startTime=2024-01-01T00:00:00Z\
             &endTime=2024-01-01T00:00:05Z"
        ))
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
    let base_url = start_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!(
            "{base_url}/v1/data?flightId=TEST123\
             &startTime=2024-01-01T00:00:00Z\
             &endTime=2024-01-01T00:00:05Z"
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 401, "data without auth should return 401");
}
