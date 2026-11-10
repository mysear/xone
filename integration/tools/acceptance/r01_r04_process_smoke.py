#!/usr/bin/env python3
"""Finite normal-binary PG/Kafka regressions; controlled sources are explicit."""
from pathlib import Path
import hashlib,json,os,secrets,shutil,time
from weighted_evidence import source_hash,write_result
ROOT=Path(__file__).resolve().parents[3];OUT=ROOT/'integration/artifacts/local';TARGET=OUT/'r01-r04-normal-process.json'
def require(value,message):
 if not value:raise ValueError(message)
def main():
 attempt=secrets.token_hex(6);report={'schema_version':1,'state':'RUNNING','passed':False,'all_requested_features_complete':False,'production_ready':False,'authority':'controlled RPC sources with actual local PG/Kafka and normal binaries; not independent live providers','started_at_unix_ms':int(time.time()*1000),'source_tree_sha256':source_hash()};write_result(TARGET,report)
 try:
  from receipt_root_smoke import FAULTS
  from receipt_witness_smoke import run
  limits={}
  def configure(alias,cfg):
   if alias=='go':
    raw={'max_bytes':512*1024*1024,'max_records':100000,'max_record_bytes':cfg['journal']['max_payload_bytes']*4+16384};cfg['journal']['raw_evidence']=raw;limits.update(raw)
  def retain(root,result):
   source=root/'go/state/raw-evidence/evm-31337';target=OUT/('r01-r04-raw-'+attempt)
   require(source.is_dir(),'normal Go process did not create configured raw archive')
   shutil.copytree(source,target)
   files=[p for p in target.iterdir()if p.is_file()and p.suffix in ['.raw','.dlq']]
   result['raw_evidence']={'directory':str(target.relative_to(ROOT)),'limits':limits,'raw_records':sum(p.suffix=='.raw'for p in files),'dlq_records':sum(p.suffix=='.dlq'for p in files),'files':{p.name:hashlib.sha256(p.read_bytes()).hexdigest()for p in files}}
   require(result['raw_evidence']['raw_records']>0,'normal process raw reception missing')
  result=run(prefix='r01-r04-evm',faults=FAULTS,configure=configure,retain=retain)
  require(result.get('passed')is True and len(result['checks'])==13,'incomplete normal EVM process matrix')
  require(result['raw_evidence']['dlq_records']>=len(FAULTS),'failed collection was not durably quarantined')
  report['evm']=result;write_result(TARGET,report)
  prior=OUT/('r01-r04-prior-source-wal-'+attempt);prior.mkdir()
  for old in OUT.glob('source-wal*'):
   if old.is_file():shutil.copy2(old,prior/old.name)
  import source_wal_smoke
  solana=source_wal_smoke.run();require(solana.get('passed')is True,'normal Solana RPC source WAL recovery failed');report['solana_rpc_wal']=solana
  require(source_hash()==report['source_tree_sha256'],'core source changed during normal-process run')
  report.update(passed=True,state='LOCAL_PROCESS_SCOPE_VERIFIED',completed_at_unix_ms=int(time.time()*1000),limits='no actual Yellowstone/mainnet execution/source failure-domain/online production recovery closure');write_result(TARGET,report)
  print(json.dumps({'passed':True,'evm_checks':len(result['checks']),'raw_records':result['raw_evidence']['raw_records'],'dlq_records':result['raw_evidence']['dlq_records'],'solana_rpc_checks':len(solana['checks']),'all_requested_features_complete':False}))
 except BaseException as error:
  report.update(state='FAILED',error=str(error),error_type=type(error).__name__,completed_at_unix_ms=int(time.time()*1000));write_result(TARGET,report);raise
if __name__=='__main__':main()
