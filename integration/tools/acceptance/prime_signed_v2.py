"""Collect missing signed-bank inputs using disposable ordinary Go owners.

The caller owns the capture lifecycle. Only a matching HTTP baseline from the
normal owner can mark this run primed; collecting bytes grants no readiness.
"""
import copy
import hashlib
import json
import math
import os
from pathlib import Path
import re
import secrets
import subprocess
import tempfile
import time
import urllib.error
import urllib.request

from current_v2_evidence import ROOT, config_for, free_port, pairs, source_configuration


PROOF_BYTES = 16 << 20
LOG_TAIL_BYTES = 64 << 10
RAW_LIMITS = dict(max_bytes=512 << 20, max_records=10000, max_record_bytes=128 << 20)


def _code(error, fallback):
    value = getattr(error, 'code', str(error))
    return value if isinstance(value, str) and re.fullmatch(r'[A-Z0-9_:-]{1,160}', value) else fallback


def _private_bytes(path, raw):
    with path.open('xb') as stream:
        path.chmod(0o600)
        stream.write(raw)


def _stats(capture):
    value = capture.stats()
    for key in ('pending', 'misses', 'enqueued'):
        if type(value.get(key)) is not int or value[key] < 0:
            raise ValueError('SIGNED_PRIME_CAPTURE_STATS')
    if value.get('closed'):
        raise ValueError('CAPTURE_CLOSED')
    if value.get('storage_failed'):
        raise ValueError('CAPTURE_STORAGE_FAILED')
    return value


def _check_proof(path, digest):
    if path.is_symlink() or not path.is_file():
        raise ValueError('SIGNED_PRIME_PROOF_CHANGED')
    with path.open('rb') as stream:
        raw = stream.read(PROOF_BYTES + 1)
    if len(raw) > PROOF_BYTES or hashlib.sha256(raw).hexdigest() != digest:
        raise ValueError('SIGNED_PRIME_PROOF_CHANGED')


def http(url, token, timeout):
    request = urllib.request.Request(url, headers={'Authorization': 'Bearer ' + token})
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            raw = response.read(PROOF_BYTES + 1)
            if len(raw) > PROOF_BYTES:
                raise ValueError('SIGNED_PRIME_BASELINE_BUDGET')
            return {'status': response.status, 'body': json.loads(raw, object_pairs_hook=pairs)}
    except urllib.error.HTTPError as error:
        return {'status': error.code}
    except OSError:
        return {'status': None}


def _failure_code(path, returncode):
    with path.open('rb') as stream:
        stream.seek(0, os.SEEK_END)
        stream.seek(max(0, stream.tell() - LOG_TAIL_BYTES))
        tail = stream.read(LOG_TAIL_BYTES)
    for line in reversed(tail.splitlines()):
        try:
            event = json.loads(line, object_pairs_hook=pairs)
            if event.get('event') == 'service_start_failed':
                return _code(event['fields']['error_code'], 'SIGNED_PRIME_CHILD_FAILED')
        except (ValueError, KeyError, TypeError, AttributeError):
            pass
    return 'SIGNED_PRIME_CHILD_EXIT_' + str(returncode)


def _stop_child(child, deadline):
    deadline = min(deadline, time.monotonic() + 3)
    if child.poll() is not None:
        child.wait(timeout=max(0, deadline - time.monotonic()))
        return
    try:
        child.terminate()
    except ProcessLookupError:
        child.wait(timeout=max(0, deadline - time.monotonic()))
        return
    try:
        child.wait(timeout=min(2, max(0, deadline - time.monotonic()) / 2))
    except subprocess.TimeoutExpired:
        child.kill()
        try:
            child.wait(timeout=max(0, deadline - time.monotonic()))
        except subprocess.TimeoutExpired:
            raise ValueError('SIGNED_PRIME_CHILD_STOP_TIMEOUT') from None


def _matching_baseline(reply, binding):
    try:
        block = reply['body']['reference']['block']
        return (block['chain_key'] == 'evm:1' and type(block['number_or_slot']) is int
                and block['number_or_slot'] == binding['execution_number']
                and block['hash'] == binding['execution_hash'])
    except (KeyError, TypeError):
        return False


def prime_current_bank(owner_binary, capture, proof_file, checkpoint_root, output,
                       budget_seconds=900, max_attempts=96):
    """Return a compact result after ordinary owner admission or a bounded failure."""
    if (type(budget_seconds) not in (int, float) or not math.isfinite(budget_seconds)
            or not 0 < budget_seconds <= 900 or type(max_attempts) is not int
            or not 1 <= max_attempts <= 96):
        raise ValueError('SIGNED_PRIME_LIMITS')
    started = time.monotonic()
    deadline = started + budget_seconds
    # Reserve time to terminate and reap the last child inside the total budget.
    work_deadline = deadline - min(3, budget_seconds / 4)
    original = Path(proof_file)
    if original.is_symlink() or not original.is_file() or original.stat().st_size > PROOF_BYTES:
        raise ValueError('SIGNED_PROOF_FILE')
    with original.open('rb') as stream:
        proof = stream.read(PROOF_BYTES + 1)
    if len(proof) > PROOF_BYTES:
        raise ValueError('SIGNED_PROOF_FILE')
    output = Path(output).resolve()
    output.mkdir(mode=0o700, parents=True, exist_ok=False)
    output.chmod(0o700)
    fixed_proof = output / 'signed-proof.json'
    _private_bytes(fixed_proof, proof)
    report = dict(primed=False, code='SIGNED_PRIME_NOT_STARTED', attempts=[],
                  proof_file=str(fixed_proof), binding=None)
    try:
        endpoints = tuple(capture.endpoints)
        if len(endpoints) != 3:
            raise ValueError('SIGNED_PRIME_SOURCE_COUNT')
        try:
            chain, binding, _ = source_configuration(endpoints[:2], live=False,
                signed_finality=dict(checkpoint_root=checkpoint_root, proof_file=str(fixed_proof),
                                     max_checkpoint_age_ms=7 * 86400000),
                proof_endpoint=endpoints[2])
        except (KeyError, TypeError):
            raise ValueError('SIGNED_PRIME_PROOF_ENCODING') from None
        report['binding'] = binding
        for attempt in range(1, max_attempts + 1):
            if time.monotonic() >= work_deadline:
                report['code'] = 'SIGNED_PRIME_TIMEOUT'
                break
            _check_proof(fixed_proof, binding['proof_sha256'])
            before = _stats(capture)
            token, port = secrets.token_urlsafe(32), free_port()
            owner_url = 'http://127.0.0.1:' + str(port)
            matched = False
            code = 'SIGNED_PRIME_TIMEOUT'
            with tempfile.TemporaryDirectory(prefix='owner-attempt-', dir=output) as temporary:
                directory = Path(temporary)
                directory.chmod(0o700)
                cfg = config_for('chain-parser-go', directory / 'owner', token, port)
                cfg['chains'] = [copy.deepcopy(chain)]
                cfg['history']['enabled'] = False
                cfg['startup']['max_baseline_age_ms'] = 3600000
                cfg['journal'].update(transport='http-development', source_urls=[owner_url],
                    poll_interval_ms=60000, request_timeout_ms=60000, max_payload_bytes=16 << 20,
                    raw_evidence=copy.deepcopy(RAW_LIMITS))
                config = directory / 'owner-config.json'
                _private_bytes(config, json.dumps(cfg).encode())
                log_path = directory / 'owner.log'
                with log_path.open('xb') as log:
                    log_path.chmod(0o600)
                    child = subprocess.Popen([str(Path(owner_binary).resolve()), '--config', str(config)],
                        cwd=ROOT / 'chain-parser-go', stdout=log, stderr=subprocess.STDOUT,
                        start_new_session=True)
                    entry = dict(attempt=attempt, code='SIGNED_PRIME_RUNNING', pending=before['pending'])
                    report['attempts'].append(entry)
                    try:
                        while time.monotonic() < work_deadline:
                            returncode = child.poll()
                            if returncode is not None:
                                log.flush()
                                code = _failure_code(log_path, returncode)
                                break
                            remaining = work_deadline - time.monotonic()
                            if remaining <= 0:
                                break
                            reply = http(owner_url + '/internal/chains/evm:1/baseline', token,
                                         timeout=min(3, remaining))
                            if time.monotonic() >= work_deadline:
                                break
                            _stats(capture)
                            if reply['status'] == 200:
                                if child.poll() is not None:
                                    code = 'SIGNED_PRIME_CHILD_EXITED'
                                elif not _matching_baseline(reply, binding):
                                    code = 'SIGNED_PRIME_EXECUTION_BINDING'
                                else:
                                    matched = True
                                    code = 'SIGNED_PRIME_BASELINE_MATCHED'
                                break
                            time.sleep(min(.2, max(0, work_deadline - time.monotonic())))
                    finally:
                        _stop_child(child, deadline)
            after = _stats(capture)
            entry.update(code=code, pending=after['pending'])
            report['code'] = code
            if matched:
                if time.monotonic() >= deadline:
                    report['code'] = 'SIGNED_PRIME_TIMEOUT'
                    break
                _check_proof(fixed_proof, binding['proof_sha256'])
                if time.monotonic() >= deadline:
                    report['code'] = 'SIGNED_PRIME_TIMEOUT'
                    break
                entry['code'] = report['code'] = 'PRIMED'
                report['primed'] = True
                break
            if time.monotonic() >= work_deadline:
                report['code'] = 'SIGNED_PRIME_TIMEOUT'
                break
            # A wrong published binding is an actual owner failure, regardless
            # of unrelated capture work that may still be running.
            if code == 'SIGNED_PRIME_EXECUTION_BINDING':
                break
            if after['enqueued'] < before['enqueued']:
                raise ValueError('SIGNED_PRIME_CAPTURE_COUNTER')
            if not before['pending'] and not after['pending'] and after['enqueued'] == before['enqueued']:
                break
            if attempt == max_attempts:
                report['code'] = 'SIGNED_PRIME_ATTEMPTS'
                break
            while after['pending']:
                remaining = work_deadline - time.monotonic()
                if remaining <= 0:
                    break
                capture.wait_pending(timeout=min(30, remaining))
                after = _stats(capture)
            if time.monotonic() >= work_deadline:
                report['code'] = 'SIGNED_PRIME_TIMEOUT'
                break
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        report['code'] = _code(error, 'SIGNED_PRIME_FAILED')
    except BaseException:
        report['code'] = 'SIGNED_PRIME_CANCELLED'
        raise
    finally:
        if report['attempts'] and report['attempts'][-1]['code'] in (
                'SIGNED_PRIME_RUNNING', 'SIGNED_PRIME_BASELINE_MATCHED'):
            entry = report['attempts'][-1]
            entry['code'] = report['code']
            try:
                pending = capture.stats()['pending']
                if type(pending) is int and pending >= 0:
                    entry['pending'] = pending
            except Exception:
                pass
        _private_bytes(output / 'prime-report.json', (json.dumps(report, indent=2) + '\n').encode())
    return report
