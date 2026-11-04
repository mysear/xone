use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
fn hash(path: &Path) -> String {
    format!(
        "{:x}",
        Sha256::digest(fs::read(path).expect("reference closure file missing"))
    )
}
pub fn verify(root: &Path, build: bool) -> Value {
    let upstream = root.join("../orca-whirlpools");
    let metadata_path = upstream.join("source-metadata.json");
    let metadata: Value = serde_json::from_slice(&fs::read(&metadata_path).unwrap()).unwrap();
    assert_eq!(
        metadata["revision"], "cc683ec92acb5e7606caa3d8e514d9383e9c7c11",
        "wrong upstream revision"
    );
    assert_eq!(
        metadata["archive_sha256"],
        "9ddee363a1aec82d4130dd94deb52c78c659dff1e071d0fa7d56a1cbd8e450fb"
    );
    assert_eq!(metadata["source_adaptations"], json!([]));
    let mut inventory = Vec::new();
    for item in metadata["files"].as_array().unwrap() {
        let relative = item["path"].as_str().unwrap();
        let path = upstream.join("upstream").join(relative);
        let sha = hash(&path);
        assert_eq!(
            sha,
            item["sha256"].as_str().unwrap(),
            "upstream source changed: {relative}"
        );
        if build {
            println!("cargo:rerun-if-changed={}", path.display());
        }
        inventory.push(json!({"path": relative, "sha256":sha}));
    }
    let mut harness = Vec::new();
    for name in [
        "Cargo.toml",
        "Cargo.lock",
        "build.rs",
        "identity.rs",
        "src/main.rs",
    ] {
        let path = root.join(name);
        if build {
            println!("cargo:rerun-if-changed={}", path.display());
        }
        harness.push(json!({"path":name,"sha256":hash(&path)}));
    }
    if build {
        println!("cargo:rerun-if-changed={}", metadata_path.display());
    }
    let closure = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&inventory).unwrap())
    );
    let critical: Vec<_> = inventory
        .iter()
        .filter(|x| {
            let path = x["path"].as_str().unwrap();
            path.starts_with("programs/whirlpool/src/manager/")
                || path.starts_with("programs/whirlpool/src/util/")
                || path == "programs/whirlpool/src/state/oracle.rs"
                || path.starts_with("programs/whirlpool/src/math/")
        })
        .cloned()
        .collect();
    json!({"revision":metadata["revision"], "archive_sha256":metadata["archive_sha256"],
        "source_metadata_sha256":hash(&metadata_path),"source_files_verified":inventory.len(),
        "source_inventory_sha256":closure,"critical_sources":critical,"harness_closure":harness,
        "execution":"unchanged-original-native-swap-manager-and-post-update", "sbf":false,
        "compiler":option_env!("ORCA_REFERENCE_RUSTC").unwrap_or("build-time")})
}
