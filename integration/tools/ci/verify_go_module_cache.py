#!/usr/bin/env python3
"""Verify external Go caches; the local protobuf module is source, not a zip."""
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile

ROOT=Path(__file__).resolve().parents[3]
MODULE=ROOT/'chain-parser-go'
LOCAL='xone/common/protoc/golang'
REPLACEMENT='replace xone/common/protoc/golang => ../common/protoc/golang'

def run(*args):
    return subprocess.check_output(['go','-C',str(MODULE),*args],text=True,stderr=subprocess.STDOUT)

def modules(text):
    decoder=json.JSONDecoder();result={}
    while text.strip():
        text=text.lstrip();row,end=decoder.raw_decode(text);text=text[end:]
        if row['Path'] in result:raise ValueError('DUPLICATE_MODULE')
        result[row['Path']]=row
    return result

def main():
    original=modules(run('list','-mod=readonly','-m','-json','all'))
    local=original.get(LOCAL,{})
    if {key for key,row in original.items() if 'Replace' in row}!={LOCAL} or Path(local.get('Dir','')).resolve()!=(ROOT/'common/protoc/golang').resolve():raise ValueError('UNEXPECTED_LOCAL_REPLACEMENT')
    text=(MODULE/'go.mod').read_text()
    if text.count(REPLACEMENT)!=1:raise ValueError('UNEXPECTED_REPLACEMENT_DECLARATION')
    text=text.replace(REPLACEMENT,'')
    text,count=re.subn(r'^\s*xone/common/protoc/golang v0\.0\.0\s*$', '',text,flags=re.M)
    if count!=1:raise ValueError('UNEXPECTED_LOCAL_REQUIREMENT')
    with tempfile.TemporaryDirectory(prefix='xone-external-module-verification-') as temporary:
        path=Path(temporary)/'external.mod';path.write_text(text)
        path.with_suffix('.sum').write_bytes((MODULE/'go.sum').read_bytes())
        external=modules(run('list','-mod=readonly','-modfile='+str(path),'-m','-json','all'))
        identity=lambda rows:{key:(row.get('Version'),row.get('Sum'),row.get('Main',False)) for key,row in rows.items() if key!=LOCAL}
        if identity(external)!=identity(original) or any('Replace' in row for row in external.values()):raise ValueError('EXTERNAL_MODULE_GRAPH_CHANGED')
        output=run('mod','verify','-modfile='+str(path))
    if 'all modules verified' not in output:raise ValueError('EXTERNAL_MODULE_VERIFICATION_UNCONFIRMED')
    print(json.dumps({'state':'EXTERNAL_MODULE_CACHE_VERIFIED','external_modules':len(external)-1,'local_source_module':LOCAL,'local_source_verification':'FROZEN_SOURCE_AND_CONTRACT_GATES','module_graph_unchanged':True}))
    print(output,end='')

if __name__=='__main__':main()
