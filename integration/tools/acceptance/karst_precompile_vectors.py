#!/usr/bin/env python3
"""Original modern OP components; no current-chain VM or trusted-bank claim."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tomllib
from op_chain_cost_vectors import ROOT, SOURCE as GETH, digest, require, original_inputs as geth_original_inputs
from op_precompile_vectors import cases as old_cases

BASE = ROOT / 'integration/.runtime/design-review/op-karst-reference'
OP_COMMIT = '60360273aada79572fe2212497de578ad2fa249b'
REGISTRY_COMMIT = '0ca0a93ca0aebb05c90c4dae3f1c66927f8db0af'
OP = BASE / 'source' / ('optimism-' + OP_COMMIT)
REGISTRY = BASE / 'source' / ('superchain-registry-' + REGISTRY_COMMIT)
RUNNER = ROOT / 'integration/tools/acceptance/reference/op_karst_precompiles.rs'
ARCHIVES = {'optimism-' + OP_COMMIT: 'c4fa3360916881d233fc7f933f129452043a25bbee5227e4d8e3325a8f272120',
            'superchain-registry-' + REGISTRY_COMMIT: '172c5572379683ccc0f326e7d1a804923e7977f1d0e0f084a8461574d4a508b0'}


def originals():
    refs = {}
    # Verify whole immutable source archives before selecting the actual component
    # operands. No original module is patched, relabelled, or reconstructed.
    for prefix, expected in ARCHIVES.items():
        archive = BASE / (prefix + '.tar.gz')
        require(digest(archive) == expected, 'Original modern archive changed')
        refs[str(archive.relative_to(ROOT))] = expected
        with tarfile.open(archive) as tar:
            count = 0
            for member in tar:
                if not member.isfile(): continue
                parts = Path(member.name).parts
                require(parts[0] == prefix and '..' not in parts and not Path(member.name).is_absolute(), 'Unsafe modern archive member')
                local = BASE / 'source' / member.name
                require(digest(local) == hashlib.sha256(tar.extractfile(member).read()).hexdigest(), 'Original modern source changed: ' + member.name)
                count += 1
            require(count > 100, 'Original modern archive incomplete')
    for p in [OP / 'rust/op-revm/src/spec.rs', OP / 'rust/op-revm/src/precompiles.rs', OP / 'rust/op-revm/Cargo.toml', OP / 'rust/Cargo.toml', OP / 'rust/Cargo.lock', REGISTRY / 'superchain/configs/mainnet/op.toml', RUNNER]:
        refs[str(p.relative_to(ROOT))] = digest(p)
    chain = tomllib.loads((REGISTRY / 'superchain/configs/mainnet/op.toml').read_text())
    require(chain['chain_id'] == 10 and chain['hardforks']['karst_time'] == 1783526401 and chain['hardforks']['lagoon_time'] == 1799683201, 'Pinned current OP interval changed')
    return refs


def cases():
    old, refs = old_cases(include_jovian=True)
    rows = []
    for row in old:
        if row['fork'] != 'jovian': continue
        value = {k: row[k] for k in ['id', 'address', 'data', 'gas']}
        value['id'] = value['id'].replace('jovian-', 'karst-', 1)
        if value['id'].startswith('karst-cap-8-'):
            label = value['id'].split('8-', 1)[1]
            size = {'last-valid-width': 57600, 'cap': 57600, 'cap-plus-one': 57601, 'first-over-width': 57792}[label]
            value['data'] = '00' * size
        rows.append(value)
    def add(name, address, data, offered):
        for gas in offered:
            rows.append(dict(id=f'{name}-g{gas}', address=f'0x{address:040x}', data=data, gas=gas))
    corpus = GETH / 'core/vm/testdata/precompiles'
    for name in ['modexp', 'modexp_eip2565', 'modexp_eip7883']:
        path = corpus / (name + '.json'); refs[str(path.relative_to(ROOT))] = digest(path)
        for i, v in enumerate(json.loads(path.read_bytes())):
            add(f'{name}-{i}', 5, v['Input'], [0, 499, 500, 1_000_000_000])
    for count in range(97):
        add(f'padded-zero-header-{count}', 5, '00' * count, [499, 500, 1_000_000])
        if count:
            add(f'padded-nonzero-header-{count}', 5, '00'*(count-1)+'01', [499, 500, 1_000_000])
    for index in range(3):
        for length in [0, 1, 31, 32, 33, 64, 1024, 1025, 2**256-1]:
            lengths = [1, 1, 1]; lengths[index] = length
            header = ''.join(n.to_bytes(32, 'big').hex() for n in lengths)
            data = '03' * min(lengths[0], 1024) + 'ff' * min(lengths[1], 1024) + '07' * min(lengths[2], 1024)
            add(f'length-{index}-{length}', 5, header+data, [499, 500, 1_000_000_000])
    for exponent in [1, 31, 32, 33, 64, 1024]:
        for byte in ['00', '80', 'ff']:
            header = bytes(32).hex() + exponent.to_bytes(32, 'big').hex() + bytes(32).hex()
            add(f'zero-output-{exponent}-{byte}', 5, header + byte*exponent, [499, 500, 1_000_000_000])
    for length in [1, 8, 32, 33, 64, 128, 1024]:
        for exponent in [0, 1, 31, 32, 33, 64, 1024]:
            header = ''.join(n.to_bytes(32, 'big').hex() for n in [length, exponent, length])
            add(f'arithmetic-{length}-{exponent}', 5, header+'ab'*length+'cd'*exponent+'ef'*length, [500, 1_000_000_000])
    p256 = corpus / 'p256VerifyFjord.json'
    for i, v in enumerate(json.loads(p256.read_bytes())):
        add(f'p256-gas-boundary-{i}', 256, v['Input'], [0, 3449, 3450, 6899, 6900, 6901])
    for size in [57599, 57600, 57601, 57792, 81984]:
        add(f'bn-cap-order-{size}', 8, '00'*size, [0, 44999, 45000, 30_000_000])
    require(len(rows) == 9452 and len({r['id'] for r in rows}) == len(rows), 'Karst component coverage incomplete')
    return rows, refs


def audit(inputs, rows):
    expected, _ = cases()
    require(inputs == expected and len(rows) == len(inputs), 'Original Karst component coverage changed')
    for value, row in zip(inputs, rows):
        require(row['input'] == value, 'Original Karst input substituted')
        require(isinstance(row['success'], bool) and isinstance(row['out_of_gas'], bool), 'Original Karst result type changed')
        require(set(row) == {'input', 'success', 'out_of_gas', 'gas_used', 'output', 'status'}, 'Original Karst result fields changed')
        address = int(value['address'], 16)
        raw = bytes.fromhex(value['data'])
        expected_price = {8: 45000 + 34000 * (len(raw)//192), 10: 50000,
                          11: 375, 13: 600, 15: 37700 + 32600 * (len(raw)//384),
                          16: 5500, 17: 23800, 256: 6900}.get(address)
        if address == 5:
            header = raw[:96].ljust(96, b'\0')
            base, exponent, modulus = [int.from_bytes(header[i:i+32], 'big') for i in [0, 32, 64]]
            too_large = max(base, exponent, modulus) > 1024
            expected_oog = value['gas'] < 500
            if not too_large:
                high_count = min(32, exponent)
                high = int.from_bytes(raw[96+base:96+base+high_count].ljust(high_count, b'\0'), 'big')
                iterations = max(1, max(0, high.bit_length()-1) + (16*(exponent-32) if exponent > 32 else 0))
                size = max(base, modulus)
                expected_price = max(500, (16 if size <= 32 else 2*((size+7)//8)**2) * iterations)
                expected_oog |= expected_price > value['gas']
            require(row['out_of_gas'] == expected_oog and row['success'] == (not too_large and not expected_oog), 'Original Osaka size/gas order changed')
            if row['success']:
                data = raw[96:].ljust(base+exponent+modulus, b'\0')
                b = int.from_bytes(data[:base], 'big')
                e = int.from_bytes(data[base:base+exponent], 'big')
                m = int.from_bytes(data[base+exponent:base+exponent+modulus], 'big')
                expected_output = (pow(b, e, m) if m else 0).to_bytes(modulus, 'big').hex()
                require(row['output'] == expected_output, 'Original Osaka arithmetic output changed')
        elif address == 256:
            require(row['success'] == (value['gas'] >= 6900) and row['out_of_gas'] == (value['gas'] < 6900), 'Original P256 gas threshold changed')
        elif address == 8 and len(raw) > 57600:
            require(not row['success'] and not row['out_of_gas'], 'Original Karst BN cap must precede gas')
        if row['success'] and expected_price is not None:
            require(row['gas_used'] == expected_price, 'Original Karst component price changed')
        if row['success']:
            require(not row['out_of_gas'] and 0 <= row['gas_used'] <= value['gas'], 'Original Karst successful gas invalid')
            require(row['status'] == 'Success', 'Original Karst success status changed')
        else:
            require(row['out_of_gas'] == (row['status'] == 'Halt(OutOfGas)') and row['status'].startswith('Halt('), 'Original Karst halt changed')


def dependencies(module):
    env = dict(os.environ, PATH=str(Path.home()/'.cargo/bin')+os.pathsep+os.environ['PATH'])
    raw = subprocess.check_output(['cargo', '+1.99.0', 'metadata', '--locked', '--format-version', '1', '--manifest-path', str(module/'Cargo.toml')], cwd=ROOT, env=env)
    (module/'metadata.json').write_bytes(raw)
    refs = {str((module/'metadata.json').relative_to(ROOT)): digest(module/'metadata.json')}
    metadata = json.loads(raw)
    locked = {(p['name'], p['version']): p['checksum'] for p in tomllib.loads((module/'Cargo.lock').read_text())['package'] if 'checksum' in p}
    for package in metadata['packages']:
        if package['source'] is None: continue
        require(package['source'].startswith('registry+'), 'Unpinned modern dependency source')
        directory = Path(package['manifest_path']).parent
        archive = directory.parents[1].parent/'cache'/directory.parent.name/(package['name']+'-'+package['version']+'.crate')
        expected_archive = locked[(package['name'], package['version'])]
        require(digest(archive) == expected_archive, 'Original Cargo dependency archive changed')
        refs[str(archive)] = expected_archive
        expected_files = set()
        with tarfile.open(archive) as tar:
            for member in tar:
                if not member.isfile(): continue
                parts = Path(member.name).parts
                require(parts[0] == package['name']+'-'+package['version'] and '..' not in parts and not Path(member.name).is_absolute(), 'Unsafe Cargo archive')
                relative = Path(*parts[1:]); expected_files.add(relative)
                path = directory/relative
                expected = hashlib.sha256(tar.extractfile(member).read()).hexdigest()
                require(digest(path) == expected, 'Original Cargo dependency source modified: '+str(path))
                refs[str(path)] = expected
        actual_files = {p.relative_to(directory) for p in directory.rglob('*') if p.is_file()} - {Path('.cargo-ok'), Path('.cargo-checksum.json')}
        require(actual_files == expected_files, 'Extra or missing Cargo dependency operand: '+str(directory))
    sysroot = Path(subprocess.check_output(['rustc', '+1.99.0', '--print', 'sysroot'], env=env, text=True).strip())
    for p in [sysroot/'bin/rustc', sysroot/'bin/cargo', *sorted((sysroot/'lib').glob('*.dylib'))]: refs[str(p)] = digest(p)
    return refs


def write(output):
    output = Path(output).resolve(); output.parent.mkdir(parents=True, exist_ok=False)
    refs = originals(); refs.update(geth_original_inputs()); inputs, corpus = cases(); refs.update(corpus)
    module = output.parent/'runner'; (module/'src').mkdir(parents=True)
    for name in ['spec.rs', 'precompiles.rs']:
        shutil.copyfile(OP/'rust/op-revm/src'/name, module/'src'/name)
    shutil.copyfile(RUNNER, module/'src/main.rs')
    # Retain the original workspace resolution before Cargo selects the closure
    # for these exact two unmodified original components.
    shutil.copyfile(OP/'rust/Cargo.lock', module/'Cargo.lock')
    (module/'Cargo.toml').write_text('''[package]
name = "xone-karst-original-components"
version = "0.0.0"
edition = "2024"
[workspace]
[features]
serde = []
[dependencies]
revm = { version = "=43.0.3", default-features = false, features = ["std", "c-kzg", "blst", "secp256k1", "portable"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
hex = "=0.4.3"
''')
    env = dict(os.environ, PATH=str(Path.home()/'.cargo/bin')+os.pathsep+os.environ['PATH'], CARGO_BUILD_JOBS='2', CARGO_INCREMENTAL='0', CARGO_TARGET_DIR=str(BASE/'target'))
    build = subprocess.run(['cargo', '+1.99.0', 'build', '--manifest-path', str(module/'Cargo.toml')], cwd=ROOT, env=env, capture_output=True, text=True, timeout=600)
    (output.parent/'build.log').write_text(build.stdout+build.stderr)
    require(build.returncode == 0, 'Original modern component build failed: '+build.stderr[-3500:])
    refs.update(dependencies(module))
    binary = module/'runner'; shutil.copy2(BASE/'target/debug/xone-karst-original-components', binary)
    (output.parent/'inputs.json').write_text(json.dumps(inputs)+'\n')
    result = subprocess.run([str(binary)], input=json.dumps(inputs), capture_output=True, text=True, timeout=300)
    (output.parent/'original.stderr').write_text(result.stderr)
    require(result.returncode == 0, 'Original modern components failed: '+result.stderr[-2500:])
    rows = json.loads(result.stdout); audit(inputs, rows)
    for p in [*sorted((module/'src').glob('*.rs')), module/'Cargo.toml', module/'Cargo.lock', binary, output.parent/'build.log', output.parent/'inputs.json', output.parent/'original.stderr']:
        refs[str(p.relative_to(ROOT))] = digest(p)
    report = dict(passed=True, cases=rows, case_count=len(rows), references=refs, original_op_commit=OP_COMMIT, registry_commit=REGISTRY_COMMIT, production_ready=False, all_requested_features_complete=False, current_vm_admitted=False, scope='ORIGINAL_KARST_PRECOMPILE_COMPONENTS_NOT_FULL_CLIENT_TRANSITION_OR_CONSENSUS')
    output.write_text(json.dumps(report)+'\n')
    return refs


def replay(vector, output):
    vector = Path(vector).resolve(); v = json.loads(vector.read_bytes())
    for p, h in v['references'].items(): require(digest(ROOT/p) == h, 'Pinned modern operand changed: '+p)
    inputs, _ = cases()
    result = subprocess.run([str(vector.parent/'runner/runner')], input=json.dumps(inputs), capture_output=True, text=True, timeout=300)
    Path(output).with_suffix('.stderr').write_text(result.stderr)
    require(result.returncode == 0, 'Original modern replay failed: '+result.stderr[-2000:])
    rows = json.loads(result.stdout); require(rows == v['cases'], 'Original modern replay changed'); audit(inputs, rows)
    Path(output).write_text(json.dumps(dict(passed=True, cases=len(rows), vector_sha256=digest(vector), current_vm_admitted=False))+'\n')


if __name__ == '__main__':
    import sys
    if len(sys.argv) == 4 and sys.argv[1] == '--replay': replay(sys.argv[2], sys.argv[3])
    else: write(sys.argv[1])
