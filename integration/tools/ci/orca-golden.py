#!/usr/bin/env python3
"""Regenerate/check full-state Orca goldens by executing the unchanged original program natively."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tomllib

sys.dont_write_bytecode = True

ROOT = Path(__file__).resolve().parents[3]
REFERENCE = ROOT / 'integration/tools/reference/orca-whirlpools'
REPORT = ROOT / 'integration/artifacts/local/orca-golden-generation.json'
LOG = ROOT / 'integration/artifacts/local/orca-golden-generation.log'
TOOLCHAIN = '1.90.0'


def digest(data):
    return hashlib.sha256(data).hexdigest()

def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

def encoded(value):
    # Python integer serialization preserves every u64 bit; wide CoreAmount values are strings.
    return (json.dumps(value, indent=2, sort_keys=True, ensure_ascii=True)+'\n').encode()

def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--check', action='store_true', help='compare independently regenerated outputs without modifying fixtures')
    ap.add_argument('--offline', action='store_true', help='forbid Cargo network access; requires already cached locked dependencies')
    args = ap.parse_args()
    sources_module = load('orca_fetch', Path(__file__).with_name('orca-reference-fetch.py'))
    sources = sources_module.verify()
    generator = load('orca_generate', REFERENCE / 'generate.py')
    rustup = shutil.which('rustup') or str(Path.home()/'.cargo/bin/rustup')
    tools = {name:subprocess.check_output([rustup,'which','--toolchain',TOOLCHAIN,name],text=True).strip() for name in ['cargo','rustc','rustdoc']}
    env = dict(os.environ, RUSTC=tools['rustc'], RUSTDOC=tools['rustdoc'], CARGO_TARGET_DIR=str(REFERENCE/'target'))
    flags = ['--locked'] + (['--offline'] if args.offline else [])
    commands = []
    output = []
    def run(arguments):
        commands.append(arguments)
        result = subprocess.run(arguments, cwd=ROOT, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        output.append('$ '+' '.join(arguments)+'\n'+result.stdout)
        if result.returncode:
            LOG.parent.mkdir(parents=True, exist_ok=True)
            LOG.write_text('\n'.join(output))
            print(result.stdout[-10000:], file=sys.stderr)
            raise SystemExit(result.returncode)
        return result.stdout
    compiler = run([tools['rustc'],'--version']).strip()
    cargo_version = run([tools['cargo'],'--version']).strip()
    run([tools['cargo'],'test','--release','--manifest-path',str(REFERENCE/'Cargo.toml'),*flags])
    # cargo test builds the executable today; explicit build keeps the regeneration contract stable.
    run([tools['cargo'],'build','--release','--manifest-path',str(REFERENCE/'Cargo.toml'),*flags])
    metadata = json.loads(run([tools['cargo'],'metadata','--format-version','1','--manifest-path',str(REFERENCE/'Cargo.toml'),*flags]))
    locked = tomllib.loads((REFERENCE/'Cargo.lock').read_text())
    checksums = {(p['name'], p['version']):p.get('checksum') for p in locked['package']}
    dependencies = sorted([{'name':p['name'],'version':p['version'],'source':p['source'],'checksum':checksums[(p['name'],p['version'])]} for p in metadata['packages']],key=lambda p:(p['name'],p['version']))
    local = {Path(p['manifest_path']).resolve() for p in metadata['packages'] if p['source'] is None}
    assert local == {REFERENCE/'Cargo.toml', REFERENCE/'upstream/programs/whirlpool/Cargo.toml'}, 'unexpected local/production math dependency' 
    executable = REFERENCE/'target/release/orca-whirlpools-reference'
    corpus, fixture, tickmath = generator.build(executable)
    counts = generator.verify(corpus)
    second_corpus, second_fixture, second_tickmath = generator.build(executable)
    assert encoded(corpus) == encoded(second_corpus) and encoded(fixture) == encoded(second_fixture) and encoded(tickmath) == encoded(second_tickmath), 'nondeterministic native regeneration'
    outputs = {
        ROOT/'integration/test/contracts/orca-quotes.json':encoded(corpus),
        ROOT/'dex-router/test/contracts/orca-quotes.json':encoded(corpus),
        ROOT/'integration/test/solana/orca-base.json':encoded(fixture),
        REFERENCE/'orca-tickmath.json':encoded(tickmath),
    }
    for path, data in outputs.items():
        if args.check:
            assert path.is_file() and path.read_bytes() == data, f'regeneration mismatch: {path.relative_to(ROOT)}'
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
    harness = sorted(list((REFERENCE/'src').glob('*.rs'))+list((REFERENCE/'tests').glob('*.rs'))+[REFERENCE/'generate.py',REFERENCE/'Cargo.toml',REFERENCE/'README.md',REFERENCE/'boundary-return-baseline.json',Path(__file__).resolve(),Path(__file__).with_name('orca-reference-fetch.py')])
    report = {
        'schema_version':1,'repository':sources['repository'],'revision':sources['revision'],'archive_sha256':sources['archive_sha256'],
        'unchanged_source_files_verified':len(sources['files']),'source_manifest_sha256':digest((REFERENCE/'source-metadata.json').read_bytes()),'source_adaptations':[],
        'execution':'native unchanged original program crate, default-features=false and no-entrypoint; original public account loaders, sparse builder, manager::swap, Whirlpool.update_after_swap',
        'compiler':compiler,'cargo':cargo_version,'cargo_lock_sha256':digest((REFERENCE/'Cargo.lock').read_bytes()),'original_cargo_lock_sha256':digest((REFERENCE/'upstream/Cargo.lock').read_bytes()),
        'harness_files':{str(p.relative_to(ROOT)):digest(p.read_bytes()) for p in harness},'dependencies':dependencies,
        'counts':counts,'tickmath_vectors':len(tickmath['vectors']),'exhaustive_tick_price_sha256':tickmath['reference']['exhaustive_tick_le_i32_price_le_u128_sha256'],
        'output_sha256':{str(p.relative_to(ROOT)):digest(data) for p,data in outputs.items()},
        'pool_id':fixture['pool']['pool_id'],'programdata':fixture['pool']['orca_whirlpool']['programdata'],'program_code_sha256':fixture['pool']['orca_whirlpool']['program_code_sha256'],
        'business_math_imported':False,'synthetic_proof_identity':True,'sbf_transaction_executed':False,'token_program_executed':False,'validator_executed':False,
        'deterministic_regeneration':True,'independent_native_regenerations':2,'golden_copies_identical':True,'offline':args.offline,'check_mode':args.check,
        'tests':'cargo test --release --locked; all harness tests and doctests passed','commands':commands,
        'environment':{k:env[k] for k in ['RUSTC','RUSTDOC','CARGO_TARGET_DIR']},'coverage_limits':corpus['reference']['limitations'],
    }
    REPORT.parent.mkdir(parents=True, exist_ok=True)
    REPORT.write_bytes(encoded(report))
    output.append('COVERAGE '+json.dumps(counts,sort_keys=True))
    output.append('OUTPUT_SHA256 '+json.dumps(report['output_sha256'],sort_keys=True))
    output.append('PASS: original archive/tree verified; native release tests passed; every golden regenerated'+(' and matched byte-for-byte' if args.check else ''))
    LOG.write_text('\n'.join(output)+'\n')
    print(json.dumps({'counts':counts,'tickmath_vectors':len(tickmath['vectors']),'revision':sources['revision'],'source_files':len(sources['files']),'output_sha256':report['output_sha256'],'report':str(REPORT),'check':args.check,'offline':args.offline},indent=2))

if __name__ == '__main__':
    main()
