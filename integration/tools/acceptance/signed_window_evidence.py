"""Fresh signed Ethereum window over bounded retained, byte-exact RPC inputs.

Run the CLI in its own process. Concurrent library campaigns are rejected.
Seeds supply request shapes only; ordinary Go owners and the replay decoder
remain responsible for BLS, complete bodies/receipts and account/storage proofs.
"""
import argparse
from contextlib import contextmanager, nullcontext
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import tempfile
import threading
import time
from urllib.parse import urlsplit

import current_v2_evidence as current
import prime_signed_v2 as prime
import replay_beacon_window as replay
import signed_rpc_capture as capture_module
import signed_rpc_window as window
import verify_beacon_owner as owner

MODULES = (current, prime, replay, capture_module, window, owner)
RUN_LOCK = threading.Lock()
HASH = re.compile(r'0x[0-9a-f]{64}')
PROOF_BYTES = 16 << 20


def require(value, code):
    if not value:
        raise ValueError(code)


def compact(error):
    return prime._code(error, type(error).__name__.upper())


def file_sha(path):
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1 << 20), b''):
            digest.update(chunk)
    return digest.hexdigest()


def private_write(path, raw):
    temporary = path.with_name(path.name + '.tmp')
    descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(descriptor, 'wb') as stream:
        stream.write(raw)
    temporary.chmod(0o600)
    temporary.replace(path)


def read_json(path, maximum=PROOF_BYTES):
    path = Path(path)
    require(path.is_file() and not path.is_symlink(), 'SIGNED_EVIDENCE_REGULAR_FILE')
    with path.open('rb') as stream:
        raw = stream.read(maximum + 1)
    require(len(raw) <= maximum, 'SIGNED_EVIDENCE_FILE_BUDGET')
    return raw, json.loads(raw, object_pairs_hook=current.pairs)


class CampaignClock:
    """Never refund elapsed time after either suspension or a clock rollback."""
    def __init__(self, mono=time.monotonic, wall=time.time):
        self.mono, self.wall = mono, wall
        self.lock = threading.Lock()
        self.last_mono, self.last_wall = mono(), wall()
        self.value = self.last_mono

    def monotonic(self):
        with self.lock:
            mono, wall = self.mono(), self.wall()
            self.value += max(0, mono-self.last_mono, wall-self.last_wall)
            self.last_mono, self.last_wall = mono, wall
            return self.value

    def time(self):
        return self.wall()


class Runtime:
    """Scope helper clocks and only this campaign's actual child processes."""
    def __init__(self):
        self.clock = CampaignClock()
        self.collection = self.clock.monotonic() + 1800
        self.phase = None
        self.abort = threading.Event()
        self.stopped = threading.Event()
        self.lock = threading.Lock()
        self.reap_lock = threading.Lock()
        self.children = {}
        self.capture = None
        self.reason = None
        self.closing = False

    def remaining(self, maximum=1800):
        if self.reason:
            raise ValueError(self.reason)
        now = self.clock.monotonic()
        limits = [value-now for value in (self.collection, self.phase) if value is not None]
        remaining = min([maximum, *limits])
        require(remaining > 0, 'SIGNED_EVIDENCE_DEADLINE')
        return remaining

    @contextmanager
    def bounded(self, seconds):
        previous = self.phase
        candidate = self.clock.monotonic() + seconds
        self.phase = min(previous, candidate) if previous is not None else candidate
        try:
            self.remaining()
            yield
            self.remaining()
        finally:
            self.phase = previous

    def sleep(self, seconds):
        require(not self.abort.wait(min(seconds, self.remaining(seconds))), 'SIGNED_EVIDENCE_DEADLINE')
        self.remaining()

    def spawn(self, *args, **kwargs):
        with self.lock:
            self.remaining()
            require(not self.closing, 'SIGNED_EVIDENCE_CLOSED')
            child = subprocess.Popen(*args, **kwargs)
            self.children[child] = bool(kwargs.get('start_new_session'))
            return child

    def run(self, *args, capture_output=False, check=False, timeout=None, input=None, **kwargs):
        if capture_output:
            kwargs.update(stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        if input is not None:
            kwargs['stdin'] = subprocess.PIPE
        with self.spawn(*args, **kwargs) as child:
            try:
                stdout, stderr = child.communicate(input, timeout=timeout)
            except BaseException:
                self.stop_children([child])
                raise
            result = subprocess.CompletedProcess(child.args, child.returncode, stdout, stderr)
            if check:
                result.check_returncode()
            return result

    def stop_children(self, selected=None):
        with self.reap_lock:
            with self.lock:
                children = [(child, group) for child, group in self.children.items()
                            if selected is None or child in selected]
            def send(child, group, sig):
                if child.poll() is None:
                    try:
                        if group:
                            os.killpg(child.pid, sig)
                        else:
                            child.send_signal(sig)
                    except ProcessLookupError:
                        pass
                    except PermissionError:
                        # Darwin can reject signaling a process group whose
                        # last member is already exiting. Verify actual exit.
                        try:
                            child.wait(timeout=.1)
                        except subprocess.TimeoutExpired:
                            raise
            for child, group in children:
                send(child, group, signal.SIGTERM)
            deadline = time.monotonic() + 1
            for child, _ in children:
                try:
                    child.wait(timeout=max(0, deadline-time.monotonic()))
                except subprocess.TimeoutExpired:
                    pass
            for child, group in children:
                send(child, group, signal.SIGKILL)
            for child, _ in children:
                child.wait(timeout=2)

    def watch(self):
        while not self.stopped.wait(.05):
            try:
                self.remaining()
            except ValueError:
                self.reason = 'SIGNED_EVIDENCE_DEADLINE'
                self.abort.set()
                if self.capture is not None:
                    self.capture.cancel.set()
                try:
                    self.stop_children()
                except Exception:
                    # The main context retries cleanup and reports a compact
                    # failure; never print child argv/URLs from a daemon.
                    pass
                return
            with self.lock:
                self.children = {child: group for child, group in self.children.items() if child.poll() is None}

    def __enter__(self):
        require(RUN_LOCK.acquire(blocking=False), 'SIGNED_EVIDENCE_BUSY')
        self.saved = []
        runtime = self
        class TimeView:
            monotonic = staticmethod(runtime.clock.monotonic)
            time = staticmethod(runtime.clock.time)
            sleep = staticmethod(runtime.sleep)
        class Processes:
            Popen = staticmethod(runtime.spawn)
            run = staticmethod(runtime.run)
            def __getattr__(self, key):
                return getattr(subprocess, key)
        try:
            for module in MODULES:
                for name, replacement in [('time', TimeView), ('subprocess', Processes())]:
                    if hasattr(module, name):
                        self.saved.append((module, name, getattr(module, name)))
                        setattr(module, name, replacement)
            self.watcher = threading.Thread(target=self.watch, daemon=True)
            self.watcher.start()
            return self
        except BaseException:
            self.__exit__(None, None, None)
            raise

    def __exit__(self, *_):
        self.closing = True
        self.abort.set()
        try:
            if self.capture is not None:
                self.capture.cancel.set()
                self.capture.close()
        finally:
            self.stopped.set()
            try:
                self.stop_children()
                if hasattr(self, 'watcher'):
                    self.watcher.join(timeout=4)
                    require(not self.watcher.is_alive(), 'SIGNED_EVIDENCE_WATCHER_CLOSE')
            finally:
                for module, name, original in reversed(self.saved):
                    setattr(module, name, original)
                RUN_LOCK.release()


def https(url, beacon=False):
    require(isinstance(url, str) and not any(c.isspace() or ord(c) < 32 for c in url), 'SIGNED_EVIDENCE_HTTPS')
    parsed = urlsplit(url)
    require(parsed.scheme == 'https' and parsed.hostname and parsed.username is None
            and parsed.password is None and not parsed.fragment and (not beacon or not parsed.query),
            'SIGNED_EVIDENCE_HTTPS')


def proof_block(path):
    raw, proof = read_json(path)
    value = proof['finality']['data']['finalized_header']['execution']
    require(HASH.fullmatch(value['block_hash']) and HASH.fullmatch(value['parent_hash']), 'SIGNED_EVIDENCE_HASH')
    number, stamp = value['block_number'], value['timestamp']
    require(isinstance(number, str) and number.isascii() and number.isdigit()
            and isinstance(stamp, str) and stamp.isascii() and stamp.isdigit(), 'SIGNED_EVIDENCE_BLOCK')
    return raw, proof, dict(chain_key='evm:1', number_or_slot=int(number), hash=value['block_hash'],
                            parent_hash=value['parent_hash'], timestamp_ms=int(stamp)*1000)


def fresh(block, clock):
    require(0 <= int(clock.time()*1000)-block['timestamp_ms'] <= 3600000, 'SIGNED_EVIDENCE_ONE_HOUR')


def copy_templates(source, destination, upstreams, check=lambda: None):
    destination.mkdir(mode=0o700)
    if source is None:
        return dict(records=0, excluded=0, authority='REQUEST_SHAPE_ONLY')
    source = Path(source)
    require(source.is_dir() and not source.is_symlink(), 'SIGNED_EVIDENCE_TEMPLATE_DIRECTORY')
    reader = capture_module.SignedRPCCapture(destination, upstreams)
    copied = excluded = decoded = disk = scanned = 0
    for count, path in enumerate(source.iterdir(), 1):
        check()
        scanned += path.lstat().st_size
        require(count <= reader.MAX_RECORDS and scanned <= reader.MAX_DISK_BYTES, 'SIGNED_EVIDENCE_TEMPLATE_BUDGET')
        metadata, _, _ = reader._read_file(path)
        decoded += metadata['response_bytes']
        require(decoded <= reader.MAX_DECODED_BYTES, 'SIGNED_EVIDENCE_TEMPLATE_BUDGET')
        if metadata['provider_id'] not in reader.providers:
            excluded += 1
            continue
        maximum = reader.MAX_RESPONSE_BYTES + capture_module.META_BYTES + capture_module.GZIP_OVERHEAD + len(capture_module.MAGIC) + 4
        with path.open('rb') as stream:
            raw = stream.read(maximum + 1)
        disk += len(raw)
        require(len(raw) <= maximum and disk <= reader.MAX_DISK_BYTES,
                'SIGNED_EVIDENCE_TEMPLATE_BUDGET')
        private_write(destination/path.name, raw)
        copied += 1
    return dict(records=copied, excluded=excluded, authority='REQUEST_SHAPE_ONLY')


def refresh(runtime, binary, config, path, beacon, retries, after=None):
    with runtime.bounded(900):
        while True:
            before = read_json(path)[0] if after is not None or os.path.lexists(path) else None
            with runtime.bounded(70):
                reply = runtime.run([str(binary), '--config', str(config), '--refresh-beacon-proof', beacon],
                    capture_output=True, timeout=runtime.remaining(70), start_new_session=True)
            if reply.returncode:
                try:
                    error = json.loads(reply.stderr, object_pairs_hook=current.pairs)
                    code = error['fields']['error_code'] if error['event'] == 'service_start_failed' else None
                except (ValueError, KeyError, TypeError):
                    code = None
                require(isinstance(code, str), 'SIGNED_EVIDENCE_REFRESH_REJECTED')
                if code not in owner.RETRYABLE_REFRESH_ERRORS:
                    raise ValueError(compact(ValueError(code)))
                unchanged = not os.path.lexists(path) if before is None else read_json(path)[0] == before
                require(unchanged, 'SIGNED_EVIDENCE_REJECTED_PROOF_CHANGED')
                retries.append(code)
            else:
                raw, proof, block = proof_block(path)
                binding = json.loads(reply.stdout, object_pairs_hook=current.pairs)
                require(type(binding.get('execution_number')) is int
                        and binding['execution_number'] == block['number_or_slot']
                        and binding.get('execution_hash') == block['hash'], 'SIGNED_EVIDENCE_REFRESH_BINDING')
                require(after is None or block['number_or_slot'] >= after, 'SIGNED_EVIDENCE_REFRESH_REGRESSION')
                fresh(block, runtime.clock)
                if after is None or block['number_or_slot'] > after:
                    return raw, proof, block
            runtime.sleep(12)


def checked_stats(capture):
    stats = capture.stats()
    require(not stats['closed'] and not stats['storage_failed'] and stats['errors'] == 0,
            'SIGNED_EVIDENCE_CAPTURE_FAILED')
    return stats


def drain(runtime, capture, block):
    while checked_stats(capture)['pending']:
        fresh(block, runtime.clock)
        capture.wait_pending(timeout=runtime.remaining(30))
    runtime.remaining();fresh(block, runtime.clock)


def window_lineage(result, first, last):
    require(result['passed'] and result.get('journal_verified'), 'SIGNED_EVIDENCE_WINDOW_REJECTED')
    require(result.get('source_mode') == 'RETAINED_RECENT_SIGNED_RPC_BYTES'
            and result.get('production_ready') is False and result.get('consensus_verified') is False,
            'SIGNED_EVIDENCE_DISCLOSURE')
    cursor = result['first']
    require(cursor['block'] == first, 'SIGNED_EVIDENCE_FIRST_BINDING')
    gap = last['number_or_slot']-first['number_or_slot']
    require(1 <= gap <= 128 and len(result['commits']) == gap, 'SIGNED_EVIDENCE_GAP')
    for record in result['commits']:
        ref, block = record['reference'], record['reference']['block']
        require(record['kind'] == 'BLOCK_COMMIT' and block['chain_key'] == 'evm:1'
                and block['number_or_slot'] == cursor['block']['number_or_slot']+1
                and block['parent_hash'] == cursor['block']['hash']
                and ref == dict(cursor, block=block, journal_seq=cursor['journal_seq']+1), 'SIGNED_EVIDENCE_LINEAGE')
        cursor = ref
    require(cursor['block'] == last and result['target'] == dict(block_number=str(last['number_or_slot']),
            block_hash=last['hash']), 'SIGNED_EVIDENCE_LAST_BINDING')
    return gap


def missing_failure(error, directory, before, after):
    if (str(error) not in {'OWNER_BASELINE_STATUS_200', 'OWNER_JOURNAL_DISCONTINUITY', 'OWNER_JOURNAL_UNAVAILABLE'}
            or not (after['enqueued'] > before['enqueued'] or after['pending'])):
        return False
    path = directory/'owner.log'
    if not path.is_file():
        return False
    codes = []
    with path.open('rb') as stream:
        stream.seek(0, os.SEEK_END);stream.seek(max(0, stream.tell()-prime.LOG_TAIL_BYTES))
        for line in stream.read(prime.LOG_TAIL_BYTES).splitlines():
            try:
                event = json.loads(line, object_pairs_hook=current.pairs)
                if event.get('event') in {'service_start_failed', 'chain_start_failed', 'chain_sync_failed'}:
                    fields = event['fields'];code = fields['error_code']
                    if code != 'context canceled':
                        codes.append(code == 'RPC_ERROR_-32000' or
                            (code in {'RECEIPT_WITNESS_UNAVAILABLE', 'RECEIPT_WITNESS_INVALID'}
                             and fields.get('witness_error_class') == 'unavailable'))
            except (ValueError, KeyError, TypeError):
                continue
    return bool(codes) and all(codes)


def run_evidence(*, owner_binary, router_binary, replay_binary, output, checkpoint_root,
                 body_urls, proof_url, beacon_url, seed_proof=None, seed_config=None,
                 template_cache=None, template_block=None):
    require(len(body_urls) == 2 and len(set(body_urls)) == 2, 'SIGNED_EVIDENCE_BODY_SOURCES')
    for url in (*body_urls, proof_url):
        https(url)
    https(beacon_url, beacon=True)
    require(isinstance(checkpoint_root, str) and HASH.fullmatch(checkpoint_root), 'SIGNED_EVIDENCE_PIN')
    require((template_cache is None) == (template_block is None), 'SIGNED_EVIDENCE_TEMPLATE_PAIR')
    binaries = [Path(value).resolve() for value in (owner_binary, router_binary, replay_binary)]
    require(all(path.is_file() and os.access(path, os.X_OK) for path in binaries), 'SIGNED_EVIDENCE_BINARIES')
    output = Path(output).resolve()
    report = dict(passed=False, phase='STARTING', source_mode='RETAINED_RECENT_SIGNED_RPC_BYTES',
        started_ms=int(time.time()*1000), collection_budget_seconds=1800,
        production_ready=False, consensus_verified=False, public_transactions_submitted=False,
        provider_ids=[current.provider_id(url) for url in (*body_urls, proof_url)],
        proof_shared_with_body_indices=[i for i, url in enumerate(body_urls) if url == proof_url],
        distinct_upstream_urls=len(set((*body_urls, proof_url))), failure_domain_independence='UNKNOWN',
        attempts=[], refresh_retries=[])
    def save(phase=None):
        if phase:
            report['phase'] = phase
        private_write(output/'report.json', (json.dumps(report, indent=2)+'\n').encode())
    created = False
    @contextmanager
    def owned_runtime():
        try:
            with Runtime() as runtime:
                report['binary_sha256'] = dict(zip(('owner', 'router', 'replay'), map(file_sha, binaries)))
                report['driver_sha256'] = file_sha(Path(__file__))
                yield runtime
        except BaseException as error:
            if created and report['phase'] != 'FAILED':
                report.update(passed=False, failed_phase=report['phase'], error=compact(error))
                save('FAILED')
            raise
    # Own the module substitutions before creating output or touching inputs.
    with owned_runtime() as runtime:
        output.mkdir(mode=0o700, parents=True, exist_ok=False)
        output.chmod(0o700)
        created = True
        try:
            save()
            current_proof = output/'current-proof.json'
            config_source = seed_config or current.ROOT/'chain-parser-go/config/config-dev.yaml'
            config_raw, cfg = read_json(config_source, 1 << 20)
            require(len(cfg['chains']) == 1 and cfg['chains'][0]['key'] == 'evm:1'
                    and cfg['chains'][0]['genesis_hash'] == current.GENESIS, 'SIGNED_EVIDENCE_SEED_SCOPE')
            if seed_config is None:
                cfg['chains'][0]['beacon_finality'] = dict(checkpoint_root=checkpoint_root,
                    max_checkpoint_age_ms=7*86400000, proof_file=str(current_proof))
            require(cfg['chains'][0]['beacon_finality']['checkpoint_root'] == checkpoint_root,
                    'SIGNED_EVIDENCE_SEED_SCOPE')
            cfg['chains'][0].update(rpc_urls=list(body_urls), proof_rpc_url=proof_url)
            cfg['startup']['max_baseline_age_ms'] = 3600000
            cfg['journal']['request_timeout_ms'] = 60000
            cfg['startup']['max_tail_blocks'] = 128
            seed_sha = None
            if seed_proof is not None:
                seed_raw, _ = read_json(seed_proof)
                private_write(current_proof, seed_raw)
                seed_sha = current.sha(seed_raw)
            cfg['chains'][0]['beacon_finality']['proof_file'] = str(current_proof)
            config = output/'refresh-A-config.json'
            private_write(config, json.dumps(cfg).encode())
            report['seed_inputs'] = dict(proof_sha256=seed_sha, config_sha256=current.sha(config_raw),
                                        authority='REQUEST_SHAPE_ONLY')
            upstreams = (*body_urls, proof_url)
            report['templates'] = copy_templates(template_cache, output/'capture', upstreams, runtime.remaining)
            save('REFRESH_A')
            a_raw, a_proof, first = refresh(runtime, binaries[0], config, current_proof, beacon_url, report['refresh_retries'])
            proof_a, proof_b = output/'proof-A.json', output/'proof-B.json'
            private_write(proof_a, a_raw)
            parts = output/'A-inputs';parts.mkdir(mode=0o700)
            for key in ('bootstrap', 'updates', 'finality'):
                private_write(parts/(key+'.json'), json.dumps(a_proof.get(key, [])).encode())
            report['A'] = first
            template = read_json(template_block, 1 << 20)[1] if template_block is not None else first
            capture = capture_module.SignedRPCCapture(output/'capture', upstreams)
            runtime.capture = capture
            with capture:
                report['mapping'] = capture.mapping()
                save('PREFILL_A')
                report['identity_records'] = window.prefill_signed_identities(capture, current.GENESIS, runtime.remaining())
                report['prefill_A'] = window.prefill_signed_range(capture, template,
                    first['number_or_slot'], first['number_or_slot'], first['hash'],
                    budget_seconds=runtime.remaining(1200))
                with runtime.bounded(900):
                    for attempt in range(1, 97):
                        fresh(first, runtime.clock)
                        before = checked_stats(capture)
                        primed = prime.prime_current_bank(binaries[0], capture, proof_a, checkpoint_root,
                            output/f'prime-A-{attempt}', budget_seconds=runtime.remaining(900), max_attempts=1)
                        after = checked_stats(capture)
                        entry = primed['attempts'][-1] if primed['attempts'] else {}
                        report['attempts'].append(dict(kind='prime-A', attempt=attempt, code=entry.get('code', primed['code'])))
                        save('PRIME_A')
                        if primed['primed']:
                            require(entry.get('code') == 'PRIMED' and primed['binding']['execution_number'] == first['number_or_slot']
                                    and primed['binding']['execution_hash'] == first['hash']
                                    and read_json(Path(primed['proof_file']))[0] == a_raw, 'SIGNED_EVIDENCE_PRIME_BINDING')
                            break
                        require(attempt < 96 and entry.get('code') == 'RPC_ERROR_-32000'
                                and (after['enqueued'] > before['enqueued'] or after['pending']), 'SIGNED_EVIDENCE_PRIME_REJECTED')
                        drain(runtime, capture, first)
                    else:
                        raise ValueError('SIGNED_EVIDENCE_PRIME_ATTEMPTS')
                drain(runtime, capture, first)
                private_write(proof_b, a_raw)
                cfg['chains'][0]['beacon_finality']['proof_file'] = str(proof_b)
                config_b = output/'refresh-B-config.json'
                private_write(config_b, json.dumps(cfg).encode())
                save('WAIT_B')
                b_raw, _, last = refresh(runtime, binaries[0], config_b, proof_b, beacon_url,
                    report['refresh_retries'], after=first['number_or_slot'])
                gap = last['number_or_slot']-first['number_or_slot']
                require(1 <= gap <= 128, 'SIGNED_EVIDENCE_GAP')
                report.update(B=last, observed_gap=gap)
                fresh(first, runtime.clock)
                save('PREFILL_WINDOW')
                report['prefill_window'] = window.prefill_signed_range(capture, first,
                    first['number_or_slot']+1, last['number_or_slot'], last['hash'],
                    expected_parent=first['hash'], budget_seconds=runtime.remaining(1800))
                drain(runtime, capture, first)
                loopback = json.loads(json.dumps(cfg))
                loopback['chains'][0].update(rpc_urls=list(capture.endpoints[:2]), proof_rpc_url=capture.endpoints[2])
                loopback_config = output/'loopback-config.json'
                private_write(loopback_config, json.dumps(loopback).encode())
                fixed = {path: current.sha(path.read_bytes()) for path in [proof_a, proof_b,
                    *(parts/(key+'.json') for key in ('bootstrap', 'updates', 'finality'))]}
                def check_fixed():
                    runtime.remaining();fresh(first, runtime.clock)
                    require(all(current.sha(path.read_bytes()) == digest for path, digest in fixed.items()), 'SIGNED_EVIDENCE_PROOF_CHANGED')
                def ordinary(directory):
                    check_fixed()
                    with runtime.bounded(750):  # Existing baseline150 + commit-window600 stay unchanged.
                        result = owner.exercise(binaries[0], directory, parts/'bootstrap.json', parts/'updates.json',
                            parts/'finality.json', checkpoint_root, next_proof=proof_b,
                            execution_config=loopback_config, retained_signed=True)
                    window_lineage(result, first, last)
                    check_fixed()
                    return result
                save('OWNER_WINDOW')
                for attempt in range(1, 5):
                    before = checked_stats(capture)
                    directory = output/f'window-{attempt}'
                    try:
                        ordinary(directory)
                        report['attempts'].append(dict(kind='window', attempt=attempt, passed=True))
                        break
                    except (ValueError, OSError, subprocess.SubprocessError) as error:
                        after = checked_stats(capture)
                        report['attempts'].append(dict(kind='window', attempt=attempt, passed=False,
                            code=compact(error), new_enqueued=after['enqueued']-before['enqueued'], pending=after['pending']))
                        save('WINDOW_REJECTED')
                        require(attempt < 4 and missing_failure(error, directory, before, after), 'SIGNED_EVIDENCE_WINDOW_REJECTED')
                        drain(runtime, capture, first)
                drain(runtime, capture, first)
                capture.freeze()
                frozen = checked_stats(capture)
                require(frozen['pending'] == 0 and frozen['frozen'], 'SIGNED_EVIDENCE_NOT_FROZEN')
                report['collection_elapsed_seconds'] = round(1800-runtime.remaining(), 3)
                runtime.collection = None  # Acquisition is over; do not squeeze verification into1800.
                report['frozen_capture'] = frozen
                save('FROZEN_WINDOW')
                final_directory = output/'frozen-window'
                final = ordinary(final_directory)
                save('FROZEN_ROUTERS')
                with runtime.bounded(120):
                    quoted = current.run_processes(output/'B-routers', binaries[0], binaries[1], capture.endpoints[:2],
                        live=False, replica=True, signed_finality=dict(checkpoint_root=checkpoint_root,
                            proof_file=str(proof_b), max_checkpoint_age_ms=7*86400000), proof_endpoint=capture.endpoints[2])
                baseline = quoted['baseline']['body'];ref = baseline['reference'];pool = baseline['pools'][0]
                require(ref['block'] == last and len(quoted['quotes']) == 4, 'SIGNED_EVIDENCE_QUOTE_BINDING')
                current.check_source_binding(quoted['source_evidence']['body'], ref,
                    dict(execution_number=last['number_or_slot'], execution_hash=last['hash']))
                require(quoted['signed_finality']['proof_sha256'] == current.sha(b_raw), 'SIGNED_EVIDENCE_QUOTE_PROOF')
                require(not (output/'B-routers/router/state').exists()
                        and not (output/'B-routers/replica/state').exists(), 'SIGNED_EVIDENCE_ROUTER_LOCAL_KV')
                for row in quoted['quotes']:
                    current.check_quote(pool, ref, row['request'], row['response'])
                request = quoted['quotes'][0]['request']
                for key in ('healthy_peer_quote', 'cold_restarted_quote'):
                    current.check_quote(pool, ref, request, quoted['replica_recovery'][key])
                check_fixed()
                save('RAW_REPLAY')
                with runtime.bounded(625):
                    decoded = replay.replay(binaries[2], final_directory, checkpoint_root, retained_signed=True)
                original = read_json(final_directory/'report.json', 16 << 20)[1]
                window_lineage(original, first, last)
                endpoints = owner.execution_endpoints(final_directory/'owner-config.json', retained_signed=True)
                _, staged = replay.read_window(final_directory/'owner/state/raw-evidence/evm-1', endpoints)
                replay.check_rows(decoded['verified_blocks'], original, staged)
                require(decoded['matched_staged_records'] == gap and len(decoded['verified_blocks']) == gap+1,
                        'SIGNED_EVIDENCE_REPLAY_COUNT')
                after = checked_stats(capture)
                require(after['fetches'] == frozen['fetches'] and after['enqueued'] == frozen['enqueued'], 'SIGNED_EVIDENCE_FROZEN_FETCH')
                check_fixed()
                private_write(output/'replay-report.json', (json.dumps(decoded, indent=2)+'\n').encode())
                report.update(passed=True, verified_commits=len(final['commits']), matched_staged_records=gap,
                    frozen_upstream_fetches=0, final_capture=after,
                    fixed_proof_sha256={str(path.relative_to(output)): digest for path, digest in fixed.items()})
            report['capture_closed'] = capture.closed
            save('COMPLETE')
        except BaseException as error:
            report.update(passed=False, failed_phase=report['phase'], error=runtime.reason or compact(error),
                          error_detail_sha256=current.sha(str(error).encode()))
            if runtime.capture is not None:
                report['capture_failure_stats'] = runtime.capture.stats()
            save('FAILED')
            raise
    return report


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--owner-binary', type=Path, default=current.ROOT/'chain-parser-go/bin/chain-parser-go')
    router_target = current.ROOT/'dex-router'/os.environ.get('CARGO_TARGET_DIR', 'target')
    parser.add_argument('--router-binary', type=Path, default=router_target/'debug/dex-router')
    parser.add_argument('--replay-binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, help='retain evidence in this new directory (default: temporary)')
    parser.add_argument('--checkpoint-root', required=True)
    parser.add_argument('--body-urls', nargs=2)
    parser.add_argument('--proof-url')
    parser.add_argument('--beacon-url', default='https://ethereum-beacon-api.publicnode.com')
    parser.add_argument('--seed-proof', type=Path)
    parser.add_argument('--seed-config', type=Path)
    parser.add_argument('--template-cache', type=Path)
    parser.add_argument('--template-block', type=Path)
    args = parser.parse_args(argv)
    keep = args.output is not None
    directory = nullcontext(None) if keep else tempfile.TemporaryDirectory(prefix='xone-signed-')
    with directory as scratch:
        if not keep:
            args.output = Path(scratch)/'window'
        new_output = not os.path.lexists(args.output)
        try:
            if args.body_urls is None or args.proof_url is None:
                _, dev = read_json(current.ROOT/'chain-parser-go/config/config-dev.yaml', 1 << 20)
                chain, = dev['chains']
                args.body_urls = args.body_urls or chain['rpc_urls']
                args.proof_url = args.proof_url or chain['proof_rpc_url']
            result = run_evidence(**vars(args))
        except (Exception, KeyboardInterrupt) as error:
            summary = dict(passed=False, error=compact(error))
            report = args.output/'report.json'
            if new_output and report.is_file():
                try:
                    _, details = read_json(report)
                    if details.get('phase') == 'FAILED':
                        summary['failed_phase'] = details.get('failed_phase')
                        summary['error'] = details['error']
                        summary['capture_errors'] = details.get('capture_failure_stats', {}).get('error_codes', {})
                except (OSError, ValueError, TypeError, KeyError):
                    pass
                if keep:
                    summary['report'] = str(report)
            print(json.dumps(summary), file=sys.stderr)
            return 130 if isinstance(error, KeyboardInterrupt) else 1
        fields = ('passed', 'source_mode', 'A', 'B', 'observed_gap', 'verified_commits',
                  'matched_staged_records', 'frozen_upstream_fetches', 'production_ready', 'consensus_verified')
        print(json.dumps({key: result[key] for key in fields}, indent=2))
        return 0


if __name__ == '__main__':
    def terminate(signum, frame):
        raise KeyboardInterrupt
    signal.signal(signal.SIGTERM, terminate)
    raise SystemExit(main())
