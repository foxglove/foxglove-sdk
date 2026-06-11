use ::foxglove::Encode;
use prost::Message;

// Ensure the macro properly references the foxglove crate
mod foxglove {}

#[derive(Encode)]
struct Zulu {
    v: u32,
}

#[derive(Encode)]
struct Alpha {
    v: u32,
}

#[derive(Encode)]
struct Mike {
    v: u32,
}

#[derive(Debug, Clone, Copy, Encode)]
#[allow(dead_code)]
enum Yankee {
    A = 0,
}

#[derive(Debug, Clone, Copy, Encode)]
#[allow(dead_code)]
enum Bravo {
    A = 0,
}

// Fields are declared in a deliberately non-alphabetical order. The generated
// schema must emit the nested message and enum definitions sorted by type name,
// regardless of declaration order, so that the encoded schema bytes are stable
// across builds.
#[derive(Encode)]
struct Container {
    zulu: Zulu,
    yankee: Yankee,
    alpha: Alpha,
    bravo: Bravo,
    mike: Mike,
}

#[test]
fn nested_definitions_are_deterministically_ordered() {
    let schema = Container::get_schema().expect("schema");
    let fds = prost_types::FileDescriptorSet::decode(schema.data.as_ref()).expect("decode schema");

    let container = fds
        .file
        .iter()
        .flat_map(|f| f.message_type.iter())
        .find(|m| m.name() == "Container")
        .expect("Container descriptor");

    let nested: Vec<&str> = container.nested_type.iter().map(|n| n.name()).collect();
    assert_eq!(nested, ["Alpha", "Mike", "Zulu"]);

    let enums: Vec<&str> = container.enum_type.iter().map(|e| e.name()).collect();
    assert_eq!(enums, ["Bravo", "Yankee"]);
}
