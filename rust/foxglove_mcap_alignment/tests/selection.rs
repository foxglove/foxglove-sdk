use foxglove_mcap_alignment::*;
use std::cell::Cell;
use std::rc::Rc;

fn file() -> FileIdentity {
    FileIdentity {
        sha256: [0; 32],
        byte_length: 10000,
    }
}
fn record(time: u64, offset: u64) -> SourceRecord {
    SourceRecord {
        identity: SourceIdentity {
            file: 0,
            channel: 7,
            locator: Locator::Unchunked { offset },
        },
        log_time: time,
        publish_time: 123,
        sequence: 99,
    }
}
fn source(records: &[SourceRecord]) -> Box<dyn MetadataSource + '_> {
    Box::new(SliceSource::new(Records::new(records, &[file()]).unwrap()))
}
fn fixed(origin: u64, numerator: u64, denominator: u64) -> Timeline<'static> {
    Timeline::Fixed {
        rate: Rate::new(numerator, denominator).unwrap(),
        origin_ns: origin,
    }
}

#[test]
fn validates_configuration_and_source_metadata() {
    assert_eq!(
        Episode::new(1, 1).unwrap_err().kind,
        ErrorKind::InvalidConfiguration
    );
    assert_eq!(
        Episode::new(2, 1).unwrap_err().kind,
        ErrorKind::InvalidConfiguration
    );
    for (n, d) in [(0, 1), (1, 0), (1_000_000_001, 1), (u64::MAX, 1)] {
        assert_eq!(
            Rate::new(n, d).unwrap_err().kind,
            ErrorKind::InvalidConfiguration
        );
    }
    assert_eq!(
        (
            Rate::new(60, 2).unwrap().numerator(),
            Rate::new(60, 2).unwrap().denominator()
        ),
        (30, 1)
    );
    for rows in [
        vec![record(2, 1), record(1, 2)],
        vec![record(1, 1), record(2, 1)],
        vec![record(1, 10000)],
    ] {
        assert_eq!(
            Records::new(&rows, &[file()]).unwrap_err().kind,
            ErrorKind::InvalidSource
        );
    }
    assert_eq!(
        Records::new(&[], &[file(), file()]).unwrap_err().kind,
        ErrorKind::InvalidConfiguration
    );
    let mut unknown = record(1, 1);
    unknown.identity.file = 1;
    assert_eq!(
        Records::new(&[unknown], &[file()]).unwrap_err().kind,
        ErrorKind::InvalidSource
    );
    let episode = Episode::new(0, 10).unwrap();
    assert!(matches!(
        Selector::new(episode, fixed(0, 1, 1), vec![], 0),
        Err(Error {
            kind: ErrorKind::InvalidConfiguration,
            ..
        })
    ));
    let stream = Stream {
        source: source(&[]),
        policy: Policy::Exact,
        tolerance_ns: 1,
        required: false,
    };
    assert!(matches!(
        Selector::new(episode, fixed(0, 1, 1), vec![stream], 1),
        Err(Error {
            kind: ErrorKind::InvalidConfiguration,
            ..
        })
    ));
    assert!(matches!(
        Targets::new(Episode::new(1, 2).unwrap(), fixed(0, 1, 1)),
        Err(Error {
            kind: ErrorKind::EmptyTimeline,
            ..
        })
    ));
    assert!(matches!(
        Targets::new(episode, Timeline::Reference(source(&[]))),
        Err(Error {
            kind: ErrorKind::EmptyTimeline,
            ..
        })
    ));
}

#[test]
fn duplicate_reference_validation_precedes_required_misses_and_clips_boundaries() {
    let reference = [
        record(0, 0),
        record(0, 1),
        record(5, 2),
        record(5, 3),
        record(9, 4),
        record(9, 5),
    ];
    let episode = Episode::new(1, 9).unwrap();
    let stream = Stream {
        source: source(&[]),
        policy: Policy::Exact,
        tolerance_ns: 0,
        required: true,
    };
    assert!(matches!(
        Selector::new(
            episode,
            Timeline::Reference(source(&reference)),
            vec![stream],
            1
        ),
        Err(Error {
            kind: ErrorKind::DuplicateReference,
            ..
        })
    ));
    let clipped = [
        record(0, 0),
        record(0, 1),
        record(5, 2),
        record(9, 4),
        record(9, 5),
    ];
    let mut targets = Targets::new(episode, Timeline::Reference(source(&clipped))).unwrap();
    assert_eq!(targets.next_time().unwrap(), Some(5));
    assert_eq!(targets.next_time().unwrap(), None);
}

#[test]
fn bounded_batches_preserve_source_fields_reuse_and_optional_slots() {
    let records = [record(0, 0), record(5, 1)];
    let streams = vec![
        Stream {
            source: source(&records),
            policy: Policy::Previous,
            tolerance_ns: 4,
            required: true,
        },
        Stream {
            source: source(&[]),
            policy: Policy::Nearest,
            tolerance_ns: u64::MAX,
            required: false,
        },
    ];
    let mut selector = Selector::new(
        Episode::new(0, 10).unwrap(),
        fixed(0, 1_000_000_000, 1),
        streams,
        3,
    )
    .unwrap();
    let mut index = 0;
    while let Some(batch) = selector.next_batch().unwrap() {
        assert!(batch.len() <= 3);
        for row in batch {
            assert_eq!(row.index, index);
            assert_eq!(row.time, index);
            let selection = row.slots[0];
            assert_eq!(selection.source.unwrap().publish_time, 123);
            assert_eq!(selection.source.unwrap().sequence, 99);
            assert_eq!(selection.reused, index != 0 && index != 5);
            assert_eq!(selection.offset_ns, Some(-i128::from(index % 5)));
            assert_eq!(
                row.slots[1],
                Selection {
                    source: None,
                    offset_ns: None,
                    valid: false,
                    reused: false
                }
            );
            index += 1;
        }
    }
    assert_eq!(index, 10);
    assert_eq!(selector.records_advanced().collect::<Vec<_>>(), vec![2, 0]);
}

#[test]
fn nearest_ties_duplicate_locator_order_and_extreme_signed_offsets() {
    let mut records = vec![record(4, 0), record(4, 0), record(4, 1), record(6, 8)];
    records[0].identity.locator = Locator::Chunked {
        chunk_offset: 10,
        message_offset: 5,
    };
    records[1].identity.locator = Locator::Unchunked { offset: 20 };
    records[2].identity.file = 1;
    let files = [
        file(),
        FileIdentity {
            sha256: [1; 32],
            ..file()
        },
    ];
    let view = Records::new(&records, &files).unwrap();
    let stream = Stream {
        source: Box::new(SliceSource::new(view)),
        policy: Policy::Nearest,
        tolerance_ns: 1,
        required: true,
    };
    let mut selector =
        Selector::new(Episode::new(5, 6).unwrap(), fixed(5, 1, 1), vec![stream], 1).unwrap();
    // All sources at 4/6 are out of episode even though within tolerance.
    assert_eq!(
        selector.next_batch().unwrap_err().kind,
        ErrorKind::MissingRequired
    );
    assert!(selector.next_batch().unwrap().is_none());
    let stream = Stream {
        source: Box::new(SliceSource::new(view)),
        policy: Policy::Nearest,
        tolerance_ns: 1,
        required: false,
    };
    let mut selector = Selector::new(
        Episode::new(0, 10).unwrap(),
        fixed(5, 100_000_000, 1),
        vec![stream],
        1,
    )
    .unwrap();
    assert_eq!(
        selector.next_batch().unwrap().unwrap()[0].slots[0].source,
        Some(records[0])
    );
    let extreme = [record(u64::MAX - 1, 0)];
    let stream = Stream {
        source: source(&extreme),
        policy: Policy::Nearest,
        tolerance_ns: u64::MAX,
        required: true,
    };
    let mut selector = Selector::new(
        Episode::new(0, u64::MAX).unwrap(),
        fixed(0, 1, u64::MAX),
        vec![stream],
        1,
    )
    .unwrap();
    assert_eq!(
        selector.next_batch().unwrap().unwrap()[0].slots[0].offset_ns,
        Some(i128::from(u64::MAX - 1))
    );
    assert!(selector.next_batch().unwrap().is_none());
    let early = [record(0, 0)];
    let stream = Stream {
        source: source(&early),
        policy: Policy::Previous,
        tolerance_ns: u64::MAX,
        required: true,
    };
    let mut selector = Selector::new(
        Episode::new(0, u64::MAX).unwrap(),
        fixed(u64::MAX - 1, 1, u64::MAX),
        vec![stream],
        1,
    )
    .unwrap();
    assert_eq!(
        selector.next_batch().unwrap().unwrap()[0].slots[0].offset_ns,
        Some(-i128::from(u64::MAX - 1))
    );
}

struct GeneratedSource {
    next: u64,
    count: u64,
    reads: Rc<Cell<u64>>,
    bad_order: bool,
    fail_at: Option<u64>,
}
impl MetadataSource for GeneratedSource {
    fn next_record(&mut self) -> Result<Option<SourceRecord>, Error> {
        if self.fail_at == Some(self.next) {
            return Err(Error {
                kind: ErrorKind::InvalidSource,
                message: "injected metadata read failure",
                detail: None,
                target: None,
                stream: None,
            });
        }
        if self.next == self.count {
            return Ok(None);
        }
        let index = self.next;
        self.next += 1;
        self.reads.set(self.reads.get() + 1);
        Ok(Some(record(
            if self.bad_order {
                self.count - index
            } else {
                index
            },
            index,
        )))
    }
    fn rewind(&mut self) -> Result<(), Error> {
        self.next = 0;
        Ok(())
    }
}
fn generated(count: u64, reads: Rc<Cell<u64>>) -> GeneratedSource {
    GeneratedSource {
        next: 0,
        count,
        reads,
        bad_order: false,
        fail_at: None,
    }
}

#[test]
fn fallible_stream_rejects_disorder_and_propagates_read_errors() {
    for bad_order in [false, true] {
        let mut input = generated(5, Rc::new(Cell::new(0)));
        input.bad_order = bad_order;
        if !bad_order {
            input.fail_at = Some(2);
        }
        let stream = Stream {
            source: Box::new(input),
            policy: Policy::Previous,
            tolerance_ns: 10,
            required: false,
        };
        let mut selector = Selector::new(
            Episode::new(0, 10).unwrap(),
            fixed(0, 1_000_000_000, 1),
            vec![stream],
            10,
        )
        .unwrap();
        assert_eq!(
            selector.next_batch().unwrap_err().kind,
            ErrorKind::InvalidSource
        );
        assert!(selector.next_batch().unwrap().is_none());
    }
}

#[test]
fn lazy_metadata_and_selection_traverse_linearly_without_dataset_sized_results() {
    // A generated million-record source cannot hide a resident input or result matrix.
    let count = 1_000_000;
    let reads = Rc::new(Cell::new(0));
    let stream = Stream {
        source: Box::new(generated(count, Rc::clone(&reads))),
        policy: Policy::Previous,
        tolerance_ns: 0,
        required: true,
    };
    let mut selector = Selector::new(
        Episode::new(0, count).unwrap(),
        fixed(0, 1_000_000_000, 1),
        vec![stream],
        127,
    )
    .unwrap();
    assert_eq!(reads.get(), 1);
    assert_eq!(selector.next_batch().unwrap().unwrap().len(), 127);
    assert_eq!(reads.get(), 127);
    let mut selected = 127;
    while let Some(batch) = selector.next_batch().unwrap() {
        assert!(batch.len() <= 127);
        selected += batch.len();
    }
    assert_eq!(selected, count as usize);
    assert_eq!(reads.get(), count);
    assert_eq!(
        selector.records_advanced().collect::<Vec<_>>(),
        vec![count as usize - 1]
    );
}
