#!/usr/bin/env python3
"""Build explicit local-only pinned runtime manifests from retained account banks."""
from pathlib import Path
import hashlib,json,sys
ROOT=Path(__file__).resolve().parents[3]
def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def main():
 runtime=ROOT/'integration/artifacts/local/r01-solana-rebuild/python';directory=ROOT/'integration/artifacts/local/local-sbf-runtime';directory.mkdir(parents=True,exist_ok=True)
 files={str(p.relative_to(runtime)):sha(p)for folder in ['solders','jsonalias']for p in (runtime/folder).rglob('*')if p.is_file()and '__pycache__'not in p.parts and p.suffix in ['.py','.pyi','.so','.pyd']}
 files['typing_extensions.py']=sha(runtime/'typing_extensions.py')
 token=json.loads((ROOT/'integration/test/contracts/meteora-token2022-execution-bank.json').read_bytes())['cases']
 ray=json.loads((ROOT/'integration/test/contracts/raydium-execution-bank.json').read_bytes())['cases']
 values={'meteora':json.loads((ROOT/'integration/test/contracts/meteora-execution-bank.json').read_bytes()),'wsol':json.loads((ROOT/'integration/test/contracts/meteora-wsol-bank.json').read_bytes()),'token2022':token[1],'raydium':ray[0],'raydium-reverse':ray[1]}
 manifests={}
 service_directory=ROOT/'dex-router/test/contracts/sbf-runtime';service_directory.mkdir(parents=True,exist_ok=True)
 for name,bank in values.items():
  path=directory/(name+'-bank.json');path.write_text(json.dumps(bank,separators=(',',':'))+'\n')
  manifest=dict(schema_version=1,chain_key=bank['pool']['chain_key'],genesis_hash=bank['genesis'],python_executable=dict(path=str(Path(sys.executable).resolve()),sha256=sha(Path(sys.executable).resolve())),runtime_directory=str(runtime),runtime_files=files,banks=[dict(path=str(path),sha256=sha(path))],max_compute_units=1400000,timeout_ms=15000,concurrency=2)
  target=directory/(name+'.json');target.write_text(json.dumps(manifest,indent=2)+'\n');manifests[name]=str(target)
  service_bank=service_directory/(name+'-bank.json');service_bank.write_bytes(path.read_bytes());service_manifest=dict(manifest,banks=[dict(path=str(service_bank),sha256=sha(service_bank))]);(service_directory/(name+'.json')).write_text(json.dumps(service_manifest,indent=2)+'\n')
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
 (directory/'index.json').write_text(json.dumps(manifests,indent=2)+'\n');print(json.dumps(dict(passed=True,manifests=manifests,runtimeFiles=len(files))))
if __name__=='__main__':main()
