#!/usr/bin/env python3
"""Build explicit local-only pinned runtime manifests from retained account banks."""
from pathlib import Path
import hashlib,json,sys
ROOT=Path(__file__).resolve().parents[3]
def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def runtime_manifest(chain,genesis):
 runtime=ROOT/'integration/artifacts/local/r01-solana-rebuild/python'
 files={str(p.relative_to(runtime)):sha(p)for folder in ['solders','jsonalias']for p in (runtime/folder).rglob('*')if p.is_file()and '__pycache__'not in p.parts and p.suffix in ['.py','.pyi','.so','.pyd']}
 files['typing_extensions.py']=sha(runtime/'typing_extensions.py')
 python=Path(sys.executable).resolve()
 return dict(schema_version=1,chain_key=chain,genesis_hash=genesis,python_executable=dict(path=str(python),sha256=sha(python)),runtime_directory=str(runtime),runtime_files=files,banks=[],max_compute_units=1400000,timeout_ms=15000,concurrency=2)

def write_bank_manifest(directory, name, bank, manifest):
 directory.mkdir(parents=True, exist_ok=True)
 bank_path = directory / (name + '-bank.json')
 bank_path.write_text(json.dumps(bank, separators=(',', ':')) + '\n')
 target = directory / (name + '.json')
 manifest = dict(manifest, banks=[dict(path=str(bank_path), sha256=sha(bank_path))])
 target.write_text(json.dumps(manifest, indent=2) + '\n')
 return target

def main():
 if len(sys.argv)==5 and sys.argv[1]=='--rpc-capture':
  manifest=runtime_manifest(sys.argv[2],sys.argv[3])
  manifest['rpc_capture']=True
  Path(sys.argv[4]).write_text(json.dumps(manifest,indent=2)+'\n')
  return
 token_only = sys.argv[1:] == ['--token2022']
 if len(sys.argv)!=1 and not token_only:
  raise SystemExit('usage: solana-sbf-runtime-manifests.py [--token2022 | --rpc-capture CHAIN GENESIS OUTPUT]')
 service_directory=ROOT/'dex-router/test/contracts/sbf-runtime'
 token=json.loads((ROOT/'integration/test/contracts/meteora-token2022-execution-bank.json').read_bytes())['cases']
 if token_only:
  bank = token[1]
  manifest = runtime_manifest(bank['pool']['chain_key'], bank['genesis'])
  target = write_bank_manifest(service_directory, 'token2022', bank, manifest)
  print(json.dumps(dict(passed=True, manifest=str(target))))
  return
 directory=ROOT/'integration/artifacts/local/local-sbf-runtime'
 ray=json.loads((ROOT/'integration/test/contracts/raydium-execution-bank.json').read_bytes())['cases']
 values={'meteora':json.loads((ROOT/'integration/test/contracts/meteora-execution-bank.json').read_bytes()),'wsol':json.loads((ROOT/'integration/test/contracts/meteora-wsol-bank.json').read_bytes()),'token2022':token[1],'raydium':ray[0],'raydium-reverse':ray[1]}
 manifests={}
 for name,bank in values.items():
  manifest=runtime_manifest(bank['pool']['chain_key'],bank['genesis'])
  manifests[name]=str(write_bank_manifest(directory, name, bank, manifest))
  write_bank_manifest(service_directory, name, bank, manifest)
 captured=dict(manifest,rpc_capture=True,banks=[])
 (directory/'rpc-capture.json').write_text(json.dumps(captured,indent=2)+'\n')
 (service_directory/'rpc-capture.json').write_text(json.dumps(captured,indent=2)+'\n')
 preparations={}
 for name,file in [('meteora','integration/artifacts/local/temporary-wsol-browser-a3959fab715d/unwrap-preparation.json'),('token2022','integration/artifacts/local/phase61-browser-preflight/token2022-refund-preparation.json'),('raydium','integration/artifacts/local/phase63-preflight/raydium-0-preparation.json')]:
  value=json.loads((ROOT/file).read_bytes());prepared=value.get('preparation',value)
  # Historical phase60 wire is unchanged; project only newly introduced DTO
  # fields for the service-local replay fixture, then recompute its commitment.
  prepared.setdefault('sourceTokenProgram','TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA')
  prepared.setdefault('destinationTokenProgram','TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA')
  prepared.setdefault('expectedDestinationTransferFee','0')
  prepared.pop('preparationHash',None);prepared['preparationHash']=hashlib.sha256(json.dumps(prepared,sort_keys=True,separators=(',',':'),ensure_ascii=False).encode()).hexdigest()
  preparations[name]=prepared
 for service in ['integration','dex-router']:(ROOT/service/'test/contracts/sbf-preparations.json').write_text(json.dumps(preparations,separators=(',',':'))+'\n')
 (directory/'index.json').write_text(json.dumps(manifests,indent=2)+'\n');print(json.dumps(dict(passed=True,manifests=manifests,runtimeFiles=len(manifest['runtime_files']))))
if __name__=='__main__':main()
