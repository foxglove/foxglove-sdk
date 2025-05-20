use std::path::Path;

fn main() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let proto_path = workspace_root.join("schemas").join("proto");
    let out_path = workspace_root
        .join("rust")
        .join("foxglove")
        .join("src")
        .join("schemas");

    foxglove_proto_gen::generate_protos(&proto_path, &out_path).expect("Failed to generate protos");
}
