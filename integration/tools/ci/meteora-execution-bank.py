#!/usr/bin/env python3
"""Execution account fixture from phase53 original SBF bank and pinned decoder closure."""
from pathlib import Path
import base64, hashlib, json, sys
ROOT=Path(__file__).resolve().parents[3]
def sha(p): return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def main():
    f=json.loads((ROOT/'integration/test/contracts/meteora-orders-exact-out.json').read_bytes())
    source=ROOT/'integration/artifacts/local/core-meteora-orders-exact-out-original-20261111.json'; e=json.loads(source.read_bytes())
    i=87; c=f['cases'][i]; n=e['records'][i]['native']; d=f['decoder']; accounts=dict(d['shared_accounts']); accounts.update(d['cases'][i]['accounts'])
    for key,a in n['before'].items():
        if key not in accounts: accounts[key]=dict(owner=a['owner'],executable=a['executable'],lamports=a['lamports'],data=[a['data_base64'],'base64'])
    def wallet(mint,owner):
        return next(key for key,a in accounts.items() if len(base64.b64decode(a['data'][0]))==165 and base64.b64decode(a['data'][0])[:32]==decode58(mint) and base64.b64decode(a['data'][0])[32:64]==decode58(owner))
    sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
    from r01_public_catalog_solana_bank import native_imports
    LiteSVM,Pubkey,*_=native_imports(); svm=LiteSVM().with_mainnet_features()
    memo='MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr'; a=svm.get_account(Pubkey.from_string(memo))
    if a is None: raise ValueError('ORIGINAL_MEMO_REQUIRED')
    accounts[memo]=dict(owner=str(a.owner),executable=a.executable,lamports=a.lamports,data=[base64.b64encode(a.data).decode(),'base64'])
    accounts[n['payer']]=dict(owner='11111111111111111111111111111111',executable=False,lamports=10_000_000_000,data=['','base64'])
    p=c['steps'][0]['pool']; step=c['steps'][0]
    from_path=wallet(p['token0'],n['payer']); to_path=wallet(p['token1'],n['recipient'])
    result=dict(schema_version=1,source_sha256=sha(source),source_index=i,authority='controlled original SBF bank plus pinned programdata, runtime Memo and explicitly funded 10-SOL test payer; no live network or user funds',pool=p,accounts=accounts,caller=n['payer'],recipient=n['recipient'],sourceTokenAccount=from_path,destinationTokenAccount=to_path,genesis='5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d',blockhash=encode58(bytes([77])*32),target=step['amount'],requiredInput=step['actual_input'],after=step['after'])
    for service in ['integration','dex-router']:
        (ROOT/service/'test/contracts/meteora-execution-bank.json').write_text(json.dumps(result,separators=(',',':'))+'\n')
    print(json.dumps(dict(passed=True,accounts=len(accounts),target=result['target'],input=result['requiredInput'])))
def decode58(s):
    chars='123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'; n=0
    for c in s:n=n*58+chars.index(c)
    return b'\0'*(len(s)-len(s.lstrip('1')))+(n.to_bytes((n.bit_length()+7)//8,'big')if n else b'')
def encode58(b):
    chars='123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'; n=int.from_bytes(b,'big'); out=''
    while n:n,r=divmod(n,58);out=chars[r]+out
    return '1'*(len(b)-len(b.lstrip(b'\0')))+out
if __name__=='__main__':main()
