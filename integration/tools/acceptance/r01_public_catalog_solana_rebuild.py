#!/usr/bin/env python3
"""Fail-closed Solana deployment binding and complete SBF comparison.

All differences remain in the comparison. Section hashes diagnose a mismatch;
they never replace the complete artifact-prefix and all-zero-tail criterion.
RPC bank context and native local execution cannot establish an original bank.
"""
import argparse
import base64
import hashlib
import json
from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parents[3]
BASE = ROOT / 'integration/test/source/deployment-catalog'
LOADER = 'BPFLoaderUpgradeab1e11111111111111111111111'
ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'


class VerificationError(ValueError):
    pass


def require(condition, reason):
    if not condition:
        raise VerificationError(reason)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def b58decode(value):
    require(isinstance(value,str) and bool(value),'invalid-base58')
    try:
        number=0
        for ch in value:number=number*58+ALPHABET.index(ch)
        raw=number.to_bytes((number.bit_length()+7)//8,'big')
        return bytes(len(value)-len(value.lstrip('1')))+raw
    except ValueError as error:
        raise VerificationError('invalid-base58') from error


def b58encode(raw):
    number=int.from_bytes(raw,'big');s=''
    while number:number,idx=divmod(number,58);s=ALPHABET[idx]+s
    return '1'*(len(raw)-len(raw.lstrip(b'\0')))+s


def on_ed25519_curve(raw):
    # Compressed Edwards Y decompression, matching Solana's PDA curve check.
    prime=2**255-19;y=int.from_bytes(raw,'little')&((1<<255)-1)
    if y>=prime:return False
    d=-121665*pow(121666,-1,prime)%prime
    denominator=(d*y*y+1)%prime
    if not denominator:return False
    square=(y*y-1)*pow(denominator,-1,prime)%prime
    return square==0 or pow(square,(prime-1)//2,prime)==1


def programdata_pda(program):
    seed=b58decode(program);owner=b58decode(LOADER)
    require(len(seed)==len(owner)==32,'invalid-public-key')
    for bump in range(255,-1,-1):
        candidate=hashlib.sha256(seed+bytes([bump])+owner+b'ProgramDerivedAddress').digest()
        if not on_ed25519_curve(candidate):return b58encode(candidate)
    raise VerificationError('no-programdata-pda')


def inspect_elf(data):
    require(len(data)>=64 and data[:7]==b'\x7fELF\x02\x01\x01','elf64-le-header')
    kind,machine,version,entry,phoff,shoff,flags,ehsize,phsize,phnum,shsize,shnum,namesidx=struct.unpack_from('<HHIQQQIHHHHHH',data,16)
    require(kind==3 and machine in (247,263) and version==1 and ehsize==64,'sbf-elf-profile')
    require(phnum<=256 and 0<shnum<=4096 and shsize==64 and namesidx<shnum,'elf-table-profile')
    require(phnum==0 or phsize==56,'elf-program-header-size')
    extent=max(64,phoff+phsize*phnum,shoff+shsize*shnum)
    require(extent<=len(data),'elf-table-truncated')
    headers=[struct.unpack_from('<IIQQQQIIQQ',data,shoff+i*64) for i in range(shnum)]
    for h in headers:
        if h[1]!=8:require(h[4]+h[5]<=len(data),'elf-section-truncated')
    strings_header=headers[namesidx];require(strings_header[1]==3,'elf-section-name-table')
    strings=data[strings_header[4]:strings_header[4]+strings_header[5]];sections={}
    for h in headers:
        require(h[0]<len(strings),'elf-section-name-offset')
        end=strings.find(b'\0',h[0]);require(end>=0,'elf-section-name-terminator')
        try:name=strings[h[0]:end].decode('ascii')
        except UnicodeDecodeError as error:raise VerificationError('elf-section-name-encoding') from error
        require(name not in sections,'elf-duplicate-section-name')
        raw=data[h[4]:h[4]+h[5]] if h[1]!=8 else b''
        sections[name]={'offset':h[4],'bytes':h[5],'type':h[1],'sha256':digest(raw)}
        if h[1]!=8:extent=max(extent,h[4]+h[5])
    for i in range(phnum):
        p=struct.unpack_from('<IIQQQQQQ',data,phoff+i*phsize)
        require(p[2]+p[5]<=len(data),'elf-segment-truncated');extent=max(extent,p[2]+p[5])
    return {'machine':machine,'flags':flags,'entrypoint':entry,'declared_file_extent':extent,'sections':sections,'declared_extent_is_original_file_length':False}


def compare_artifact(payload,artifact):
    observed=inspect_elf(payload);built=inspect_elf(artifact)
    first=next((i for i,(a,b) in enumerate(zip(payload,artifact)) if a!=b),None)
    if first is None and len(artifact)>len(payload):first=len(payload)
    prefix=first is None and len(artifact)<=len(payload)
    zero_tail=not any(payload[len(artifact):])
    matched=prefix and zero_tail
    sections={name:{'observed':observed['sections'].get(name),'built':built['sections'].get(name),'equal':name in observed['sections'] and name in built['sections'] and observed['sections'][name]['sha256']==built['sections'][name]['sha256'] and observed['sections'][name]['bytes']==built['sections'][name]['bytes']} for name in sorted(observed['sections'].keys()|built['sections'].keys())}
    domains=(["elf-header"] if payload[:64]!=artifact[:64] else [])+[name for name,item in sections.items() if not item['equal']]
    if not zero_tail:domains.append('nonzero-tail-after-build-file')
    reason='complete-prefix-and-zero-padding' if matched else ('artifact-longer-than-payload' if len(artifact)>len(payload) else 'elf-prefix-different' if not prefix else 'nonzero-allocation-tail')
    return {'matched':matched,'reason':reason,'mismatch_domains':domains,'first_difference':first,'payload_sha256':digest(payload),'artifact_sha256':digest(artifact),'payload_bytes':len(payload),'artifact_bytes':len(artifact),'allocation_padding_bytes':len(payload)-len(artifact) if matched else None,'tail_all_zero':zero_tail,'observed_elf':observed,'built_elf':built,'sections':sections}


def bind_programdata(row,observation):
    address=row['address'];pd=row['upgrade']['programdata']
    require(programdata_pda(address)==pd,'programdata-pda-address')
    require(observation.get('method')=='getMultipleAccounts','observation-method')
    require(observation['params'][0]==[address,pd],'observation-address-order')
    result=observation['response']['result'];require(type(result['context']['slot']) is int and result['context']['slot']==row['anchor']['height'],'exact-observed-context-slot')
    accounts=result['value'];require(len(accounts)==2 and all(isinstance(a,dict) for a in accounts),'missing-program-accounts')
    program,data=accounts
    require(program['owner']==data['owner']==LOADER,'upgradeable-loader-owner')
    require(program['executable'] is True and data['executable'] is False,'program-executable-flags')
    require(program['data'][1]==data['data'][1]=='base64','account-encoding')
    try:p=base64.b64decode(program['data'][0],validate=True);d=base64.b64decode(data['data'][0],validate=True)
    except ValueError as error:raise VerificationError('account-base64') from error
    require(len(p)==36 and p[:4]==struct.pack('<I',2),'program-discriminator')
    require(p[4:]==b58decode(pd),'programdata-pointer')
    require(d[:4]==struct.pack('<I',3) and len(d)>45,'programdata-discriminator')
    slot=struct.unpack_from('<Q',d,4)[0];require(type(row['upgrade']['slot']) is int and slot==row['upgrade']['slot'],'upgrade-slot')
    require(slot<=result['context']['slot'],'upgrade-after-observed-bank')
    require(d[12] in (0,1),'upgrade-authority-option')
    authority=b58encode(d[13:45]) if d[12] else None
    require(authority==row['upgrade']['authority'],'upgrade-authority')
    payload=d[45:];inspect_elf(payload)
    require(digest(payload)==row['runtime']['sha256'] and len(payload)==row['runtime']['bytes'],'payload-identity')
    require(digest(p)==row['runtime']['program_sha256'],'program-account-identity')
    return {'program':address,'programdata':pd,'upgrade_slot':slot,'upgrade_authority':authority,'observed_context_slot':result['context']['slot'],'payload_sha256':digest(payload),'programdata_sha256':digest(d),'program_account_sha256':digest(p),'rpc_context_is_historical_bank_proof':False}


def token_delta(before,after,mint,recipient):
    require(before['mint']==after['mint']==mint and before['owner']==after['owner']==recipient,'recipient-token-binding')
    require(type(before['amount']) is int and type(after['amount']) is int and 0<=before['amount']<2**64 and 0<=after['amount']<2**64,'token-raw-u64')
    return after['amount']-before['amount']


def assess_bank(evidence):
    # Portable reports cannot self-authorize execution or bank ancestry. Native
    # replay is verified by rerunning the runner, not by trusting a saved flag.
    return {'bank_kind':evidence.get('bank_kind','unverified-report'),'historical_original_bank_verified':False,'net_recipient_verified':False,'reason':'requires-rerun-native-execution-and-independent-bank-ancestry; context-slot/getBlock/warp are insufficient'}


def audit_catalog(catalog):
    results=[]
    for row in catalog['deployments']:
        if row.get('chain')!='solana':continue
        path=ROOT/row['runtime']['observation']['path'];raw=path.read_bytes()
        require(digest(raw)==row['runtime']['observation']['sha256'],'observation-file-pin')
        obs=json.loads(raw);binding=bind_programdata(row,obs)
        payload=base64.b64decode(obs['response']['result']['value'][1]['data'][0])[45:]
        attempts=[]
        for ref in row.get('sbf_build_attempts',[]):
            report_raw=(ROOT/ref['path']).read_bytes();require(digest(report_raw)==ref['sha256'],'build-report-pin');report=json.loads(report_raw)
            artifact=ROOT/report['artifact']['path'];built=artifact.read_bytes();require(digest(built)==report['artifact']['sha256'],'build-artifact-pin')
            attempts.append({'source_commit':report['source_commit'],'artifact':str(artifact.relative_to(ROOT)),'comparison':compare_artifact(payload,built),'source_build_authority':'prior-build-report; command/source/toolchain pins require independent replay'})
        results.append({'id':row['id'],'binding':binding,'attempts':attempts,'historical_original_bank_verified':False})
    return {'schema_version':1,'deployments':results,'all_source_builds_matched':False,'historical_original_bank_verified':False}


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--catalog',type=Path,default=BASE/'catalog.json');parser.add_argument('--output',type=Path);args=parser.parse_args()
    result=audit_catalog(json.loads(args.catalog.read_text()));text=json.dumps(result,indent=2)+'\n'
    if args.output:args.output.write_text(text)
    else:print(text,end='')

if __name__=='__main__':main()
