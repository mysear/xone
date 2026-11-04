#[path = "identity.rs"]
mod identity;
fn run() -> identity::R<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let rustc = std::process::Command::new(std::env::var("RUSTC").map_err(|e|e.to_string())?).arg("-vV").output().map_err(|e|e.to_string())?;
    identity::require(rustc.status.success(), "rustc identity failed")?;
    let compiler = String::from_utf8(rustc.stdout).map_err(|e|e.to_string())?;
    identity::require(compiler.starts_with("rustc 1.90.0 "), "locked compiler must be Rust 1.90.0")?;
    let mut closure = identity::verify(root, true)?;
    closure["compiler"] = serde_json::json!(compiler.replace('\n'," | "));
    println!("cargo:rustc-env=ORCA_REFERENCE_RUSTC={}", compiler.replace('\n'," | "));
    println!("cargo:rustc-env=ORCA_REFERENCE_CLOSURE={closure}");
    Ok(())
}
fn main() { if let Err(e) = run() { eprintln!("native service build refused: {e}"); std::process::exit(1); } }
