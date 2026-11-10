#!/usr/bin/env python3
"""Bounded real Agave validation. All build changes stay in the artifact tree."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tarfile
import time
import tomllib

HERE = Path(__file__).resolve().parent


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--agave-root', required=True, type=Path)
    p.add_argument('--output', required=True, type=Path)
    p.add_argument('--stage', choices=['check', 'build', 'test'], required=True)
    p.add_argument('--timeout', type=int, default=600)
    p.add_argument('--cargo', type=Path, default=Path.home()/'.rustup/toolchains/1.97.1-x86_64-apple-darwin/bin/cargo')
    p.add_argument('--jobs', type=int, default=2)
    p.add_argument('--libclang-dir', type=Path)
    args = p.parse_args()
    root = args.agave_root.resolve()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    pin = json.loads((HERE.parent/'source-pin.json').read_text())
    if sha(HERE.parent/'native-observer.patch') != pin['patch_sha256']:
        raise SystemExit('patch digest mismatch')
    archive = root.parent/'agave-v4.3.0.tar.gz'
    acquisition = json.loads((root.parent/'acquisition.json').read_text())
    if sha(archive) != acquisition['archive_sha256']:
        raise SystemExit('archive digest mismatch')
    original_lock = None
    with tarfile.open(archive) as t:
        for record in pin['files']:
            data = t.extractfile('agave-4.3.0/'+record['path']).read()
            if hashlib.sha256(data).hexdigest() != record['upstream_sha256']:
                raise SystemExit('non-pinned archive input: '+record['path'])
        original_lock = tomllib.loads(t.extractfile('agave-4.3.0/Cargo.lock').read().decode())
    # Verify the patched files are exactly generated from pinned inputs before
    # adding a test-only include. Never silently accept a modified upstream file.
    import tempfile
    with tempfile.TemporaryDirectory(prefix='xone-real-agave-pin-') as temp:
        clean = Path(temp)
        with tarfile.open(archive) as t:
            for record in pin['files']:
                path = clean/record['path']
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(t.extractfile('agave-4.3.0/'+record['path']).read())
        subprocess.run(['python3', str(HERE.parent/'apply.py'), '--agave-root', str(clean), '--apply'], check=True)
        include = '\n#[cfg(test)]\n#[path = "xone_real_bank_export.rs"]\nmod xone_real_bank_export;\n'
        for record in pin['files']:
            path = root/record['path']
            expected = (clean/record['path']).read_bytes()
            actual = path.read_bytes()
            if record['path'] == 'geyser-plugin-manager/src/accounts_update_notifier.rs':
                actual = actual.removesuffix(include.encode())
            if actual != expected:
                raise SystemExit('unexpected patched source: '+record['path'])
        target = root/'geyser-plugin-manager/src/accounts_update_notifier.rs'
        if not target.read_text().endswith(include):
            with target.open('a') as f:
                f.write(include)
    shutil.copyfile(HERE/'bank_export.rs', root/'geyser-plugin-manager/src/xone_real_bank_export.rs')
    packages = ['solana-runtime', 'solana-ledger', 'solana-geyser-plugin-manager']
    features = ','.join(f'{pkg}/agave-unstable-api' for pkg in packages)
    cmd = [str(args.cargo), args.stage, '--manifest-path', str(root/'Cargo.toml'), '-j', str(args.jobs)]
    if args.stage == 'test':
        cmd += ['-p', packages[-1], '--lib', '--features', features,
                'xone_real_bank_freeze_accounts_export', '--', '--exact',
                '--test-threads=1', '--nocapture']
        # Unit test identity includes the module path; exact filtering must run one test.
        cmd[cmd.index('xone_real_bank_freeze_accounts_export')] = (
            'accounts_update_notifier::xone_real_bank_export::xone_real_bank_freeze_accounts_export')
    else:
        cmd += ['--lib', '--features', features]
        for pkg in packages:
            cmd += ['-p', pkg]
    if any(x['name'] == 'xone-bank-source' for x in tomllib.loads((root/'Cargo.lock').read_text())['package']):
        cmd.insert(cmd.index('--') if '--' in cmd else len(cmd), '--locked')
    env = os.environ.copy()
    env['PATH'] = str(args.cargo.parent)+os.pathsep+env.get('PATH', '')
    env['RUSTC'] = str(args.cargo.parent/'rustc')
    env['RUSTDOC'] = str(args.cargo.parent/'rustdoc')
    if args.libclang_dir:
        clang_dir = args.libclang_dir.resolve()
        if not (clang_dir/'libclang.dylib').is_file():
            raise SystemExit('libclang.dylib missing from selected directory')
        env['LIBCLANG_PATH'] = str(clang_dir)
        env['DYLD_LIBRARY_PATH'] = str(clang_dir) + (os.pathsep+env['DYLD_LIBRARY_PATH'] if env.get('DYLD_LIBRARY_PATH') else '')
    env['CARGO_TARGET_DIR'] = str(root.parent/'native-validation/target')
    env['CARGO_PROFILE_DEV_DEBUG'] = '0'
    env['CARGO_PROFILE_TEST_DEBUG'] = '0'
    env['XONE_REAL_BANK_OUTPUT'] = str(output/'bank-evidence')
    env['XONE_BANK_SOURCE_CONFIG'] = str(output/'bank-evidence/source-config.json')
    record = {
        'command':cmd, 'cwd':str(root), 'timeout_seconds':args.timeout,
        'started_unix':time.time(), 'archive_sha256':sha(archive),
        'pin_sha256':sha(HERE.parent/'source-pin.json'),
        'patch_sha256':pin['patch_sha256'], 'harness_sha256':sha(HERE/'bank_export.rs'),
        'kernel_sha256':sha(HERE.parents[1]/'src/lib.rs'),
        'input_files':{r['path']:sha(root/r['path']) for r in pin['files']},
        'environment':{k:env[k] for k in ['CARGO_TARGET_DIR','CARGO_PROFILE_DEV_DEBUG', 'CARGO_PROFILE_TEST_DEBUG','XONE_REAL_BANK_OUTPUT', 'XONE_BANK_SOURCE_CONFIG', 'RUSTC', 'RUSTDOC']},
        'upstream_inputs_verified':True, 'timed_out':False,
        'cargo_net_offline':env.get('CARGO_NET_OFFLINE'),
        'libclang_dir':str(args.libclang_dir) if args.libclang_dir else None,
        'libclang_sha256':sha(args.libclang_dir/'libclang.dylib') if args.libclang_dir else None,
    }
    with (output/'command.json').open('w') as f:
        json.dump(record, f, indent=2)
    if not args.cargo.exists():
        record.update(exit_code=None, status='toolchain_unavailable', error=str(args.cargo))
    else:
        record['cargo_version'] = subprocess.check_output([str(args.cargo), '-V'], env=env, text=True).strip()
        record['rustc_version'] = subprocess.check_output([env['RUSTC'], '-vV'], env=env, text=True).strip()
        record['rustc_sha256'] = sha(Path(env['RUSTC']))
        record['cargo_home'] = env.get('CARGO_HOME')
        if not record['rustc_version'].startswith('rustc 1.97.1 '):
            raise SystemExit('Refusing compiler mismatch: '+record['rustc_version'])
        with (output/'cargo.log').open('w') as log:
            process = subprocess.Popen(cmd, cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
            try:
                record['exit_code'] = process.wait(timeout=args.timeout)
            except subprocess.TimeoutExpired:
                record['timed_out'] = True
                os.killpg(process.pid, signal.SIGTERM)
                try:
                    record['exit_code'] = process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    record['exit_code'] = process.wait()
        record['status'] = 'passed' if record['exit_code'] == 0 else 'failed'
        log_text = (output/'cargo.log').read_text(errors='replace')
        if args.stage == 'test' and record['status'] == 'passed':
            result = output/'bank-evidence/real-bank-result.json'
            if '1 passed; 0 failed' not in log_text or not result.exists():
                record['status'] = 'failed'
                record['error'] = 'successful cargo command did not prove the real Bank test ran'
            if 'xone native bank scope export refused' in log_text or 'xone native bank source initialization failed' in log_text:
                record['status'] = 'failed'
                record['error'] = 'real Bank execution reported a native exporter rejection'
    locked = tomllib.loads((root/'Cargo.lock').read_text())
    def identities(lock):
        return {(x['name'],x['version'],x.get('source'),x.get('checksum')) for x in lock['package'] if 'source' in x}
    record['removed_or_changed_upstream_lock_inputs'] = sorted(identities(original_lock)-identities(locked), key=str)
    if record['removed_or_changed_upstream_lock_inputs']:
        record['status'] = 'failed'
    record['kernel_sha256_after'] = sha(HERE.parents[1]/'src/lib.rs')
    record['kernel_changed_during_run'] = record['kernel_sha256_after'] != record['kernel_sha256']
    if record['status'] == 'passed' and record['kernel_changed_during_run']:
        record['status'] = 'inputs_changed'
        record['error'] = 'kernel changed during validation; rerun to bind one exact source version'
    config = Path(env['CARGO_HOME'])/'config.toml' if env.get('CARGO_HOME') else None
    record['cargo_config_sha256'] = sha(config) if config and config.exists() else None
    record['ended_unix'] = time.time()
    record['lock_sha256'] = sha(root/'Cargo.lock')
    (output/'result.json').write_text(json.dumps(record, indent=2)+'\n')
    print(json.dumps({k:record.get(k) for k in ['status','exit_code','timed_out','error']}, indent=2))
    return 0 if record['status'] == 'passed' else 1


if __name__ == '__main__':
    raise SystemExit(main())
