#!/usr/bin/env python3
from pathlib import Path
import json
root=Path(__file__).resolve().parents[3];schema=json.loads((root/'integration/schemas/log-event.schema.json').read_text());count=0
for name in ['chain-parser-go','chain-parser-rust','dex-router']:
 path=root/'integration/artifacts/local'/(name+'.log')
 if not path.exists():raise SystemExit('Run fixture-test before log-schema-test')
 for line in path.read_text().splitlines():
  if not line.strip():continue
  if not line.startswith('{'):raise SystemExit('Non-JSON service log: '+name)
  event=json.loads(line)
  if not set(schema['required']).issubset(event) or set(event)-set(schema['properties']):raise SystemExit('Invalid log fields: '+name)
  if event['service']!=name or not isinstance(event['fields'],dict):raise SystemExit('Invalid service/fields')
  count+=1
if count<3:raise SystemExit('Insufficient log coverage')
print(json.dumps({'passed':True,'log_events':count,'scope':'three-process-json-field-schema'}))
