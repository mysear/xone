#!/usr/bin/env python3
"""Check protocol, golden data and vendored source copies across services."""
from pathlib import Path
import json
import os

root = Path(__file__).resolve().parents[3]
services = ['chain-parser-go', 'chain-parser-rust', 'dex-router']
checked = []


def check_copy(source, target):
    if not target.is_file() or source.read_bytes() != target.read_bytes():
        raise SystemExit(f'Contract mismatch: {target.relative_to(root)}')
    checked.append(str(target.relative_to(root)))


# Each service carries only the test vectors it actually references.
import re
for service in services:
    base = root / service
    referenced = set()
    for folder in ['internal', 'cmd', 'crates', 'test']:
        for path in (base / folder).rglob('*'):
            if path.suffix in ['.go', '.rs']:
                foreign = re.search(r'(?:\.\./)+(?:chain-parser-go|chain-parser-rust|dex-router|integration)/', path.read_text())
                if foreign:
                    raise SystemExit(f'Cross-project source/test path: {path.relative_to(root)}: {foreign.group(0)}')
                referenced.update(re.findall(r'test/contracts/([a-zA-Z0-9_.-]+\.json)', path.read_text()))
    local = {path.name for path in (base / 'test/contracts').glob('*.json')}
    if referenced != local:
        raise SystemExit(f'Test vector set mismatch: {service}; missing={sorted(referenced-local)}, unused={sorted(local-referenced)}')
    for name in sorted(referenced):
        check_copy(root / 'integration/test/contracts' / name, base / 'test/contracts' / name)
    if (base / 'contracts').exists():
        raise SystemExit(f'Protocol copies must live in common/protoc: {service}')
contract_count = len(checked)
for chain, owners in [('evm', services), ('solana', ['chain-parser-rust'])]:
    for service in owners:
        check_copy(root / 'integration/test' / chain / 'baseline.json', root / service / 'test/fixtures' / chain / 'baseline.json')

check_copy(root / 'integration/test/solana/rayclmm-base.json', root / 'chain-parser-rust/test/fixtures/solana/rayclmm-base.json')
check_copy(root / 'integration/test/solana/rayclmm-bitmap-base.json', root / 'chain-parser-rust/test/fixtures/solana/rayclmm-bitmap-base.json')
check_copy(root / 'integration/test/solana/rayclmm-seed-base.json', root / 'chain-parser-rust/test/fixtures/solana/rayclmm-seed-base.json')
check_copy(root / 'integration/test/solana/rayclmm-dynamic-base.json', root / 'chain-parser-rust/test/fixtures/solana/rayclmm-dynamic-base.json')

# Pinned source and derived EVM harness stay local to the Router's standalone tests.
for name in ['Pool.sol', 'Reference.sol']:
    check_copy(root / 'integration/tools/reference/velodrome-v2' / name,
               root / 'dex-router/test/fixtures/velodrome-v2' / name)

check_copy(root/'integration/tools/acceptance/solana_prepare_simulation_worker.py',root/'dex-router/test/tools/solana_prepare_simulation_worker.py')
check_copy(root/'integration/tools/acceptance/solana_wallet_worker.py',root/'dex-router/test/tools/solana_wallet_worker.py')

# Shared Rust implementation copies remain synchronized; wire types come from common/protoc.
for crate in ['domain','config','observability','kafka-transport']:
    source=root/'chain-parser-rust/crates'/crate/'src'
    target=root/'dex-router/crates'/crate/'src'
    source_files={p.relative_to(source) for p in source.rglob('*.rs')}
    target_files={p.relative_to(target) for p in target.rglob('*.rs')}
    if source_files!=target_files:raise SystemExit(f'Vendored common crate file drift: {crate}')
    for file in source_files:
        if (source/file).read_bytes()!=(target/file).read_bytes():raise SystemExit(f'Vendored common crate drift: {crate}/{file}')

for service in ['chain-parser-rust', 'dex-router']:
    for directory, children, files in os.walk(root / service):
        children[:] = [name for name in children if name != 'target']
        if 'Cargo.toml' not in files:
            continue
        path = Path(directory) / 'Cargo.toml'
        text = path.read_text()
        for sibling in services:
            if f'../{sibling}' in text:
                raise SystemExit(f'Cross-project source dependency: {path}')

# Public source evidence is copied into Go's own test tree so its tests remain
# runnable without a sibling integration checkout. Match the complete declared
# set and every byte; never relax the cross-project dependency rule above.
source_root = root / 'integration/test/source'
local_source_root = root / 'chain-parser-go/test/fixtures/source/testdata'
manifest = json.loads((source_root / 'chain-receipt-vectors/manifest.json').read_text())
source_files = {
    Path('chain-receipt-vectors/manifest.json'),
    Path('deployment-catalog/v3-pool-template/two-real-pools.json'),
    Path('deployment-catalog/verified-source-inputs/ethereum-uniswap-v3.json'),
    Path('ethereum-mainnet/uniswap-v2-26355865-evidence.json'),
}
manifest_paths = [entry['path'] for entry in manifest['files']]
if len(manifest_paths) != len(set(manifest_paths)):
    raise SystemExit('Duplicate retained public source fixture')
for name in manifest_paths:
    relative = Path(name)
    if relative.is_absolute() or '..' in relative.parts or not relative.parts:
        raise SystemExit('Nonlocal retained public source fixture')
    source_files.add(Path('chain-receipt-vectors') / relative)
actual_sources = {p.relative_to(local_source_root) for p in local_source_root.rglob('*') if p.is_file()}
if actual_sources != source_files:
    raise SystemExit('Public source fixture membership mismatch')
for relative in sorted(source_files):
    check_copy(source_root / relative, local_source_root / relative)
source_fixture_count = len(source_files)

print(json.dumps({
    'passed': True,
    'contract_files': contract_count,
    'fixture_files': len(checked) - contract_count - source_fixture_count,
    'public_source_fixture_files': source_fixture_count,
    'services': len(services), 'protocol_source': 'common/protoc/proto',
}))
