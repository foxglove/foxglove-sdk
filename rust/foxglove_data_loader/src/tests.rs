use std::borrow::Cow;

use foxglove::Encode;

use super::*;

#[test]
fn test_created_init_with_no_schemas() {
    let mut builder = InitializationBuilder::default();

    builder
        .add_channel("/first")
        .message_encoding("json")
        .message_count(10);

    builder.add_channel("/second").message_encoding("json");

    let init = loader::Initialization::from(builder.build());

    assert_eq!(init.channels.len(), 2);
    assert_eq!(init.schemas.len(), 0);

    assert_eq!(init.channels[0].message_encoding, "json");
    assert_eq!(init.channels[1].message_encoding, "json");

    assert_eq!(init.channels[0].message_count, Some(10));
    assert_eq!(init.channels[1].message_count, None);
}

#[test]
fn test_add_channel_with_existing_id() {
    let mut builder = InitializationBuilder::default();

    builder
        .add_channel_with_id(1, "/first")
        .expect("first channel should get added");

    let second_channel = builder.add_channel_with_id(1, "/second");

    assert!(second_channel.is_none());
}

#[test]
fn test_add_auto_channel_skips_taken_ids() {
    let mut builder = InitializationBuilder::default();

    builder.add_channel_with_id(1, "/manual").unwrap();
    builder.add_channel_with_id(2, "/manual").unwrap();
    builder.add_channel_with_id(3, "/manual").unwrap();
    builder.add_channel_with_id(4, "/manual").unwrap();
    builder.add_channel_with_id(5, "/manual").unwrap();

    builder.add_channel("/auto");

    let init = loader::Initialization::from(builder.build());

    assert_eq!(init.channels.len(), 6);
    assert_eq!(init.channels[5].id, 6);
}

#[test]
fn test_add_channel_from_encode() {
    #[derive(Encode)]
    struct MyData {
        name: String,
    }

    let mut builder = InitializationBuilder::default();

    builder
        .add_encode::<MyData>()
        .expect("failed to add encode")
        .add_channel("/my-data");

    let init = loader::Initialization::from(builder.build());

    assert_eq!(init.channels.len(), 1);
    assert_eq!(init.schemas.len(), 1);

    assert_eq!(init.channels[0].topic_name, "/my-data");
    assert_eq!(init.channels[0].message_encoding, "protobuf");

    assert_eq!(init.schemas[0].encoding, "protobuf");
}

#[test]
fn test_add_channel_from_schema() {
    let mut builder = InitializationBuilder::default();

    let schema = foxglove::Schema {
        name: "test.MySchema".into(),
        encoding: "jsonschema".into(),
        data: Cow::Borrowed(&[]),
    };

    builder
        .add_schema(schema)
        .message_encoding("json")
        .add_channel("/my-data");

    let init = loader::Initialization::from(builder.build());

    assert_eq!(init.channels.len(), 1);
    assert_eq!(init.schemas.len(), 1);

    assert_eq!(init.channels[0].topic_name, "/my-data");
    assert_eq!(init.channels[0].message_encoding, "json");

    assert_eq!(init.schemas[0].encoding, "jsonschema");
}

#[test]
fn test_add_schema_wtih_existing_id() {
    let mut builder = InitializationBuilder::default();

    builder
        .add_schema_with_id(
            1,
            foxglove::Schema {
                name: "test.FirstSchema".into(),
                encoding: "jsonschema".into(),
                data: Cow::Borrowed(&[]),
            },
        )
        .expect("first schema should get added");

    let second_schema = builder.add_schema_with_id(
        1,
        foxglove::Schema {
            name: "test.SecondSchema".into(),
            encoding: "jsonschema".into(),
            data: Cow::Borrowed(&[]),
        },
    );

    assert!(second_schema.is_none());
}

#[test]
fn test_add_encode_wtih_existing_id() {
    #[derive(Encode)]
    struct MyData {
        name: String,
    }

    let mut builder = InitializationBuilder::default();

    builder
        .add_encode_with_id::<MyData>(1)
        .expect("schema should encode")
        .expect("first schema should get added");

    let second_schema = builder
        .add_encode_with_id::<MyData>(1)
        .expect("schema should encode");

    assert!(second_schema.is_none());
}

#[test]
fn test_add_auto_schema_skips_taken_ids() {
    let mut builder = InitializationBuilder::default();

    builder.add_channel_with_id(1, "/manual").unwrap();
    builder.add_channel_with_id(2, "/manual").unwrap();
    builder.add_channel_with_id(3, "/manual").unwrap();
    builder.add_channel_with_id(4, "/manual").unwrap();
    builder.add_channel_with_id(5, "/manual").unwrap();

    builder.add_channel("/auto");

    let init = loader::Initialization::from(builder.build());

    assert_eq!(init.channels.len(), 6);
    assert_eq!(init.channels[5].id, 6);
}

#[test]
fn test_multiple_channels_same_topic() {
    let mut init = Initialization::builder();

    init.add_channel("/json");
    init.add_channel("/json");
    init.add_channel("/json");
    init.add_channel("/json");
    init.add_channel("/json");

    let init = init.build();

    assert_eq!(init.channels.len(), 5);
    assert_eq!(init.channels[0].topic_name, "/json");
    assert_eq!(init.channels[1].topic_name, "/json");
    assert_eq!(init.channels[2].topic_name, "/json");
    assert_eq!(init.channels[3].topic_name, "/json");
    assert_eq!(init.channels[4].topic_name, "/json");
}

#[test]
fn test_set_time_range() {
    let init = loader::Initialization::from(
        Initialization::builder()
            .time_range(TimeRange {
                start_time: 20,
                end_time: 30,
            })
            .build(),
    );

    assert_eq!(init.time_range.start_time, 20);
    assert_eq!(init.time_range.end_time, 30);

    let init = loader::Initialization::from(
        Initialization::builder()
            .start_time(50)
            .end_time(60)
            .build(),
    );

    assert_eq!(init.time_range.start_time, 50);
    assert_eq!(init.time_range.end_time, 60);
}
