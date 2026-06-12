use ::foxglove::Encode;
use prost::Message;

// Ensure the macro properly references the foxglove crate
mod foxglove {}

#[derive(Encode)]
struct Inner {
    v: u32,
}

#[derive(Debug, Clone, Copy, Encode)]
#[allow(dead_code)]
enum Color {
    Red = 0,
}

// The same message and enum types appear both bare and container-wrapped.
// Containers share their element's descriptors, so each nested definition must
// be emitted only once regardless of wrapping.
#[derive(Encode)]
struct Outer {
    bare: Inner,
    optional: Option<Inner>,
    repeated: Vec<Inner>,
    array: [Inner; 2],
    color: Color,
    colors: Vec<Color>,
}

#[test]
fn container_wrapped_definitions_are_deduplicated() {
    let schema = Outer::get_schema().expect("schema");
    let fds = prost_types::FileDescriptorSet::decode(schema.data.as_ref()).expect("decode schema");

    let outer = fds
        .file
        .iter()
        .flat_map(|f| f.message_type.iter())
        .find(|m| m.name() == "Outer")
        .expect("Outer descriptor");

    let nested: Vec<&str> = outer.nested_type.iter().map(|n| n.name()).collect();
    assert_eq!(nested, ["Inner"]);

    let enums: Vec<&str> = outer.enum_type.iter().map(|e| e.name()).collect();
    assert_eq!(enums, ["Color"]);
}
