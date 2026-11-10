# Fixed CLMM native Linux build contract

The source archive is the fixed Raydium CLMM commit `6d7a0f82d2f293c553c69aaf943377367c6c4cc6`. Preserve its complete Cargo.lock (SHA-256 `9370048a70726a60e281003ff798d425ce81b84ce42343d3ffdeb91def2a7d8a`) and all source files. `clmm-source-verification.json` binds the cached tree to the original archive. No application source changes, dependency lock regeneration, or extra Cargo features are part of this recipe.

The toolchain is the official Linux x86_64 platform-tools v1.52 archive and the Linux Agave 3.1.10 cargo-build-sbf executable. Their public release metadata and digest verification receipts are retained here. Registry and the fixed parity-common git dependency come from the existing offline vendor snapshot. The SDK scripts are copied unchanged; Linux Criterion v2.3.3 supplies the SDK's platform-specific installation dependency. Criterion's old GitHub asset has no independently published digest in the retained release metadata; its downloaded SHA-256 is an observed-byte pin. It is not a CLMM Rust dependency.

## VM and inputs

Use vfkit 0.6.4 with the previously digest-verified Ubuntu 24.04 QCOW2 parent. The converter uses unchanged go-qcow2reader v0.8.0 from Lima's digest-verified vendor archive. `disk-conversion.json` compares SHA-256 over every original virtual disk byte before extending the sparse raw image to 16 GiB. Conversion and Go caches stay under the task directory.

The resource contract is 2 vCPU, 4096 MiB RAM and a 16 GiB writable guest disk. The input ISO is attached as an explicitly read-only block device. Source, vendor, platform tools and SDK are also bind-mounted read-only inside the guest; failed write probes are recorded. The only host directory share is the task-owned writable output directory. Native Apple NAT is used for signed Ubuntu prerequisite packages; Cargo compilation uses `--offline --locked`. No host home, wallet, SSH directory, Docker context or unrelated VM is selected.

`launch-ubuntu.py` records the full vfkit argument vector and puts temporary cloud-init media in the task-owned temporary directory. Cloud-init enters a normal guest root login through `runuser`; this supplies the account's HOME to cargo-build-sbf without importing any host account environment. `guest-bootstrap.sh` prepares the input mounts and fixed prerequisites. `run-build.py` records Linux/glibc/tool versions, tool hashes, installed package versions, mounts and compiler commands.

## Build and replay

The build command is retained verbatim in `output/clmm-linux-native-build.json`. It uses the fixed manifest, official SDK, no tools installation, no rustup override, two jobs, no extra features, verbose command evidence, offline vendor and the locked dependency graph. `clmm-actual-codegen-flags.json` extracts the actual root-crate rustc command, target and cfg from the raw build log. The SDK's standard postprocessing is part of the unmodified upstream pipeline. Retain both its complete output and the unstripped Cargo ELF. No additional ELF normalization, stripping, header removal, metadata exclusion or trimming is permitted.

For another build on this task-owned disk, run from the repository root:

```sh
python3 integration/artifacts/local/r01-sbf-linux/build-stage/replay-ubuntu-build.py
```

This archives the preceding task output, changes the task cloud-init instance identity and clears only the guest `/build-target` before reexecuting the fixed input recipe. It refuses to launch while the recorded VM PID is present. Source/vendor/tool inputs remain unchanged. A cold replay instead starts from the verified QCOW2 parent, creates a new raw disk with the recorded converter command, and uses the recorded cloud-init and launch arguments. Do not exceed the disk budget with additional simultaneously attached disks.

The guest exit trap writes its exit receipt, syncs output and requests normal guest poweroff. `request-owned-shutdown.py` can request normal ACPI Stop only after checking the task vfkit executable and task-local REST endpoint. It never selects another VM or requests HardStop. `watch-owned-vm.py` bounds preparation to 900 seconds and the one build phase to 1860 seconds; cargo itself has an 1800-second limit.

## Byte and deployment verification

After shutdown:

```sh
python3 integration/artifacts/local/r01-sbf-linux/build-stage/compare-native-clmm.py
python3 -OO integration/artifacts/local/r01-sbf-linux/build-stage/verify-measured-artifact.py
```

The comparator binds Program address, canonical ProgramData PDA, loader accounts, exact upgrade slot, authority and full payload hash to the preserved public observation. It verifies the unchanged complete source tree and lock, then requires every byte of the complete SDK ELF to equal the ProgramData ELF prefix and every subsequent allocation byte to be zero. Header, code, relocation and nonzero-tail counterexamples are rejected. No current-mainnet freshness or original historical Bank ancestry follows from this build contract.
