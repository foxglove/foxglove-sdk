use std::process::Command;

fn main() {
    for subcmd in ["typecheck", "build:prod"] {
        let status = Command::new("yarn")
            .arg(subcmd)
            .status()
            .expect("failed to run yarn");
        if !status.success() {
            panic!("yarn {subcmd} failed: {status}");
        }
    }
    println!("cargo:rerun-if-changed=ts/");
}
