#!/usr/bin/env python3
"""Assemble the checked-in finite catalog from retained public captures (no network).

Re-running updates only generated catalog/scope files inside deployment-catalog.
A scope binds actual observed runtime versions, addresses and H/hash, not a config
hash masquerading as a deployment. Public compiler reports remain separate.
"""
import argparse
import json
from pathlib import Path
import re
import sys
sys.path.insert(0,str(Path(__file__).resolve().parent))
from r01_public_catalog import ROOT, load_json, sha256, require, validate_catalog
from r01_public_catalog_capture import reference
BASE=ROOT/'integration/test/source/deployment-catalog'
TARGETS=ROOT/'integration/config/chain-targets.json'
TOOL_PATTERNS=(
    'integration/tools/acceptance/r01_public_catalog*.py',
    'integration/tools/acceptance/r01_public_catalog*.cjs',
    'integration/tools/acceptance/tests/test_r01_public_catalog*.py',
)

def write(name,data):
    (BASE/name).write_text(json.dumps(data,indent=2)+'\n')

def scope_rows(rows):
    fields = ('chain', 'chain_key', 'genesis_hash', 'protocol', 'version', 'address')
    return [{key: row[key] for key in fields} | {
        'anchor': {key: row['anchor'][key] for key in ('height', 'hash')},
    } for row in rows]

def inventory(directory):
    manifest = directory / 'manifest.json'
    files = [reference(path) for path in sorted(directory.rglob('*'))
             if path.is_file() and path != manifest]
    return {
        'schema_version': 1,
        'data_directory': str(directory.relative_to(ROOT)),
        'authority': 'complete byte inventory; no consensus or deployment qualification inferred',
        'files': files,
        'tools': [reference(path) for pattern in TOOL_PATTERNS for path in sorted(ROOT.glob(pattern))],
        'file_count': len(files),
        'total_bytes': sum((ROOT / ref['path']).stat().st_size for ref in files),
    }

def build_observations(directory):
    directory = directory.resolve()
    require(directory != BASE and directory.is_relative_to(BASE), 'OBSERVATION_DIRECTORY')
    rows = load_json(directory / 'observations.json')['deployments']
    seeds = [row for name in ('r01-evm-history-deployments.json', 'r01-solana-deployments.json')
             for row in load_json(TARGETS.parent / name)['deployments']]
    identity_fields = ('id', 'chain', 'protocol', 'address')
    expected = {tuple(row[key] for key in identity_fields) for row in seeds}
    observed = {tuple(row[key] for key in identity_fields) for row in rows}
    require(len(rows) == len(seeds) and observed == expected, 'OBSERVATION_DEPLOYMENT_SCOPE')

    go = ROOT / 'chain-parser-go/internal/domain/protocols.go'
    rust = ROOT / 'dex-router/crates/domain/src/protocols.rs'
    go_profiles = go.read_text() + go.with_name('lfj.go').read_text()
    rust_profiles = rust.read_text() + rust.with_name('lfj.rs').read_text()
    profile_literal = r'"([a-z][a-z0-9-]+)"'
    common_profiles = set(re.findall(profile_literal, go_profiles)) & set(re.findall(profile_literal, rust_profiles))
    for row in rows:
        runtime = row['runtime']['sha256']
        if row['chain'] == 'solana':
            row['version'] = f"sbf-sha256:{runtime}@upgrade:{row['upgrade']['slot']}"
        else:
            row['version'] = f'runtime-sha256:{runtime}'
        protocol = 'lfj-lb-v21' if row['protocol'] == 'lfj-liquidity-book' else row['protocol']
        row['profiles'] = sorted(profile for profile in common_profiles
                                 if profile == protocol or profile.startswith(protocol + '-'))

    catalog = {'schema_version': 1, 'catalog_kind': 'finite-read-only-public-deployment-evidence',
               'deployments': rows}
    scope = {'schema_version': 1, 'id': 'r01-eight-chains-public-observations',
             'chain_targets_sha256': reference(TARGETS)['sha256'], 'required': scope_rows(rows)}
    report = validate_catalog(ROOT, catalog, scope, load_json(TARGETS))
    require(report['structurally_valid'], 'OBSERVATION_EVIDENCE_INVALID:' + str(report['errors']))
    for name, data in (('catalog.json', catalog), ('scope.json', scope)):
        (directory / name).write_text(json.dumps(data, indent=2) + '\n')
    (directory / 'manifest.json').write_text(json.dumps(inventory(directory), indent=2) + '\n')

def build():
    catalog=json.loads((BASE/'samples/index.json').read_text());retry=json.loads((BASE/'samples-retry/index.json').read_text())
    replacements={r['id']:r for r in retry['deployments']}
    for i,row in enumerate(catalog['deployments']):
        if row['id'] in replacements:catalog['deployments'][i]=replacements[row['id']]
    go=ROOT/'chain-parser-go/internal/domain/protocols.go';rust=ROOT/'dex-router/crates/domain/src/protocols.rs'
    domain=[go.read_text(),rust.read_text()]
    for row in catalog['deployments']:
        row['release_label_from_directory']=row['version']
        row['version']=('sbf-sha256:'+row['runtime']['sha256']+'@upgrade:'+str(row['upgrade']['slot'])) if row['chain']=='solana' else 'runtime-sha256:'+row['runtime']['sha256']
        row['profiles_requested']=row['profiles']
        row['profiles']=[p for p in row['profiles'] if all('"'+p+'"' in text for text in domain)]
        row['domain_profile_support']={'go_and_rust_literal_profiles':row['profiles'],'go_source':reference(go),'rust_source':reference(rust),'deployment_source_match_established':False}
        row.pop('capture_height',None)
        if row['id']=='ethereum-uniswap-v3':row['failed_public_proof_requests']=[reference(p) for p in sorted((BASE/'ethereum-v3-proof-probes').glob('*.json'))]
        if row['id']=='ethereum-uniswap-v2':
            replay=json.loads((BASE/'ethereum-replay/row-input.json').read_text())
            directory=json.loads((BASE/'documents/v2-and-compiler-index.json').read_text())['uni-v2-sdk-constants']
            row['sources'].append(directory)
            row['upgrade']['kind']='direct-source-proven'
            row['rebuild']=replay['rebuild'];row['rebuild']['inputs']['compiler']=reference(ROOT/'integration/artifacts/local/r01-public-compilers/soljson-v0.5.16+commit.9c3226ce.js');row['rebuild']['report']=reference(BASE/'ethereum-replay/output/rebuild-report.json')
            row['proof']={'kind':'ethereum-header-transactions-account-mpt-code','report':reference(BASE/'ethereum-replay/output/proof-report.json')}
            row['execution']={'scope':'original-runtime-local-bank-not-original-chain-history','original_chain_historical_execution':False,'report':reference(BASE/'ethereum-replay/output/execution-report.json'),'fixture':reference(BASE/'ethereum-replay/output/execution-fixture.json')}
            row['reference_sources'].append(reference(ROOT/'integration/tools/reference/evm-execution/source-pins.json'))
            row['qualification_rule']='explicit-anchor-commitment-checked-runtime + exact-public-source-compiler-rebuild + original-runtime-local-recipient-execution; no historical-state execution or consensus claim'
        if row['chain']=='solana':
            row['runtime']['sbf_identity']='full ProgramData payload including allocation padding, offset 45; not a reconstructed source build'
            row['public_verifiable_build_status']=reference(BASE/'solana-verifiable'/((row['protocol'])+'.json'))
            names=('cpmm-actual-sbf-comparison.json','cpmm-modern-actual-sbf-comparison.json') if row['protocol']=='raydium-cpmm' else (row['protocol']+'-actual-sbf-comparison.json',)
            row['sbf_build_attempts']=[reference(BASE/'solana-verifiable'/name) for name in names if (BASE/'solana-verifiable'/name).is_file()]
            if row['id']=='solana-raydium-cpmm':row['sbf_build_replay']=reference(BASE/'solana-verifiable/cpmm-modern-build-replay.json')
    manifest=BASE/'public-source-build-inputs.json';report=BASE/'public-source-build-report.json'
    if manifest.exists():
        entries={r['id']:r for r in json.loads(manifest.read_text())['entries']}
        results={r['id']:r for r in json.loads(report.read_text())['entries']} if report.exists() else {}
        for row in catalog['deployments']:
            if row['id'] in entries:
                row['public_source_compilation']={'input_manifest':reference(manifest),'entry_id':row['id'],'reported_rebuilt':results.get(row['id'],{}).get('source_rebuild_verified',False),'authority':'public-source-repository + local compiler result; state proof and execution remain separate'}
                if report.exists():row['public_source_compilation']['report']=reference(report)
    exact_manifest=BASE/'metadata-exact/exact-source-manifest.json';exact_report=BASE/'metadata-exact/exact-source-report-complete.json'
    if exact_manifest.exists() and exact_report.exists():
        entries={e['id']:e for e in json.loads(exact_manifest.read_text())['entries']}
        for row in catalog['deployments']:
            if row['id'] in entries:
                row['exact_source_compilation']={'entry_id':row['id'],'input_manifest':reference(exact_manifest),'report':reference(exact_report),'authority':'complete original runtime including metadata after public compiler rebuild and compiler-declared immutable linking; state proof and historical execution remain separate'}
    catalog['historical_anchors']={'catalog':reference(BASE/'history-catalog.json'),'scope':reference(BASE/'history-scope.json'),'qualified_catalog':reference(BASE/'history-catalog-qualified.json'),'qualified_scope':reference(BASE/'history-scope-qualified.json'),'acceptance_tool':'integration/tools/acceptance/r01_public_catalog_history.py'}
    catalog.update(schema_version=1,catalog_kind='finite-read-only-public-deployment-evidence',
        all_requested_versions_verified=False,production_ready=False,consensus_verified=False,
        qualification='computed only by validator with --replay; stored passed/status fields never grant qualification')
    required=scope_rows(catalog['deployments'])
    scope={'schema_version':1,'id':'r01-initial-eight-chains-observed-versions','chain_targets_sha256':sha256((ROOT/'integration/config/chain-targets.json').read_bytes()),
        'plan_sections':['3','7.1','12.11','12.14'],'trace_bindings':['R01.1','R01.2','7.1.r01-deployment-evidence','12.14.r01-deployed-source-rebuild'],
        'required':required,'scope_limit':'one observed runtime version per initial chain/protocol; selected real pools; not all markets, historical upgrades or all protocol releases',
        'source_policy':'only credential-free public HTTPS inputs; no account, private service or API key required'}
    write('catalog.json',catalog);write('scope.json',scope)
    eth=[r for r in catalog['deployments'] if r['id']=='ethereum-uniswap-v2'];write('catalog-ethereum.json',{'schema_version':1,'deployments':eth})
    write('scope-ethereum.json',scope|{'id':'r01-ethereum-h26355865-single-runtime-local-execution','required':[r for r in required if r['chain']=='ethereum' and r['protocol']=='uniswap-v2']})
    write('manifest.json', inventory(BASE))
    print(json.dumps({'deployments':len(required),'chains':len(set(r['chain'] for r in required)),'sample_pools':sum(len(r.get('sample_pools',[])) for r in catalog['deployments']),'programdata_payload_bytes':sum(r['runtime']['bytes'] for r in catalog['deployments'] if r['chain']=='solana')},indent=2))
if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--observations', type=Path, help='Versioned capture directory with observations.json')
    args = parser.parse_args()
    if args.observations:
        build_observations(args.observations)
    else:
        build()
