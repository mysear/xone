#!/usr/bin/env python3
"""Build a separate versioned EVM evidence catalog; never rewrites the main manifest."""
import argparse
import json
from pathlib import Path
import sys
sys.path.insert(0,str(Path(__file__).resolve().parent))
from r01_public_catalog import ROOT,load_json,sha256
from r01_public_catalog_capture import reference
from r01_public_catalog_evm_acceptance import environment_differences
BASE=ROOT/'integration/test/source/deployment-catalog';OUT=BASE/'evm-continuation-v2'

def write(path,value):path.write_text(json.dumps(value,indent=2)+'\n')

def build(version="v2"):
    if version not in {"v2","v3"}:raise ValueError("EVM_DELIVERY_VERSION")
    original=load_json(BASE/'catalog.json');by_id={r['id']:r for r in original['deployments'] if r['chain']!='solana'};entries=[];native=[]
    selected={'base':'base-01','optimism':'optimism-01','polygon':'polygon-01','bsc':'bsc-05','arbitrum':'arbitrum-02','avalanche':'avalanche-03'}
    if version=='v3' and all((OUT/'bsc-06'/f).exists() for f in ['fork-execution.json','final-commitments-report.json','source-runtime-report.json']):selected['bsc']='bsc-06'
    completed=OUT/('avalanche-07' if version=='v3' else 'avalanche-06')
    if (completed/'fork-execution.json').exists() and (completed/'final-commitments-report.json').exists() and (completed/'source-runtime-report.json').exists():selected['avalanche']=completed.name
    for chain,name in selected.items():
        directory=OUT/name;execution=load_json(directory/'fork-execution.json');plan=load_json(directory/'execution-plan.json');anchor=load_json(directory/'anchor.json');sourcefile=directory/'source-runtime-report-v2.json'
        if not sourcefile.exists():sourcefile=directory/'source-runtime-report.json'
        source=load_json(sourcefile);source_by_id={r['id']:r for r in source['entries']};env=directory/'environment-differences-v2.json';write(env,environment_differences(directory))
        proof=directory/'prestate-evidence-v2.json'
        if not proof.exists():proof=directory/'prestate-evidence.json'
        finality=directory/'finality-observation-v2.json'
        if not finality.exists():finality=directory/'finality-observation.json'
        for case in execution['cases']:
            prior=by_id[case['catalog_deployment_id']];built=source_by_id[prior['id']];proxy=built.get('proxy',{});row={'id':prior['id'],'chain':chain,'chain_id':execution['chain_id'],'chain_key':prior['chain_key'],'genesis_hash':prior['genesis_hash'],'genesis_observation':reference(directory/'auxiliary-genesis.json') if (directory/'auxiliary-genesis.json').exists() else prior['identity']['genesis'],'protocol':case['protocol'],'factory':case['factory'],'pool':case['pool'],'anchor':{'height':execution['anchor']['height'],'hash':execution['anchor']['hash']},'factory_runtime_sha256':case['factory_runtime_sha256'],'pool_runtime_sha256':case['pool_runtime_sha256'],'implementation_runtime_sha256':proxy.get('runtime_sha256'),'implementation_address':proxy.get('target'),'directory':str(directory.relative_to(ROOT)),'inputs':{'anchor':reference(directory/'anchor.json'),'plan':reference(directory/'execution-plan.json'),'execution':reference(directory/'fork-execution.json'),'rpc_index':reference(directory/'rpc-index.json'),'local_helper_compilation':reference(directory/'local-helper-compiler.json'),'source_runtime':reference(sourcefile),'environment':reference(env)},'local_domain_profile_support':prior.get('domain_profile_support'),'authority_boundary':'finite public original-pool reference; Go SPEC3, normal Parser/Router differential and native hardfork fees remain separate'}
            if proof.exists():row['inputs']['prestate']=reference(proof)
            if finality.exists():row['inputs']['subsequent_finality']=reference(finality)
            entries.append(row)
        spec={'chain':plan['chain'],'chain_key':'evm:'+str(plan['chain_id']),'directory':str(directory.relative_to(ROOT)),'target_header':anchor['header'],'genesis_observation':reference(directory/'auxiliary-genesis.json') if (directory/'auxiliary-genesis.json').exists() else by_id[plan['cases'][0]['catalog_deployment_id']]['identity']['genesis'],'entrypoints':['evm.VerifyChainBlockJSON','evm.VerifyNativeReceiptEvidence','evm.VerifyExecutionAccountProof'],'public_prestate':reference(proof) if proof.exists() else None,'proof_json_pointer':'/deployments/{index}/proof','code_json_pointer':'/deployments/{index}/code','pool_scope':[{'factory':c['factory'],'pool':c['pool'],'protocol':c['protocol']} for c in plan['cases']],'normal_parser_router_differential_required':True,'go_native_spec3_verified':False}
        receipts=directory/'canonical-native-receipts.json'
        if receipts.exists():spec['canonical_native_receipts']=reference(receipts)
        if chain=='avalanche':spec['storage_key_encoding']='coreth-partitioned-storage/v1';spec['storage_partition_rule']='physical[0] = logical[0] & 0xfe, then normal secure keccak(physical); odd raw-key proofs must not be retroactively accepted';spec['logical_physical_bindings_directory']=str((directory/'storage-key-bindings').relative_to(ROOT))
        native.append(spec)
    write(OUT/'source-catalog-evm-snapshot.json',{'schema_version':1,'deployments':list(by_id.values()),'authority':'retained original public catalog rows; no qualification inferred from configuration'})
    write(OUT/f'catalog-{version}.json',{'schema_version':2,'kind':'finite-public-original-EVM-pool-references','entries':entries,'full_R01_qualified':False})
    keys=['id','chain','protocol','factory','pool','factory_runtime_sha256','pool_runtime_sha256','implementation_runtime_sha256','anchor'];required=[{k:r[k] for k in keys} for r in entries]
    write(OUT/f'scope-{version}.json',{'schema_version':2,'id':'remaining-eleven-EVM-public-reference-scope','required':required})
    qualified=[r for r in entries if r['chain']!='avalanche' or (version=='v3' and selected['avalanche']=='avalanche-07')];write(OUT/f'catalog-qualified-{version}.json',{'schema_version':2,'kind':'finite-public-original-EVM-pool-references','entries':qualified,'full_R01_qualified':False});write(OUT/f'scope-qualified-{version}.json',{'schema_version':2,'id':str(len(qualified))+'-proven-public-EVM-original-pool-references','required':[{k:r[k] for k in keys} for r in qualified]})
    counterexamples=[]
    for name in ['avalanche-04','avalanche-05','avalanche-06']:
        d=OUT/name
        if (d/'prestate-evidence.json').exists():counterexamples.append({'directory':str(d.relative_to(ROOT)),'anchor':reference(d/'anchor.json'),'prestate':reference(d/'prestate-evidence.json'),'native_headers_json_pointer':'/native_headers','account_proof_json_pointer':'/deployments/{index}/proof','code_json_pointer':'/deployments/{index}/code','saved_diagnostic':reference(d/'state-proof-diagnostics.json') if (d/'state-proof-diagnostics.json').exists() else None,'verdict':'native independent replay required; preserve raw nonzero values and exact proof failures'})
    write(OUT/f'go-native-replay-inputs-{version}.json',{'schema_version':1,'authority':'actual raw public native header/body/receipt and state proof inputs; no fabricated NativeHistoryEvidence or dataset ID','entries':native,'avalanche_storage_counterexample':reference(OUT/'avalanche-04/state-proof-diagnostics.json'),'avalanche_counterexample_inputs':counterexamples,'native_go_verdict':'pending independent SPEC3 consumer; diagnostic records must not be normalized into passing proof'})
    # Close all literal file references used by the selected sources and immutable
    # compiler manifest. The main manifest and Go/Rust files are not modified.
    external={}
    def visit(value):
        if isinstance(value,dict):
            if isinstance(value.get('path'),str) and isinstance(value.get('sha256'),str):
                p=(ROOT/value['path']).resolve()
                if p.is_file() and not p.is_relative_to(OUT.resolve()):external[value['path']]=value
            for v in value.values():visit(v)
        elif isinstance(value,list):
            for v in value:visit(v)
    for value in [entries,load_json(BASE/'public-source-build-inputs.json'),load_json(BASE/'metadata-exact/exact-source-manifest.json')]:visit(value)
    for f in OUT.rglob('*.json'):
        if f.name=='manifest-v2.json':continue
        try:visit(load_json(f))
        except (ValueError,OSError):pass
    tools=[]
    names=['r01_public_catalog.py','r01_public_catalog_capture.py','r01_public_catalog_history.py','r01_public_catalog_proof.cjs','r01_public_catalog_fork.cjs','r01_public_catalog_sources.cjs']
    toolpaths=[ROOT/'integration/tools/acceptance'/n for n in names]+list((ROOT/'integration/tools/acceptance').glob('r01_public_catalog_evm*.py'))+list((ROOT/'integration/tools/acceptance').glob('r01_public_catalog_evm*.cjs'))+list((ROOT/'integration/tools/acceptance/tests').glob('test_r01_public_catalog_evm*.py'))
    files=[reference(p) for p in sorted(OUT.rglob('*')) if p.is_file() and p!=OUT/'manifest-v2.json']
    manifest_path=OUT/'manifest-v2.json' if version=='v2' else ROOT/'integration/artifacts/local/r01-public-evm-manifest-v3.json'
    write(manifest_path,{'schema_version':2,'data_directory':str(OUT.relative_to(ROOT)),'excluded_inventory_files':['manifest-v2.json'],'files':files,'tools':[reference(p) for p in sorted(set(toolpaths))],'external_inputs':list(external.values()),'file_count':len(files),'authority':'complete byte inventory for this independent EVM version; not the main catalog manifest or a deployment qualification'})
    print(json.dumps({'entries':len(entries),'qualified_reference_scope_count':len(qualified),'files':len(files),'external_inputs':len(external)},indent=2))
if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--version',choices=['v2','v3'],default='v2');build(parser.parse_args().version)
