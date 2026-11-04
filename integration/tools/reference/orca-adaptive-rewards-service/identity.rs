use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
pub type R<T> = Result<T, String>;
pub fn require(ok: bool, message: impl ToString) -> R<()> {
    if ok { Ok(()) } else { Err(message.to_string()) }
}
pub fn hash(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
fn file_hash(path: &Path) -> R<String> {
    Ok(hash(&fs::read(path).map_err(|e| format!("{}: {e}",path.display()))?))
}
pub fn verify(root: &Path, build: bool) -> R<Value> {
    let source = root.join("../orca-whirlpools");
    let metadata_path = source.join("source-metadata.json");
    require(file_hash(&metadata_path)? == "90871acbd985186f605c077da4ab6063a62ee0e530b4452228d2bb38126cd8cf", "pinned complete source manifest differs")?;
    let metadata: Value = serde_json::from_slice(&fs::read(&metadata_path).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    require(metadata["revision"] == "cc683ec92acb5e7606caa3d8e514d9383e9c7c11", "upstream revision differs")?;
    require(metadata["archive_sha256"] == "9ddee363a1aec82d4130dd94deb52c78c659dff1e071d0fa7d56a1cbd8e450fb", "archive identity differs")?;
    require(metadata["source_adaptations"] == json!([]), "source adaptations forbidden")?;
    let mut inventory = Vec::new();
    for item in metadata["files"].as_array().ok_or("missing inventory")? {
        let relative = item["path"].as_str().ok_or("missing source path")?;
        let path = source.join("upstream").join(relative);
        let sha = file_hash(&path)?;
        require(item["sha256"] == sha, format!("upstream changed: {relative}"))?;
        if build { println!("cargo:rerun-if-changed={}",path.display()); }
        inventory.push(json!({"path":relative,"sha256":sha}));
    }
    require(!inventory.is_empty(), "empty source inventory")?;
    let mut harness = Vec::new();
    for name in ["Cargo.toml","Cargo.lock","build.rs","identity.rs","src/main.rs","README.md",
        "../../ci/orca-adaptive-rewards-service-golden.py","../../ci/orca-adaptive-rewards-service-check.py",
        "../orca-whirlpools/src/lib.rs"] {
        let path = root.join(name);
        if build { println!("cargo:rerun-if-changed={}",path.display()); }
        harness.push(json!({"path":name,"sha256":file_hash(&path)?}));
    }
    if build { println!("cargo:rerun-if-changed={}",metadata_path.display()); }
    let critical: Vec<_> = inventory.iter().filter(|v| {
        let p = v["path"].as_str().unwrap_or("");
        ["programs/whirlpool/src/state/", "programs/whirlpool/src/manager/", "programs/whirlpool/src/math/", "programs/whirlpool/src/util/"].iter().any(|s|p.starts_with(s))
    }).cloned().collect();
    Ok(json!({"revision":metadata["revision"],"archive_sha256":metadata["archive_sha256"],
        "source_metadata_sha256":file_hash(&metadata_path)?,"source_files_verified":inventory.len(),
        "source_inventory_sha256":hash(&serde_json::to_vec(&inventory).map_err(|e|e.to_string())?),
        "critical_sources":critical,"harness_closure":harness,
        "execution":"unchanged-original-native-pack-read-swap-manager-and-post-update", "sbf":false,
        "compiler":option_env!("ORCA_REFERENCE_RUSTC").unwrap_or("build-time")}))
}
