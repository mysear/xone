#[path = "identity.rs"]
mod identity;
fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    // The compiler/dependency lock and every original source byte form the closure.
    let rustc = std::process::Command::new(std::env::var("RUSTC").unwrap())
        .arg("-vV")
        .output()
        .unwrap();
    assert!(rustc.status.success());
    let compiler = String::from_utf8(rustc.stdout).unwrap();
    assert!(
        compiler.starts_with("rustc 1.90.0 "),
        "reference requires locked Rust 1.90.0"
    );
    let mut result = identity::verify(root, true);
    result["compiler"] = serde_json::json!(compiler);
    println!(
        "cargo:rustc-env=ORCA_REFERENCE_RUSTC={}",
        compiler.replace('\n', " | ")
    );
    println!("cargo:rustc-env=ORCA_REFERENCE_CLOSURE={}", result);
}
