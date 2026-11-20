"""Drive one Go owner over historical OP heights and check each committed journal row."""
import argparse
from contextlib import ExitStack
import hashlib
import json
from pathlib import Path
import secrets
import shutil
import subprocess
import tempfile
import threading
import time
import urllib.error
import urllib.request

from current_v2_evidence import RecordedRPC, free_port, http, require, stop


class WindowRPC(RecordedRPC):
    """Resolve a moving head tag to a selected numeric height at each original endpoint."""
    methods = {'eth_chainId', 'eth_getBlockByNumber', 'eth_getBlockByHash', 'eth_getCode',
               'eth_getStorageAt', 'eth_getProof', 'eth_call', 'eth_getBlockReceipts',
               'eth_getTransactionReceipt', 'eth_getLogs', 'eth_getBalance'}

    def __init__(self, endpoint, selected, cache, max_bytes, cache_only):
        super().__init__([])
        self.endpoint = endpoint
        self.selected = selected
        self.cache = cache
        self.max_bytes = max_bytes
        self.cache_only = cache_only
        self.upstream = self.hits = 0
        self.errors = []

    def response(self, method, params):
        require(method in self.methods, 'OP_WINDOW_METHOD')
        if (self.selected and method == 'eth_getBlockByNumber'
                and params[0] in ('latest', 'safe', 'finalized')):
            params = [hex(self.selected()), *params[1:]]
        key = json.dumps([self.endpoint, method, params], sort_keys=True, separators=(',', ':'))
        path = self.cache / hashlib.sha256(key.encode()).hexdigest()
        if path.is_file():
            require(path.stat().st_size <= self.max_bytes, 'OP_WINDOW_RESPONSE_LIMIT')
            with self.lock:
                self.hits += 1
            return path.read_bytes()
        require(not self.cache_only, 'OP_WINDOW_CACHE_MISS_' + method)
        data = json.dumps(dict(id=1, jsonrpc='2.0', method=method, params=params)).encode()
        req = urllib.request.Request(self.endpoint, data, {'Content-Type': 'application/json'})
        with self.lock:
            self.upstream += 1
        for attempt in range(3):
            try:
                with urllib.request.urlopen(req, timeout=8) as response:
                    body = response.read(self.max_bytes + 1)
                break
            except urllib.error.HTTPError as exc:
                raise ValueError(f'OP_WINDOW_SOURCE_{method}_HTTP_{exc.code}') from None
            except OSError as exc:
                if attempt == 2:
                    raise ValueError(f'OP_WINDOW_SOURCE_{method}_{type(exc).__name__}') from None
                time.sleep(.4 * (attempt + 1))
        require(len(body) <= self.max_bytes, 'OP_WINDOW_RESPONSE_LIMIT')
        decoded = json.loads(body)
        if 'error' in decoded:
            with self.lock:
                self.errors.append([method, decoded['error'].get('code')])
        elif decoded.get('result') is not None:
            with tempfile.NamedTemporaryFile(dir=self.cache, delete=False) as tmp:
                tmp.write(body)
            Path(tmp.name).replace(path)
        return body


def run(config, beacon, spans, binary, out, first, last, max_age_ms, rpc_cache=None, cache_only=False):
    require(0 < first < last <= first + 512, 'OP_WINDOW_RANGE')
    cfg = json.loads(config.read_bytes())
    chain, = cfg['chains']
    require(chain['key'] == 'evm:10' and chain['op_l1_origin'] and chain['pools'], 'OP_WINDOW_OWNER')
    require(cfg['journal']['transport'] == 'http-development' and cfg['control']['backend'] == 'disabled',
            'OP_WINDOW_LOCAL_RELEASE')
    require(not cache_only or (rpc_cache and rpc_cache.is_dir()), 'OP_WINDOW_CACHE_REQUIRED')
    require(not out.exists(), 'OP_WINDOW_OUTPUT_EXISTS')
    out.mkdir(mode=0o700)
    expected = {}
    windows = []
    for file in spans:
        blocks = json.loads(file.read_bytes())['blocks']
        require(bool(blocks), 'OP_WINDOW_EMPTY_SPAN')
        for block in blocks:
            h = int(block['number'], 16)
            if expected:
                require(h == prev + 1 and expected[prev][0] == block['parentHash'], 'OP_WINDOW_INPUT_LINK')
            expected[h] = block['hash'], block['parentHash']
            prev = h
        lo, hi = max(first, int(blocks[0]['number'], 16)), min(last, prev)
        if lo <= hi:
            windows.append((file, lo, hi))
    require(first in expected and last in expected, 'OP_WINDOW_INPUT_RANGE')
    height = first
    height_lock = threading.Lock()

    def selected():
        with height_lock:
            return height

    cache = rpc_cache or out / 'rpc'
    cache.mkdir(exist_ok=True)
    active = out / 'active-span.json'
    shutil.copyfile(windows[0][0], active)
    cfg['startup']['max_baseline_age_ms'] = max_age_ms
    cfg['state']['path'] = str(out / 'state')
    policy = chain['op_l1_origin']
    policy.update(proof_file=str(beacon.resolve()), batch_proof_file=str(active))
    nodes = []
    with ExitStack() as stack:
        def proxy(endpoint, selector=selected):
            node = WindowRPC(endpoint, selector, cache, cfg['journal']['max_payload_bytes'], cache_only)
            addr = node.start()
            stack.callback(node.close)
            nodes.append(node)
            return addr
        chain['rpc_urls'] = [proxy(url) for url in chain['rpc_urls']]
        chain['proof_rpc_url'] = proxy(chain['proof_rpc_url'])
        policy['l1_rpc_url'] = proxy(policy['l1_rpc_url'], None)
        addr = f'127.0.0.1:{free_port()}'
        base = 'http://' + addr
        token = secrets.token_urlsafe(32)
        cfg['api'].update(listen=addr, auth_token=token)
        cfg['journal']['source_urls'] = [base]
        cfg_path = out / 'owner.json'
        cfg_path.touch(mode=0o600)
        cfg_path.write_text(json.dumps(cfg))
        log = stack.enter_context((out / 'owner.log').open('w'))
        proc = subprocess.Popen([binary, '--config', cfg_path], stdout=log, stderr=log, start_new_session=True)
        stack.callback(stop, proc)
        url = base + '/internal/chains/evm:10'
        deadline = time.monotonic() + 120
        while time.monotonic() < deadline and proc.poll() is None:
            snap = http(url + '/baseline', token)
            if snap['status'] == 200:
                break
            time.sleep(.2)
        else:
            raise ValueError('OP_WINDOW_STARTUP_FAILED')
        initial = snap['body']['reference']
        require(initial['block']['hash'] == expected[first][0] and initial['block']['number_or_slot'] == first,
                'OP_WINDOW_BASELINE')
        height_done, seq = first, initial['journal_seq']
        commits = []
        report = {'baseline': initial, 'requested_last': last, 'commits': commits}
        for n, (span, _, span_end) in enumerate(windows):
            if n:
                temp = out / 'next-span.json'
                shutil.copyfile(span, temp)
            while height_done < span_end:
                with height_lock:
                    if n and temp.exists():
                        temp.replace(active)
                    height = min(span_end, height_done + cfg['startup']['max_tail_blocks'])
                deadline = time.monotonic() + 120
                while height_done < height and proc.poll() is None and time.monotonic() < deadline:
                    reply = http(url + f'/journal?after={seq}', token)
                    require(reply['status'] == 200, 'OP_WINDOW_JOURNAL_HTTP')
                    for row in reply['body'] or []:
                        ref, block = row['reference'], row['reference']['block']
                        h = height_done + 1
                        require(row['kind'] == 'BLOCK_COMMIT' and h <= height and block['number_or_slot'] == h
                                and (block['hash'], block['parent_hash']) == expected[h]
                                and ref == dict(initial, block=block, journal_seq=seq + 1), 'OP_WINDOW_JOURNAL_GAP')
                        commits.append({'reference': ref, 'payload_hash': row['payload_hash']})
                        height_done, seq = h, seq + 1
                        deadline = time.monotonic() + 120
                    if reply['body']:
                        (out / 'progress.json').write_text(json.dumps(report))
                    if height_done < height:
                        time.sleep(.4)
                require(height_done == height, 'OP_WINDOW_COMMIT_TIMEOUT')
        snap = http(url + '/baseline', token)
        require(snap['status'] == 200 and snap['body']['reference'] == commits[-1]['reference'], 'OP_WINDOW_END')
        if cache_only:
            require(all(not node.trace for node in nodes), 'OP_WINDOW_INCOMPLETE_CACHE')
        report.update(cache_only=cache_only, max_age_ms=max_age_ms, endpoint_reads=[{'upstream': n.upstream, 'cached': n.hits, 'errors': n.errors} for n in nodes],
                      spans=len(windows), last=height_done, count=len(commits))
        (out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        return dict(first=first, last=height_done, commits=len(commits), spans=len(windows))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for arg in ('config', 'beacon', 'binary', 'out'):
        parser.add_argument('--' + arg, required=True, type=Path)
    for arg in ('first', 'last', 'max-age-ms'):
        parser.add_argument('--' + arg, required=True, type=int)
    parser.add_argument('--spans', nargs='+', required=True, type=Path)
    parser.add_argument('--rpc-cache', type=Path, help='reuse successful reads keyed by endpoint and numeric request')
    parser.add_argument('--cache-only', action='store_true', help='require all RPC reads in the endpoint-specific cache')
    args = parser.parse_args()
    print(json.dumps(run(args.config, args.beacon, args.spans, args.binary.resolve(), args.out.resolve(),
                         args.first, args.last, args.max_age_ms, args.rpc_cache, args.cache_only)))
