//! Exercise writer destructor recovery in a subprocess so a double panic is fatal
//! to the child and cannot accidentally appear as a passing exception assertion.
use std::collections::BTreeMap;
use std::process::Command;
use std::sync::atomic::AtomicBool;

use foxglove_mcap_alignment::executor::{
    ExecuteLimits, Independence, Job, SampledStream, TimelineConfig, execute,
};
use foxglove_mcap_alignment::local_source::{Binding, LocalSources, SourceLimits};
use foxglove_mcap_alignment::{Episode, ErrorKind, Policy, Rate};

#[test]
fn output_errors_at_initial_mid_and_later_chunk_boundaries_do_not_abort() {
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "writer_failure_child", "--nocapture"])
        .env("FOXGLOVE_ALIGNMENT_CHUNK_FAILURE_CHILD", "1")
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "child aborted or failed: {}\n{}",
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
}
#[test]
fn writer_failure_child() {
    if std::env::var_os("FOXGLOVE_ALIGNMENT_CHUNK_FAILURE_CHILD").is_none() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.mcap");
    let mut writer = mcap::WriteOptions::new()
        .compression(None)
        .create(std::fs::File::create(&path).unwrap())
        .unwrap();
    let id = writer
        .add_channel(0, "/opaque", "custom", &BTreeMap::new())
        .unwrap();
    writer
        .write_to_known_channel(
            &mcap::records::MessageHeader {
                channel_id: id,
                sequence: 0,
                log_time: 1,
                publish_time: 1,
            },
            &vec![42; 600_000],
        )
        .unwrap();
    writer.finish().unwrap();
    drop(writer);
    let sources = LocalSources::open(
        std::slice::from_ref(&path),
        SourceLimits {
            index_memory_bytes: 64 * 1024,
            inventory_memory_bytes: 1024 * 1024,
            max_record_bytes: 1024 * 1024,
            max_chunk_bytes: 2 * 1024 * 1024,
            max_decompressed_chunk_bytes: 2 * 1024 * 1024,
            temporary_bytes: 128 * 1024 * 1024,
            max_open_files: 64,
        },
        dir.path(),
    )
    .unwrap();
    let job = |limit| Job {
        output: Default::default(),
        episode: Episode::new(1, 9).unwrap(),
        timeline: TimelineConfig::Fixed {
            rate: Rate::new(1_000_000_000, 1).unwrap(),
            origin: 1,
        },
        streams: vec![SampledStream {
            bindings: vec![Binding {
                file: 0,
                channel: id,
            }],
            output_topic: "/out".into(),
            policy: Policy::Previous,
            tolerance_ns: 8,
            required: true,
            independence: Independence::Declared,
        }],
        context: Vec::new(),
        limits: ExecuteLimits {
            compute_threads: 7,
            batch_targets: 2,
            cache_bytes: 2 * 1024 * 1024,
            max_output_bytes: limit,
            writer_index_memory_bytes: 16 * 1024 * 1024,
        },
    };
    let baseline = dir.path().join("baseline.mcap");
    execute(
        &sources,
        job(16 * 1024 * 1024),
        &baseline,
        &AtomicBool::new(false),
    )
    .unwrap();
    let bytes = std::fs::read(&baseline).unwrap();
    let summary = mcap::Summary::read(&bytes).unwrap().unwrap();
    assert!(
        summary.chunk_indexes.len() > 1,
        "fixture must force later chunk creation"
    );
    let first = &summary.chunk_indexes[0];
    let later = &summary.chunk_indexes[1];
    for (name, cap) in [
        ("initial", 100),
        ("mid", first.chunk_start_offset + first.chunk_length / 2),
        ("later", later.chunk_start_offset + 3),
    ] {
        let destination = dir.path().join(format!("{name}.mcap"));
        let error = execute(&sources, job(cap), &destination, &AtomicBool::new(false))
            .err()
            .expect("write budget must fail");
        assert_eq!(error.kind, ErrorKind::ResourceLimit, "{name}: {error}");
        assert!(!destination.exists());
        assert_eq!(
            std::fs::read_dir(sources.temporary_directory().unwrap())
                .unwrap()
                .count(),
            1,
            "{name}: only the immutable snapshot remains"
        );
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            3,
            "{name}: no sibling staged output remains"
        );
    }
    let recovery = dir.path().join("recovery.mcap");
    execute(
        &sources,
        job(16 * 1024 * 1024),
        &recovery,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(
        mcap::MessageStream::new(&std::fs::read(recovery).unwrap())
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .len(),
        16,
        "reusing source after failures resets typed write latch"
    );
}
