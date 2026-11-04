#!/usr/bin/env python3
"""Independent protoc snapshot from complete original adaptive reward bank."""
from pathlib import Path
import hashlib, importlib.util, json, subprocess, sys
ROOT=Path(__file__).resolve().parents[3]
sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
from orca_adaptive_dynamic_fixture import corpus,pool
spec=importlib.util.spec_from_file_location('adaptive_wire',Path(__file__).with_name('orca-adaptive-wire-golden.py'))
wire=importlib.util.module_from_spec(spec);spec.loader.exec_module(wire)
encode=wire.encode
pubkey=wire.pubkey

def generate():
 d=corpus();scenario=d['scenarios'][0];b=scenario['banks'][0];p=pool(scenario['config'],b['native_state'])
 snapshot=json.loads((ROOT/'integration/test/solana/baseline.json').read_text());snapshot['pools']=[p];snapshot['source']='SYNTHETIC_ORCA_ADAPTIVE_REWARDS_COMPLETE_WIRE'
 ref=snapshot['reference'];ref['block'].update(number_or_slot=b['slot'],timestamp_ms=b['timestamp']*1000)
 for key,field in [('scope_hash','pool_id'),('adapter_set_hash','adapter_version')]:ref[key]=hashlib.sha256(json.dumps([p[field]],separators=(',',':')).encode()).hexdigest()
 raw=subprocess.run(['protoc','-I',str(ROOT/'common/protoc/proto'),'--encode=dex.v1.OnlineSnapshot',str(ROOT/'common/protoc/proto/dex/v1/state.proto')],input=wire.encode(snapshot).encode(),capture_output=True,check=True).stdout
 text=json.dumps({'snapshot':snapshot,'protobuf_hex':raw.hex(),'protobuf_sha256':hashlib.sha256(raw).hexdigest()},indent=2)+'\n'
 for project in ['integration','chain-parser-go','chain-parser-rust','dex-router']:(ROOT/project/'test/contracts/orca-adaptive-dynamic-wire.json').write_text(text)
 print('Independent complete adaptive rewards wire bytes',len(raw))
if __name__=='__main__':generate()
