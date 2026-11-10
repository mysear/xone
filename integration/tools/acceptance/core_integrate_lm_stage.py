#!/usr/bin/env python3
"""Integrate only LM changes against an exact preserved working-tree baseline."""
from pathlib import Path
import argparse
import hashlib
import json
import os
import secrets
import shutil
import sys
sys.path.insert(0, str(Path(__file__).resolve().parent))
from weighted_evidence import ROOT, OUT, require, sha, source_hash, write_result

STAGE = Path('/tmp/xone-pancake-lm-next-20261108')
BASE = '374ce438e77db25642fe7bac24d52b597328fc16ebbf1556e6878786dcaff012'
EXPECTED = '353168923511037e78e7cfac969d006929fdceea5431f716ba4784f2eab411af'
PROJECTS = ['chain-parser-go', 'chain-parser-rust', 'dex-router', 'common/protoc']
EXCLUDED = {'target', 'bin', '.git', '.runtime', 'node_modules', '__pycache__', 'secrets'}


def stage_hash():
    h = hashlib.sha256()
    for project in PROJECTS:
        files = []
        for directory, dirs, names in os.walk(STAGE/project):
            dirs[:] = [d for d in dirs if d not in EXCLUDED]
            for name in names:
                p = Path(directory)/name
                if p.suffix in ['.go','.rs','.toml','.mod','.sum','.lock','.proto','.yaml','.json','.py'] or name == 'Makefile':
                    files.append(p)
        for p in sorted(files):
            h.update(str(p.relative_to(STAGE)).encode())
            h.update(p.read_bytes())
    return h.hexdigest()


def plan():
    require(source_hash() == BASE, 'MAIN_BASE_CHANGED')
    require(stage_hash() == EXPECTED, 'LM_STAGE_SOURCE_CHANGED')
    baseline = json.loads((STAGE/'baseline-manifest.json').read_text())
    paths = set(baseline)
    for project in PROJECTS:
        for directory, dirs, names in os.walk(STAGE/project):
            dirs[:] = [d for d in dirs if d not in EXCLUDED]
            paths.update(str((Path(directory)/name).relative_to(STAGE)) for name in names if name != '.DS_Store')
    changes = []
    for name in sorted(paths):
        source, target = STAGE/name, ROOT/name
        require(source.is_file(), 'UNEXPECTED_STAGE_DELETION:' + name)
        digest = sha(source)
        if baseline.get(name) == digest:
            continue
        expected = baseline.get(name)
        if expected is None:
            require(not target.exists(), 'NEW_LM_PATH_ALREADY_EXISTS:' + name)
        else:
            require(target.is_file() and sha(target) == expected, 'USER_CHANGE_CONFLICT:' + name)
        changes.append(dict(path=name, baseline_sha256=expected, lm_sha256=digest, bytes=source.stat().st_size))
    for name in ['integration/test/contracts/pancake-lm-state.json', 'integration/test/contracts/pancake-lm-wire.json',
                 'integration/tools/acceptance/pancake_lm_smoke.py']:
        source, target = STAGE/name, ROOT/name
        require(source.is_file() and not target.exists(), 'NEW_LM_SUPPORT_PATH_CONFLICT:' + name)
        changes.append(dict(path=name, baseline_sha256=None, lm_sha256=sha(source), bytes=source.stat().st_size))
    require(changes, 'NO_LM_EDITS')
    return changes


def copy_source(source, temporary):
    # Cargo's freshness checks use source mtimes, including include! dependencies.
    # A staged file can predate an older cached wire crate despite new contents.
    shutil.copyfile(source, temporary)
    shutil.copymode(source, temporary)


def apply(changes, phase39):
    receipt = json.loads(phase39.read_text())
    require(receipt.get('passed') is True and receipt.get('source_tree_sha256') == BASE,
            'PHASE39_FROZEN_PASS_REQUIRED')
    for service in ['go','rust','router']:
        value = json.loads((OUT/('core-context-final-'+service+'.json')).read_text())
        require(value.get('passed') is True and value.get('observed_exit_code') == 0
                and value.get('source_tree_sha256_after') == BASE, 'PHASE39_COMPONENT_PASS_REQUIRED')
    # Recheck every planned change immediately before the reversible copy.
    require(plan() == changes, 'LM_PLAN_CHANGED')
    backup = OUT/('core-lm-integration-backup-'+secrets.token_hex(6))
    backup.mkdir()
    copied = []
    try:
        for entry in changes:
            name = entry['path']
            source, target = STAGE/name, ROOT/name
            if entry['baseline_sha256'] is not None:
                old = backup/name
                old.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(target, old)
            target.parent.mkdir(parents=True, exist_ok=True)
            temporary = target.with_name(target.name+'.lm-'+secrets.token_hex(6)+'.tmp')
            try:
                copy_source(source, temporary)
                temporary.replace(target)
            finally:
                temporary.unlink(missing_ok=True)
            copied.append(entry)
        require(source_hash() == EXPECTED and stage_hash() == EXPECTED, 'INTEGRATED_SOURCE_HASH_MISMATCH')
    except BaseException:
        for entry in reversed(copied):
            target = ROOT/entry['path']
            if target.exists() and sha(target) == entry['lm_sha256']:
                if entry['baseline_sha256'] is None:
                    target.unlink()
                else:
                    shutil.copy2(backup/entry['path'], target)
        raise
    report = dict(schema_version=1, passed=True, authority='exact baseline guarded local source integration; no commit/push/deployment',
                  source_tree_sha256=EXPECTED, previous_source_tree_sha256=BASE, baseline_manifest_sha256=sha(STAGE/'baseline-manifest.json'),
                  phase39_report=dict(path=str(phase39.relative_to(ROOT)), sha256=sha(phase39)), backup_directory=str(backup),
                  changes=changes, all_requested_features_complete=False, production_ready=False)
    write_result(OUT/'core-pancake-lm-integration.json', report)
    return report


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--apply', action='store_true')
    parser.add_argument('--phase39-report', type=Path)
    args = parser.parse_args()
    changes = plan()
    if args.apply:
        require(args.phase39_report is not None, 'PHASE39_REPORT_ARGUMENT_REQUIRED')
        result = apply(changes, args.phase39_report.resolve())
    else:
        result = dict(schema_version=1, dry_run=True, changes=changes, source_tree_sha256=EXPECTED,
                      all_requested_features_complete=False, production_ready=False)
        write_result(OUT/'core-pancake-lm-integration-plan.json', result)
    print(json.dumps(dict(dry_run=not args.apply, files=len(changes), source_tree_sha256=EXPECTED), indent=2))
