#!/usr/bin/env python3
"""Original public ramp/stop exchanges through three services and two replicas."""
import json
from pathlib import Path
import curve_crypto_smoke as base
ROOT = base.ROOT
TARGET = base.OUT / 'curve-crypto-ramp-three-service-smoke.json'
FAULTS = ['lp-code', 'minter', 'raw-balance', 'template', 'ramp-origin-missing', 'ramp-origin-future', 'ramp-origin-domain', 'ramp-origin-width', 'ramp-end-width', 'ramp-end-before-origin', 'future-head']
base.wire.base.AMOUNTS |= {'initial_amp', 'initial_gamma'}

class Chain(base.Chain):
    def __init__(self):
        super().__init__('curve-crypto-ramp-service.json', 14)

    def advance(self, phase=None, fault=None):
        phase = getattr(self, 'phase', 0) if phase is None else phase
        timestamp = self.groups[0][phase]['pool']['multi_asset']['curve_crypto']['timestamp']
        base.require(getattr(self, 'timestamp', timestamp) <= timestamp, 'ramp source bank time moved backwards')
        return super().advance(phase, fault)

    def block(self, number):
        block = super().block(number)
        bank = self.banks.get(number)
        if bank and bank['fault'] == 'future-head':
            block['timestamp'] = hex(int(base.time.time()) + 120)
        return block

    def rpc(self, method, args):
        if method == 'eth_getStorageAt':
            with self.lock:
                anchor = args[2]
                base.require(isinstance(anchor, dict) and anchor.get('requireCanonical') is True, 'ramp origin missing canonical anchor')
                bank = next((b for b in self.banks.values() if b['hash'] == anchor.get('blockHash')), None)
                base.require(bank is not None, 'unknown ramp bank')
                item = next((i for i in bank['items'] if i['pool']['pool_id'] == args[0].lower()), None)
                base.require(item is not None, 'foreign ramp storage')
                c = item['pool']['multi_asset']['curve_crypto']
                r = c['ramp']
                slot = int(args[1], 16)
                fault = bank['fault']
                if slot in [5, 7] or slot == 8 and fault in ['ramp-end-width', 'ramp-end-before-origin']:
                    self.reads['ramp-origin'] = self.reads.get('ramp-origin', 0) + 1
                    if fault == 'ramp-origin-missing':
                        raise ValueError('missing original ramp storage')
                    value = (int(r['initial_amp']) << 128) | int(r['initial_gamma']) if slot == 5 else r['initial_time']
                    if slot == 5 and fault == 'ramp-origin-domain':
                        value = 1
                    elif slot == 7 and fault == 'ramp-origin-future':
                        value = c['timestamp'] + 1
                    elif slot == 7 and fault == 'ramp-origin-width' or slot == 8 and fault == 'ramp-end-width':
                        value = 1 << 65
                    elif slot == 8 and fault == 'ramp-end-before-origin':
                        value = r['initial_time'] - 1
                    return '0x' + base.word(value)
        return super().rpc(method, args)

def main():
    base.TARGET = TARGET
    base.write(base.initial())
    try:
        report = base.run(Chain, FAULTS, 'curve-crypto-ramp', recovery_phase=13)
        report['profile'] = 'curve-crypto2-ramp/d7d04cd9-ramp-1'
        base.write(report)
        print(json.dumps({'passed': report['passed'], 'checks': len(report['checks']), 'quotes': report['quote_checks']}))
    except BaseException as error:
        report = base.initial()
        report.update(state='FAILED', error=str(error), error_type=type(error).__name__)
        base.write(report)
        raise

if __name__ == '__main__':
    main()
