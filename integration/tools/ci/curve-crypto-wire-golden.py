#!/usr/bin/env python3
"""Independent protoc bytes for the complete, versioned Crypto pricing/LP state."""
from pathlib import Path
import hashlib, importlib.util, json, subprocess
ROOT=Path(__file__).resolve().parents[3]
spec=importlib.util.spec_from_file_location('meta_wire',Path(__file__).with_name('curve-meta-wire-golden.py'))
base=importlib.util.module_from_spec(spec);spec.loader.exec_module(base)
base.AMOUNTS |= {'gamma','out_fee','fee_gamma','allowed_extra_profit','adjustment_step','invariant','price_scale','price_oracle','last_prices','xcp_profit','xcp_profit_a','virtual_price','total_supply','admin_receiver_balance','raw_balances'}
base.DIGESTS.add('lp_token_code_sha256')
def pool():
 p=json.loads((ROOT/'integration/test/contracts/multi-asset-state.json').read_text())['pools'][0]
 s=p['multi_asset'];s.update(assets=s['assets'][:2],weights=[],amplification='400000',admin_fee=5000000000,vault='',vault_code_sha256='',pool_id='')
 for a in s['assets']:a.update(balance=str(10**24),scaling=str(10**18))
 p.update(protocol='curve-crypto2-full',adapter_version='curve-crypto2-full/d7d04cd9-standard-1',reserve0=str(10**24),reserve1=str(10**24),fee_numerator=3000000,fee_denominator=10000000000)
 s['curve_crypto']={'gamma':'10000000000','out_fee':'30000000','fee_gamma':'230000000000000','allowed_extra_profit':str(10**18),'adjustment_step':'146000000000000','ma_half_time':600,'invariant':str(2*10**24),'price_scale':str(10**18),'price_oracle':'1001000000000000000','last_prices':'999000000000000000','last_prices_timestamp':1767225600,'xcp_profit':str(10**18),'xcp_profit_a':str(10**18),'virtual_price':str(10**18),'not_adjusted':False,'future_a_gamma_time':0,'lp_token':'0x0000000000000000000000000000000000000099','lp_token_code_sha256':'d3f43d24527d6a8ed0fa9f24d110ad7cc9325f40da335ed327e8e0e002a8694d','total_supply':str(10**24),'admin_receiver':'0x0000000000000000000000000000000000000098','admin_receiver_balance':'0','raw_balances':[str(10**24)]*2,'timestamp':1767225600,'block_number':100,'block_hash':'0x'+'a'*64}
 return p

def generate():
 s=json.loads((ROOT/'integration/test/evm/baseline.json').read_text());p=pool();s['pools']=[p];s['source']='SYNTHETIC_CRYPTO_COMPLETE_STATE_WIRE_CONTRACT';ref=s['reference'];ref['block'].update(number_or_slot=100,hash='0x'+'a'*64,timestamp_ms=1767225600000)
 for key,val in [('adapter_set_hash',[p['adapter_version']]),('scope_hash',[p['pool_id']])]:ref[key]=hashlib.sha256(json.dumps(val,separators=(',',':')).encode()).hexdigest()
 raw=subprocess.run(['protoc','-I',str(ROOT/'common/protoc/proto'),'--encode=dex.v1.OnlineSnapshot',str(ROOT/'common/protoc/proto/dex/v1/state.proto')],input=base.encode(s).encode(),capture_output=True,check=True).stdout
 data=json.dumps({'snapshot':s,'protobuf_hex':raw.hex(),'protobuf_sha256':hashlib.sha256(raw).hexdigest()},indent=2)+'\n'
 for project in ['integration','chain-parser-go','chain-parser-rust','dex-router']:(ROOT/project/'test/contracts/curve-crypto-wire.json').write_text(data)
 print('Independent Crypto wire bytes',len(raw))
if __name__=='__main__':generate()
