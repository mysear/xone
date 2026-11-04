#!/usr/bin/env python3
from pathlib import Path
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
for path in (root/'dex-router').rglob('Cargo.toml'):
 if 'target' in path.parts:continue
 if 'xone-solana-ingest' in path.read_text():raise SystemExit('Router depends on live ingest')
for crate in ['protocol-math','routing','optimizer','execution-plan']:
 text=(root/'dex-router/crates'/crate/'Cargo.toml').read_text()
 for dependency in ['reqwest.workspace','axum.workspace','tokio.workspace']:
  if dependency in text:raise SystemExit(f'Pure compute crate depends on transport/runtime: {crate}/{dependency}')
print('Three service authority boundaries passed.')
