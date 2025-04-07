fn main() {
    foxglove_proto_build::compile(&["./protos/custom.proto"], &["./protos/"]).unwrap();
}
