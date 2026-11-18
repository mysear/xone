#!/usr/bin/env python3
"""Validate current product CLI and replay retained public reads without RPC."""
import copy
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import subprocess
import sys
import threading
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'integration/tools/market'))
from capture_chainlink import collect, file_digest, load, require, write
from market_feed_vectors import digest


def main(binary, vectors, output, captures):
    binary, vectors, output = Path(binary).resolve(), Path(vectors), Path(output).resolve()
    output.mkdir(parents=True, exist_ok=False)
    corpus = load(vectors.read_bytes())
    cli_cases = []
    for i, row in enumerate(corpus['synthetic'] + corpus['public']):
        c = row['candidate']
        r = subprocess.run([str(binary)], input=json.dumps(c).encode(), capture_output=True, timeout=10)
        (output / f'cli-{i}.stdout').write_bytes(r.stdout)
        (output / f'cli-{i}.stderr').write_bytes(r.stderr)
        if row['expected']['overflow']:
            require(r.returncode != 0 and load(r.stderr)['error'] == 'AMOUNT_OVERFLOW', 'CLI_RATIO_OVERFLOW')
        else:
            require(r.returncode == 0, 'CLI_VALID_CANDIDATE_REJECTED')
            u = load(r.stdout)
            require(u['observation'] == row['signed']['observation'], 'CLI_INDEPENDENT_OBSERVATION')
            expected_message = ('xone-native-token-price/1:' + digest(u['observation'])).encode().hex()
            require(u['signing_message_hex'] == expected_message and u['authorized'] is False,
                    'CLI_SIGNING_MESSAGE_OR_AUTHORITY')
        cli_cases.append(dict(id=i, overflow=row['expected']['overflow'], exit_code=r.returncode))
    replays = []
    for i, source in enumerate(captures):
        source = Path(source)
        index = load((source / 'rpc/index.json').read_bytes())
        c = load((source / 'candidate.json').read_bytes())
        replies = {}
        for row in index['records']:
            request = row['request']
            raw = (source / 'rpc' / row['response_file']).read_bytes()
            require(file_digest(source / 'rpc' / row['response_file']) == row['response_sha256'], 'RAW_PUBLIC_RPC_CHANGED')
            key = json.dumps([request['method'], request['params']], sort_keys=True, separators=(',', ':'))
            require(key not in replies, 'DUPLICATE_REPLAY_REQUEST')
            replies[key] = raw
        seen, faults = [], []

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def do_POST(self):
                try:
                    n = int(self.headers['Content-Length'])
                    require(0 < n <= 65536, 'REPLAY_REQUEST_BYTES')
                    request = load(self.rfile.read(n))
                    key = json.dumps([request['method'], request['params']], sort_keys=True, separators=(',', ':'))
                    require(key in replies, 'UNRECORDED_MARKET_RPC')
                    raw = replies[key]
                    require(load(raw)['id'] == request['id'], 'REPLAY_RPC_ID')
                    seen.append(key)
                except Exception as e:
                    faults.append(type(e).__name__)
                    raw = b'{"jsonrpc":"2.0","id":0,"error":{"code":-32000}}'
                self.send_response(200)
                self.send_header('Content-Length', str(len(raw)))
                self.end_headers()
                self.wfile.write(raw)

        server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        try:
            with patch('capture_chainlink.time.time', return_value=c['market_feed']['source_read_at_ms']/1000):
                report = collect(source / 'policy.json', 'http://127.0.0.1:' + str(server.server_port),
                                 binary, output / f'replay-{i}', c['anchor']['block_hash'], allow_loopback=True)
            require(report['passed'] and not faults and len(seen) == len(replies), 'RECORDED_MARKET_REPLAY_COVERAGE')
            unsigned = load((output / f'replay-{i}/unsigned.json').read_bytes())['observation']
            original = load((source / 'unsigned.json').read_bytes())['observation']
            expected = copy.deepcopy(original)
            expected['market_feed']['provider_id'] = unsigned['market_feed']['provider_id']
            require(unsigned == expected, 'MARKET_REPLAY_CHANGED_CAPTURED_PRICE')
            replays.append(dict(capture_report_sha256=file_digest(source / 'report.json'),
                                rpc_reads=len(seen), anchor=report['anchor']))
        finally:
            server.shutdown()
            server.server_close()
    write(output / 'report.json', dict(passed=True, cli_cases=cli_cases, public_replays=replays,
          vectors_sha256=file_digest(vectors), validator_sha256=file_digest(binary),
          public_rpc_reads_during_replay=0, historical_capture_time_preserved=True,
          signers_are_public_test_keys=True, production_ready=False, source_independence_attested=False,
          artifacts={str(p.relative_to(output)): file_digest(p) for p in sorted(output.rglob('*')) if p.is_file()}))


if __name__ == '__main__':
    main(sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4:])
