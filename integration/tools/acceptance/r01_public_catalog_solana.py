#!/usr/bin/env python3
"""Public Solana executable boundaries and isolated official build-tool acquisition.

An ELF declared range is only a lower bound on file length. Padding is removed
only after an actual build file exactly matches the entire observed prefix and
all bytes after that authoritative build length are zero.
"""
import argparse
import base64
import hashlib
import json
from pathlib import Path
import struct
import sys
import tarfile
import urllib.request
sys.path.insert(0,str(Path(__file__).resolve().parent))
from r01_public_catalog import ROOT,CatalogError,require,sha256,load_json
from r01_public_catalog_capture import write_new,reference
BASE=ROOT/'integration/test/source/deployment-catalog'
RUNTIME=ROOT/'integration/artifacts/local/r01-public-sbf-runtime'

def elf_bounds(data):
    require(len(data)>=64 and data[:7]==b'\x7fELF\x02\x01\x01','SBF_ELF64_LE_HEADER')
    fields=struct.unpack_from('<HHIQQQIHHHHHH',data,16)
    _,machine,_,_,phoff,shoff,flags,ehsize,phentsize,phnum,shentsize,shnum,_=fields
    require(machine in (247,263) and ehsize==64 and phnum<=256 and shnum<=4096,'SBF_ELF_PROFILE')
    require(phnum==0 or phentsize==56,'SBF_PROGRAM_HEADER_SIZE');require(shnum==0 or shentsize==64,'SBF_SECTION_HEADER_SIZE')
    require(not(shoff and shnum==0),'SBF_EXTENDED_SECTIONS_UNSUPPORTED')
    extent=max(64,phoff+phnum*phentsize,shoff+shnum*shentsize);ranges=[]
    require(extent<=len(data),'SBF_HEADER_TABLE_TRUNCATED')
    for i in range(phnum):
        p=struct.unpack_from('<IIQQQQQQ',data,phoff+i*phentsize);offset,size=p[2],p[5]
        require(offset+size<=len(data),'SBF_SEGMENT_TRUNCATED');extent=max(extent,offset+size);ranges.append({'kind':'program','index':i,'offset':offset,'bytes':size})
    for i in range(shnum):
        section=struct.unpack_from('<IIQQQQIIQQ',data,shoff+i*shentsize)
        if section[1]!=8:
            offset,size=section[4],section[5];require(offset+size<=len(data),'SBF_SECTION_TRUNCATED');extent=max(extent,offset+size);ranges.append({'kind':'section','index':i,'offset':offset,'bytes':size})
    return {'machine':machine,'machine_profile':'EM_BPF' if machine==247 else 'EM_SBPF','flags':flags,'declared_file_extent':extent,'payload_bytes':len(data),'zero_suffix_after_declared_extent':not any(data[extent:]),'ranges':ranges,'actual_executable_file_length_known':False}

def compare_programdata_build(payload,artifact):
    observed=elf_bounds(payload);built=elf_bounds(artifact)
    require(len(artifact)>=built['declared_file_extent'] and len(artifact)<=len(payload),'SBF_BUILD_FILE_LENGTH')
    require(payload[:len(artifact)]==artifact,'SBF_BUILD_PREFIX_MISMATCH')
    require(not any(payload[len(artifact):]),'SBF_UNKNOWN_NONZERO_TAIL')
    return {'passed':True,'source_rebuild_verified':True,'executable_file_bytes':len(artifact),'executable_file_sha256':sha256(artifact),'full_programdata_payload_sha256':sha256(payload),'allocation_padding_bytes':len(payload)-len(artifact),'padding_authority':'exact compiled artifact file length + every remaining payload byte explicitly zero','declared_file_extent':observed['declared_file_extent']}

def record_bounds():
    results=[]
    for row in load_json(BASE/'catalog.json')['deployments']:
        if row['chain']!='solana':continue
        response=load_json(ROOT/row['runtime']['observation']['path'])['response']['result'];data=base64.b64decode(response['value'][1]['data'][0]);payload=data[45:]
        bounds=elf_bounds(payload)
        results.append({'id':row['id'],'program':row['address'],'anchor':row['anchor'],'programdata':row['upgrade']['programdata'],'upgrade_slot':row['upgrade']['slot'],'full_payload_sha256':sha256(payload),'elf':bounds,'qualification':'NOT_QUALIFIED','reason':'ELF lower bound and zero suffix are observations, not proof of source-built executable length'})
    write_new(BASE/'solana-verifiable/elf-boundaries.json',(json.dumps(results,indent=2)+'\n').encode());print(json.dumps([{k:r[k] for k in ('id','full_payload_sha256')}|{'declared_extent':r['elf']['declared_file_extent'],'zero_suffix':r['elf']['zero_suffix_after_declared_extent']} for r in results],indent=2))

def acquire_tools(version):
    require(version in ('1.17.0','1.18.26','2.1.0','3.1.10'),'OFFICIAL_TOOL_VERSION')
    repo='anza-xyz/agave' if version in ('2.1.0','3.1.10') else 'solana-labs/solana';asset='solana-release-x86_64-apple-darwin.tar.bz2'
    url=f'https://github.com/{repo}/releases/download/v{version}/{asset}';dest=RUNTIME/version;dest.mkdir(parents=True,exist_ok=True);archive=dest/asset
    if not archive.exists():
        req=urllib.request.Request(url,headers={'User-Agent':'XONE-source-evidence/1.0'});h=hashlib.sha256();total=0
        with urllib.request.urlopen(req,timeout=60) as response,archive.open('xb') as output:
            while chunk:=response.read(1024*1024):
                total+=len(chunk);require(total<=512*1024*1024,'OFFICIAL_TOOL_ARCHIVE_BUDGET');output.write(chunk);h.update(chunk)
    digest=sha256(archive.read_bytes());manifest={'version':version,'url':url,'archive':reference(archive),'sha256':digest,'hash_authority':'observed official HTTPS release artifact; no independent publisher digest asserted','files':[]}
    with tarfile.open(archive,'r:bz2') as tar:
        for member in tar:
            if not member.isfile():continue
            relative=Path(member.name)
            require(not relative.is_absolute() and '..' not in relative.parts,'TOOL_ARCHIVE_PATH')
            selected=member.name.endswith('/bin/cargo-build-sbf') or '/bin/sdk/sbf/' in member.name or member.name.endswith('/version.yml')
            if not selected:continue
            require(member.size<=64*1024*1024,'OFFICIAL_TOOL_MEMBER_LIMIT');data=tar.extractfile(member).read();target=dest/relative
            if not target.exists():write_new(target,data);target.chmod(member.mode&0o777)
            manifest['files'].append(reference(target))
    path=BASE/'solana-verifiable'/('native-toolchain-'+version+'.json')
    write_new(path,(json.dumps(manifest,indent=2)+'\n').encode());print(path)

def acquire_platform(version):
    require(version in ('v1.37','v1.52'),'PLATFORM_TOOL_VERSION')
    publisher='solana-labs' if version=='v1.37' else 'anza-xyz';url=f'https://github.com/{publisher}/platform-tools/releases/download/{version}/platform-tools-osx-x86_64.tar.bz2'
    archive=RUNTIME/f'platform-tools-osx-x86_64-{version}.tar.bz2';dest=RUNTIME/f'platform-tools-{version}';dest.mkdir(parents=True,exist_ok=True)
    if not archive.exists():
        pending=archive.with_suffix('.pending')
        with urllib.request.urlopen(urllib.request.Request(url,headers={'User-Agent':'XONE-source-evidence/1.0'}),timeout=60) as response,pending.open('xb') as output:
            total=0
            while chunk:=response.read(1024*1024):
                total+=len(chunk);require(total<=512*1024*1024,'PLATFORM_ARCHIVE_LIMIT');output.write(chunk)
        pending.rename(archive)
    selected=[]
    with tarfile.open(archive,'r:bz2') as tar:
        for member in tar:
            relative=Path(member.name);require(not relative.is_absolute() and '..' not in relative.parts,'PLATFORM_ARCHIVE_PATH');target=dest/relative
            if member.isfile():
                require(member.size<=256*1024*1024,'PLATFORM_MEMBER_LIMIT');data=tar.extractfile(member).read()
                if not target.exists():write_new(target,data);target.chmod(member.mode&0o777)
                if any(name in member.name for name in ['/bin/rustc','/bin/cargo','/bin/clang','/bin/llvm-objcopy']):selected.append(reference(target))
            elif member.issym():
                require((target.parent/member.linkname).resolve().is_relative_to(dest.resolve()),'PLATFORM_SYMLINK')
                if not target.exists() and not target.is_symlink():target.parent.mkdir(parents=True,exist_ok=True);target.symlink_to(member.linkname)
    result={'version':version,'url':url,'archive':reference(archive),'binary_files':selected,'cache_scope':str(dest.relative_to(ROOT)),'global_install':False}
    write_new(BASE/'solana-verifiable'/f'platform-tools-{version}.json',(json.dumps(result,indent=2)+'\n').encode());print(json.dumps(result,indent=2))


def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('command',choices=['boundaries','acquire-tools','acquire-platform','compare']);p.add_argument('--version',default='1.17.0');p.add_argument('--payload',type=Path);p.add_argument('--artifact',type=Path);a=p.parse_args()
    if a.command=='boundaries':record_bounds()
    elif a.command=='acquire-tools':acquire_tools(a.version)
    elif a.command=='acquire-platform':acquire_platform(a.version)
    else:print(json.dumps(compare_programdata_build(a.payload.read_bytes(),a.artifact.read_bytes()),indent=2))
if __name__=='__main__':main()
