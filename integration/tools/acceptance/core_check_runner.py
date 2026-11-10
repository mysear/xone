#!/usr/bin/env python3
"""Retain observed Make exit codes even if an app terminal session is retired."""
from pathlib import Path
import argparse,hashlib,json,re,subprocess,time
from weighted_evidence import ROOT,OUT,source_hash,write_result

def main():
 parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--service',required=True,choices=['chain-parser-go','chain-parser-rust','dex-router']);parser.add_argument('--stem',required=True);args=parser.parse_args()
 if not re.fullmatch(r'core-[a-z0-9-]{1,100}',args.stem):raise ValueError('OWNED_CHECK_STEM_REQUIRED')
 status=OUT/(args.stem+'.json');log=OUT/(args.stem+'.log')
 if status.exists()or log.exists():raise ValueError('NEW_CHECK_OUTPUT_REQUIRED')
 before=source_hash();report={'schema_version':1,'state':'RUNNING','passed':False,'service':args.service,'command':['make','check'],'source_tree_sha256':before,'started_at_unix_ms':int(time.time()*1000),'all_requested_features_complete':False,'production_ready':False};write_result(status,report)
 try:
  with log.open('x')as stream:process=subprocess.run(['make','check'],cwd=ROOT/args.service,stdout=stream,stderr=subprocess.STDOUT)
  report['observed_exit_code']=process.returncode;after=source_hash();report['source_tree_sha256_after']=after;report['source_changed']=after!=before
  text=log.read_text();matches=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',text)
  report['passed_test_executions']=sum(int(x[0])for x in matches);report['ignored_test_executions']=sum(int(x[2])for x in matches);report['counts_include_contract_target_repetition']=True
  report['passed']=process.returncode==0 and after==before;report['state']='COMPLETED'if report['passed']else'FAILED'
 except BaseException as error:
  report.update(state='INTERRUPTED_OR_FAILED',passed=False,error=str(error),error_type=type(error).__name__);raise
 finally:
  report['completed_at_unix_ms']=int(time.time()*1000)
  if log.exists():report['log']={'path':str(log.relative_to(ROOT)),'bytes':log.stat().st_size,'sha256':hashlib.sha256(log.read_bytes()).hexdigest()}
  write_result(status,report)
 print(json.dumps({'passed':report['passed'],'service':args.service,'observed_exit_code':report.get('observed_exit_code'),'status':str(status.relative_to(ROOT))}))
 if not report['passed']:raise SystemExit(1)
if __name__=='__main__':main()
