#!/usr/bin/env python3
"""Prepare explicit normal-service configs for local Compose."""
from pathlib import Path
import json,secrets
root=Path(__file__).resolve().parents[3]
out=root/'integration/compose/.runtime'
token=secrets.token_urlsafe(32)
for name,port in [('chain-parser-go',8091),('chain-parser-rust',8092),('dex-router',8080)]:
 config=json.loads((root/name/'config/config-dev.yaml').read_text())
 config['api'].update(listen=f'0.0.0.0:{port}',auth_token=token)
 config['state']['path']='/app/.runtime/state'
 config['journal']['source_urls']=['http://chain-parser-go:8091','http://chain-parser-rust:8092']
 if name=='dex-router':config['journal']['source_auth_tokens']={url:token for url in config['journal']['source_urls']}
 target=out/name/'config';target.mkdir(parents=True,exist_ok=True)
 path=target/'config-dev.yaml';path.write_text(json.dumps(config,indent=2)+'\n');path.chmod(0o600)
print('Prepared local Compose configs. Configure reachable RPC endpoints and pool catalogs before starting parsers.')
