"""Synthetic receipt fixtures sealed by a pinned independent EthereumJS oracle."""
from functools import lru_cache
import copy
import json
from pathlib import Path
import hashlib

ROOT = Path(__file__).resolve().parents[3]
EMPTY_ROOT = '0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421'

@lru_cache(maxsize=256)
def _seal(raw):
    catalog = json.loads((ROOT / 'integration/test/contracts/receipt-root-fixtures.json').read_text())
    key = hashlib.sha256(raw.encode()).hexdigest()
    if key not in catalog['cases']:
        raise ValueError('unconfigured receipt fixture; explicitly refresh the independent reference')
    return catalog['cases'][key]

def seal_receipts(receipts):
    # Only consensus fields are cached; bank/tx metadata must belong to each arrival.
    consensus = [{k: v for k, v in r.items() if k in ('type', 'root', 'status', 'cumulativeGasUsed', 'logs')} for r in receipts]
    for r in consensus:
        r['logs'] = [{k: l[k] for k in ('address', 'topics', 'data')} for l in r['logs']]
    sealed = _seal(json.dumps({'receipts': consensus}, sort_keys=True))
    result = copy.deepcopy(receipts)
    for r, s in zip(result, sealed['receipts']):
        for key in ('type', 'root', 'status', 'cumulativeGasUsed', 'logsBloom'):
            if key in s:
                r[key] = s[key]
    return sealed['root'], result

SINGLE_EMPTY_ROOT, _SINGLE = seal_receipts([{'logs': []}])
SINGLE_EMPTY_FIELDS = {k: v for k, v in _SINGLE[0].items() if k != 'logs'}
