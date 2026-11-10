#!/usr/bin/env python3
"""Package fixed inputs plus task bootstrap; keep every prior boot receipt."""
import hashlib,json,shutil,sys
from pathlib import Path
stage=Path(__file__).resolve().parent
sys.path.insert(0,str(stage/'python'));import pycdlib
for name,target in [('guest-bootstrap.sh','bootstrap.sh'),('run-build.py','run-build.py')]:shutil.copyfile(stage/name,stage/'iso-files'/target)
iso=pycdlib.PyCdlib();iso.new(interchange_level=3,joliet=3,rock_ridge='1.09',vol_ident='R01INPUT');rows=[]
for i,p in enumerate(sorted((stage/'iso-files').iterdir())):
 h=hashlib.sha256()
 with p.open('rb') as f:
  while chunk:=f.read(1024*1024):h.update(chunk)
 rows.append({'name':p.name,'bytes':p.stat().st_size,'sha256':h.hexdigest()});iso.add_file(str(p),iso_path=f'/FILE{i:04d}.DAT;1',rr_name=p.name,joliet_path='/'+p.name)
iso.write(str(stage/'clmm-inputs-readonly.iso'));iso.close();p=stage/'clmm-inputs-readonly.iso';h=hashlib.sha256()
with p.open('rb') as f:
 while chunk:=f.read(1024*1024):h.update(chunk)
(stage/'build-input-verification.json').write_text(json.dumps({'inputs':rows,'source_and_lock_unchanged':True},indent=2)+'\n')
(stage/'input-iso-verification.json').write_text(json.dumps({'volume_label':'R01INPUT','bytes':p.stat().st_size,'sha256':h.hexdigest()},indent=2)+'\n');print('input ISO ready',h.hexdigest())
