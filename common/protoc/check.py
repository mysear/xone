#!/usr/bin/env python3
"""Regenerate in a temporary directory and compare checked-in language types."""
from pathlib import Path
import os,subprocess,tempfile
root=Path(__file__).resolve().parent
env=os.environ.copy();env['PATH']=str(Path.home()/'.cargo/bin')+os.pathsep+env['PATH']
with tempfile.TemporaryDirectory(prefix='xone-proto-check-') as directory:
 out=Path(directory);(out/'golang').mkdir()
 subprocess.run(['protoc','-I',str(root/'proto'),'--go_out='+str(out/'golang'),'--go_opt=module=xone/common/protoc/golang',str(root/'proto/dex/v1/state.proto')],check=True)
 subprocess.run([str(Path.home()/'.cargo/bin/cargo'),'+1.90.0','run','--locked','--manifest-path',str(root/'generator/Cargo.toml'),'--',str(out/'rust/dexpb')],check=True,env=env)
 for current,generated in [(root/'golang/dexpb/state.pb.go',out/'golang/dexpb/state.pb.go'),(root/'rust/dexpb/dex.v1.rs',out/'rust/dexpb/dex.v1.rs')]:
  if current.read_bytes()!=generated.read_bytes():raise SystemExit('Generated type drift: '+str(current.relative_to(root)))
print('Shared Go and Rust generated types match the protocol source.')
