#!/usr/bin/env python3
"""Run every EVM test/example/fuzz seed once with race and default Go budgets.

Discover from the actual compiled package. Sorted bounded shards avoid spending
one package's default 10-minute budget on the entire growing original corpus.
No test name is skipped and a failing shard stops the command immediately.
"""
import re
import argparse
import unicodedata
import subprocess
import sys
PACKAGE='./internal/chain/evm'
SHARD_TESTS=12

def parse_inventory(stdout):
    names=[]
    for line in stdout.splitlines():
        if not line.startswith(('Test','Fuzz','Example')):continue
        # Go identifiers permit Unicode letters and decimal digits, not just
        # ASCII. A test-like unrecognized line must fail, never disappear.
        valid=bool(line) and (line[0]=='_' or unicodedata.category(line[0]).startswith('L'))
        valid=valid and all(c=='_' or unicodedata.category(c).startswith('L') or unicodedata.category(c)=='Nd' for c in line[1:])
        if not valid:raise ValueError('unrecognized actual test inventory entry: '+line)
        names.append(line)
    if not names or len(names)!=len(set(names)):
        raise ValueError('empty or duplicate actual EVM test inventory')
    return sorted(names)

def make_shards(names,limit):
    if type(limit)is not int or limit<1:raise ValueError('positive integer shard size required')
    if not names or len(names)!=len(set(names)):raise ValueError('exact nonempty inventory required')
    ordered=sorted(names);return [ordered[i:i+limit]for i in range(0,len(ordered),limit)]

def run_pattern(names):return '^('+'|'.join(re.escape(name)for name in names)+')$'

def run_evm_shards(run=subprocess.run,limit=SHARD_TESTS):
    listed=run(['go','test','-race','-list','.',PACKAGE],capture_output=True,text=True)
    if listed.returncode:
        if listed.stdout:print(listed.stdout,end='')
        if listed.stderr:print(listed.stderr,end='',file=sys.stderr)
        return listed.returncode
    names=parse_inventory(listed.stdout);shards=make_shards(names,limit)
    print('EVM race inventory: '+str(len(names))+' cases in '+str(len(shards))+' complete shards',flush=True)
    for index,shard in enumerate(shards):
        print('EVM race shard '+str(index+1)+'/'+str(len(shards))+': '+', '.join(shard),flush=True)
        result=run(['go','test','-race',PACKAGE,'-run',run_pattern(shard)])
        if result.returncode:return result.returncode
    return 0

def run_all(run=subprocess.run,limit=SHARD_TESTS):
    discovered=run(['go','list','-race','./...'],capture_output=True,text=True)
    if discovered.returncode:
        if discovered.stdout:print(discovered.stdout,end='')
        if discovered.stderr:print(discovered.stderr,end='',file=sys.stderr)
        return discovered.returncode
    packages=discovered.stdout.splitlines()
    evm='xone/chain-parser-go/internal/chain/evm'
    if not packages or len(packages)!=len(set(packages)) or evm not in packages:
        raise ValueError('complete unique package discovery must include EVM')
    others=[p for p in packages if p!=evm]
    if others:
        result=run(['go','test','-race',*others])
        if result.returncode:return result.returncode
    return run_evm_shards(run,limit)

def main(argv=None,run=subprocess.run):
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--all',action='store_true',help='run all other packages and every EVM shard')
    args=parser.parse_args(argv)
    try:return run_all(run) if args.all else run_evm_shards(run)
    except (OSError,ValueError) as error:print(str(error),file=sys.stderr);return 1

if __name__=='__main__':sys.exit(main())
