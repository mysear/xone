#!/usr/bin/env python3
"""Fresh legacy, multi-model and bounded original Curve MetaUSD process regression."""
from pathlib import Path
import json
import secrets
import sys

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / 'integration/artifacts/local'
PREFIX = 'curve-meta'
SCOPES = ['three-service-smoke.json', 'multi-model-three-service-smoke.json', 'curve-ng-three-service-smoke.json', 'curve-meta-three-service-smoke.json']


def _write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + '.' + secrets.token_hex(6) + '.tmp')
    try:
        temporary.write_text(json.dumps(value, indent=2) + '\n')
        temporary.replace(path)
    finally:
        temporary.unlink(missing_ok=True)


def _invalidate():
    for name in [PREFIX + '-acceptance.json', PREFIX + '-scope-execution.json', PREFIX + '-regression-command.json'] + SCOPES:
        (OUT / name).unlink(missing_ok=True)
        _write(OUT / name, {'passed': False, 'state': 'RUNNING', 'verified': False,
                           'all_requested_features_complete': False, 'production_ready': False})


def run():
    _invalidate()
    try:
        import curve_meta_evidence as evidence
        from weighted_regression import run as execute
        return execute(evidence, evidence.PRODUCERS, PREFIX)
    except BaseException as error:
        (OUT / (PREFIX + '-regression-command.json')).unlink(missing_ok=True)
        _write(OUT / (PREFIX + '-regression-command.json'),
               {'passed': False, 'error': str(error), 'error_type': type(error).__name__,
                'verified': False, 'all_requested_features_complete': False, 'production_ready': False})
        raise


def main(argv=None):
    _invalidate()
    try:
        import argparse
        parser = argparse.ArgumentParser(description=__doc__)
        parser.parse_args(sys.argv[1:] if argv is None else argv)
        return run()
    except BaseException as error:
        (OUT / (PREFIX + '-regression-command.json')).unlink(missing_ok=True)
        _write(OUT / (PREFIX + '-regression-command.json'),
               {'passed': False, 'error': str(error), 'error_type': type(error).__name__,
                'verified': False, 'all_requested_features_complete': False, 'production_ready': False})
        raise


if __name__ == '__main__':
    main()
