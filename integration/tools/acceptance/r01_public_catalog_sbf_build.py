#!/usr/bin/env python3
"""Replay owned official SBF builds from immutable public source candidates."""
import argparse
import base64
import json
import os
from pathlib import Path
import subprocess
import time
from r01_public_catalog import ROOT, require, sha256, load_json, CatalogError
from r01_public_catalog_capture import reference
from r01_public_catalog_solana import compare_programdata_build, elf_bounds

BASE=ROOT/'integration/test/source/deployment-catalog'
OWNED=ROOT/'integration/artifacts/local/r01-public-sbf-runtime'
SPECS={'raydium-clmm':('6d7a0f82d2f293c553c69aaf943377367c6c4cc6','programs/amm/Cargo.toml','raydium_clmm'),'orca-whirlpools':('cc683ec92acb5e7606caa3d8e514d9383e9c7c11','programs/whirlpool/Cargo.toml','whirlpool')}

def build(name):
    revision,manifest,artifact_name=SPECS[name];source=OWNED/(name+'-'+revision)
    acquisition=load_json(BASE/'solana-verifiable'/(name+'-actual-build-source.json'))
    require(acquisition['source_revision']==revision and sha256((ROOT/acquisition['lock']['path']).read_bytes())==acquisition['lock']['sha256'],'SBF_LOCK_PIN')
    sdk=OWNED/'3.1.10/solana-release/bin/platform-tools-sdk/sbf';tools=OWNED/'platform-tools-v1.52'
    # The official postprocessor invokes install.sh independently of skip-tools-install.
    for marker in ['platform-tools-v1.52.md','criterion-v2.3.2.md']:
        require((sdk/'dependencies'/marker).exists(),'SDK_POSTPROCESS_MARKER_REQUIRED')
    require((sdk/'dependencies/platform-tools').resolve()==tools.resolve(),'OWNED_SDK_TOOLCHAIN')
    cargo_home=OWNED/(name+'-cargo-home');cargo_home.mkdir(exist_ok=True)
    config='[source.crates-io]\nreplace-with = "r01-public-vendor"\n[source.r01-public-vendor]\ndirectory = '+json.dumps(str(OWNED/'vendor-modern'))+'\n'
    if name=='raydium-clmm':
        config+='[source.parity-git]\ngit = "https://github.com/raydium-io/parity-common"\nreplace-with = "r01-parity-archive"\n[source.r01-parity-archive]\ndirectory = '+json.dumps(str(OWNED/'clmm-parity-git'))+'\n'
    (cargo_home/'config.toml').write_text(config)
    env=os.environ.copy();env.update(CARGO_HOME=str(cargo_home),RUSTUP_HOME=str(OWNED/(name+'-rustup-home')),CARGO_TARGET_DIR=str(OWNED/(name+'-sbf-target')),RUSTC=str(tools/'rust/bin/rustc'),PATH=str(tools/'rust/bin')+os.pathsep+env['PATH'])
    output=OWNED/(name+'-sbf-output');command=[str(OWNED/'3.1.10/solana-release/bin/cargo-build-sbf'),'--manifest-path',str(source/manifest),'--offline','--skip-tools-install','--no-rustup-override','--sbf-sdk',str(sdk),'--sbf-out-dir',str(output),'--jobs','4','--','--locked'];log=OWNED/(name+'-sbf-build.log');start=time.monotonic()
    with log.open('w') as stream:process=subprocess.run(command,cwd=source,env=env,stdout=stream,stderr=subprocess.STDOUT,timeout=900)
    result={'program':name,'source_revision':revision,'command':command,'exit_code':process.returncode,'elapsed_seconds':round(time.monotonic()-start,3),'log':str(log.relative_to(ROOT)),'source_modified':False,'global_install':False,'compile_mode':'offline directory sources; SDK postprocess dependencies already mounted','environment':{k:env[k] for k in ['CARGO_HOME','RUSTUP_HOME','CARGO_TARGET_DIR','RUSTC']}}
    # Verify every originally pinned source file, including Cargo.lock, after build.
    require(sha256((ROOT/acquisition['source_metadata']['path']).read_bytes())==acquisition['source_metadata']['sha256'],'SBF_SOURCE_METADATA_PIN');metadata=load_json(ROOT/acquisition['source_metadata']['path']);pins=metadata['archives'][0]['files'] if 'archives' in metadata else metadata['files'];failed=[f['path'] for f in pins if not (source/f['path']).is_file() or sha256((source/f['path']).read_bytes())!=f['sha256']]
    diagnostics=log.read_text();result['compiler_stack_frame_diagnostics']=diagnostics.count('overwrites values in the frame');result['postprocessor_unknown_syscall_warning']='not known syscalls' in diagnostics;result['executable_runtime_validity_claimed']=False
    result['pinned_source_files']=len(pins);result['source_unmodified']=not failed;result['changed_source_files']=failed;require(not failed,'SBF_BUILD_CHANGED_UPSTREAM_SOURCE')
    (BASE/'solana-verifiable'/(name+'-actual-build-attempt.json')).write_text(json.dumps(result,indent=2)+'\n')
    if process.returncode==0:
        artifact=output/(artifact_name+'.so');require(artifact.is_file(),'OFFICIAL_SBF_OUTPUT_MISSING');data=artifact.read_bytes()
        row=next(r for r in load_json(BASE/'catalog.json')['deployments'] if r['protocol']==name);observation=load_json(ROOT/row['runtime']['observation']['path']);payload=base64.b64decode(observation['response']['result']['value'][1]['data'][0])[45:]
        comparison={'schema_version':1,'program':row['address'],'anchor':row['anchor'],'source_commit':revision,'compiler_toolchain':'official Agave 3.1.10 / platform-tools v1.52 / rustc 1.89.0-dev','artifact':reference(artifact),'artifact_bytes':len(data),'programdata_payload_bytes':len(payload),'full_programdata_payload_sha256':sha256(payload),'source_modified':False,'source_build_completed':True,'global_install':False,'build_elf_bounds':elf_bounds(data),'program_elf_bounds':elf_bounds(payload),'first_byte_difference':next((i for i,(a,b) in enumerate(zip(data,payload)) if a!=b),None)}
        try:comparison['comparison']=compare_programdata_build(payload,data);comparison['deployed_rebuild_verified']=True
        except CatalogError as error:comparison.update(deployed_rebuild_verified=False,comparison_error=str(error))
        (BASE/'solana-verifiable'/(name+'-actual-sbf-comparison.json')).write_text(json.dumps(comparison,indent=2)+'\n');result['comparison']={k:comparison[k] for k in ['artifact','artifact_bytes','first_byte_difference','deployed_rebuild_verified']}
    print(json.dumps(result,indent=2));return process.returncode

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('program',choices=SPECS);args=parser.parse_args();raise SystemExit(build(args.program))
if __name__=='__main__':main()
