#!/usr/bin/env python3
"""Fresh complete comparison plus counterexamples on the measured native ELF."""
import base64,copy,hashlib,json,sys
from pathlib import Path
stage=Path(__file__).resolve().parent
root=next(p for p in stage.parents if (p/'integration/tools/acceptance/r01_public_catalog_solana_rebuild.py').is_file())
sys.path.insert(0,str(root/'integration/tools/acceptance'))
from r01_public_catalog_solana_rebuild import compare_artifact,bind_programdata,VerificationError,require
row=next(r for r in json.loads((root/'integration/test/source/deployment-catalog/catalog.json').read_text())['deployments'] if r['id']=='solana-raydium-clmm')
obs=json.loads((root/row['runtime']['observation']['path']).read_text());bind_programdata(row,obs)
payload=base64.b64decode(obs['response']['result']['value'][1]['data'][0],validate=True)[45:];artifact=(stage/'output/deploy/raydium_clmm.so').read_bytes();positive=compare_artifact(payload,artifact);require(positive['matched'],'measured-positive-full-ELF-match')
cases=[]
def changed_artifact(name,offset):
 changed=bytearray(artifact);changed[offset]^=1;result=compare_artifact(payload,bytes(changed));require(result['matched'] is False,'mutation-not-rejected:'+name)
 cases.append({'case':name,'authority':'in-memory counterfactual mutation of measured bytes; retained ELF unchanged','offset':offset,'matched':result['matched'],'reason':result['reason'],'first_difference':result['first_difference'],'mismatch_domains':result['mismatch_domains']})
changed_artifact('ELF-header-flags-one-byte',48)
changed_artifact('text-first-byte',positive['built_elf']['sections']['.text']['offset'])
changed_artifact('relocation-first-byte',positive['built_elf']['sections']['.rel.dyn']['offset'])
changed=bytearray(payload);changed[-1]=1;result=compare_artifact(bytes(changed),artifact);require(not result['matched'] and not result['tail_all_zero'],'nonzero-padding-tail-not-rejected');cases.append({'case':'one-nonzero-allocation-tail-byte','offset':len(changed)-1,'matched':result['matched'],'reason':result['reason']})
for name,mutate in [('wrong-upgrade-slot',lambda r:r['upgrade'].update(slot=r['upgrade']['slot']+1)),('header-only-code-hash',lambda r:r['runtime'].update(sha256=hashlib.sha256(payload[:64]).hexdigest()))]:
 candidate=copy.deepcopy(row);mutate(candidate)
 try:bind_programdata(candidate,obs)
 except VerificationError as e:cases.append({'case':name,'binding_rejected':True,'reason':str(e)})
 else:raise VerificationError('identity-counterexample-accepted:'+name)
report={'positive':{k:positive[k] for k in ['matched','reason','first_difference','payload_bytes','artifact_bytes','allocation_padding_bytes','tail_all_zero','payload_sha256','artifact_sha256']},'counterexamples':cases,'original_retained_artifact_sha256':hashlib.sha256((stage/'output/deploy/raydium_clmm.so').read_bytes()).hexdigest(),'in_memory_mutations_never_written_to_retained_ELF':True}
print(json.dumps(report,indent=2))
