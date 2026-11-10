#!/usr/bin/env python3
from pathlib import Path
import os
root=Path(__file__).resolve().parents[3]
for project in ['chain-parser-go','chain-parser-rust']:
 for base in ['internal','cmd','crates']:
  folder=root/project/base
  if not folder.exists():continue
  for path in folder.rglob('*'):
   if path.suffix not in ['.go','.rs','.toml']:continue
   text=path.read_text()
   for bad in ['xone-routing','xone-optimizer','xone-quote-api','xone_routing','xone_optimizer','xone_quote_api']:
    if bad in text:raise SystemExit(f'Parser crosses compute boundary: {path}')
for directory,children,files in os.walk(root/'dex-router'):
 children[:]=[name for name in children if name!='target']
 if 'Cargo.toml' not in files:continue
 path=Path(directory)/'Cargo.toml'
 if 'xone-solana-ingest' in path.read_text():raise SystemExit('Router depends on live ingest')
for crate in ['protocol-math','routing','optimizer','execution-plan']:
 text=(root/'dex-router/crates'/crate/'Cargo.toml').read_text()
 for dependency in ['reqwest.workspace','axum.workspace','tokio.workspace']:
  if dependency in text:raise SystemExit(f'Pure compute crate depends on transport/runtime: {crate}/{dependency}')
print('Three service authority boundaries passed.')
