use std::path::PathBuf;
fn main() {
 let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
 let out = std::env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| root.join("rust/dexpb"));
 std::fs::create_dir_all(&out).expect("create output directory");
 prost_build::Config::new().out_dir(out).compile_protos(&[root.join("proto/dex/v1/state.proto")], &[root.join("proto")]).expect("generate shared Rust types");
}
