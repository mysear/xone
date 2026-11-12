#!/usr/bin/env python3
"""Reverse original DLMM bank for native funding preparation; source WSOL ATA starts absent."""
from pathlib import Path
import base64,hashlib,json,sys
ROOT=Path(__file__).resolve().parents[3]
def main():
    original=ROOT/'integration/artifacts/local/core-meteora-orders-exact-out-original-20261111.json'
    fixture=json.loads((ROOT/'integration/test/contracts/meteora-orders-exact-out.json').read_bytes())
    n=json.loads(original.read_bytes())['records'][183]['native'];row=fixture['cases'][183]['steps'][0]
    if not n['passed']:raise ValueError('ORIGINAL_REVERSE_BANK_REQUIRED')
    # Reuse the pinned program closure and runtime programs, replace all bank-specific accounts.
    b=json.loads((ROOT/'integration/test/contracts/meteora-execution-bank.json').read_bytes())
    accounts=dict(fixture['decoder']['shared_accounts']);accounts.update(fixture['decoder']['cases'][183]['accounts'])
    for key,a in n['before'].items():
        if key not in accounts:accounts[key]=dict(owner=a['owner'],executable=a['executable'],lamports=a['lamports'],data=[a['data_base64'],'base64'])
    for key in ['MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr','ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL','11111111111111111111111111111111']:
        accounts[key]=b['accounts'][key]
    accounts[n['payer']]=dict(owner='11111111111111111111111111111111',lamports=10_000_000_000,executable=False,data=['','base64'])
    sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
    from r01_public_catalog_solana_bank import native_imports
    _,P,*_=native_imports();token='TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA';mint='So11111111111111111111111111111111111111112';ata=str(P.find_program_address([bytes(P.from_string(n['payer'])),bytes(P.from_string(token)),bytes(P.from_string(mint))],P.from_string('ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL'))[0])
    def account_for(mint,owner):
        return next(k for k,a in accounts.items()if len(base64.b64decode(a['data'][0]))==165 and base64.b64decode(a['data'][0])[:32]==bytes(P.from_string(mint))and base64.b64decode(a['data'][0])[32:64]==bytes(P.from_string(owner)))
    original_source=account_for(mint,n['payer']);destination=account_for(row['pool']['token0'],n['recipient'])
    b.update(pool=row['pool'],accounts=accounts,sourceTokenAccount=ata,destinationTokenAccount=destination,originalSourceTokenAccount=original_source,target=row['amount'],requiredInput=row['actual_input'],after=row['after'],source_index=183,source_sha256=hashlib.sha256(original.read_bytes()).hexdigest(),authority='controlled fixed original reverse swap bank; funded local test payer and absent canonical WSOL ATA explicitly declared')
    for service in ['integration','dex-router']:(ROOT/service/'test/contracts/meteora-wsol-bank.json').write_text(json.dumps(b,separators=(',',':'))+'\n')
    print(json.dumps(dict(passed=True,sourceAta=ata,input=b['requiredInput'],target=b['target'])))
if __name__=='__main__':main()
