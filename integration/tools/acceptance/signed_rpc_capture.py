"""Bounded asynchronous acquisition of exact read-only JSON-RPC response bytes.

This relay grants no readiness or consensus authority. Ordinary owners still
validate the retained bodies, receipts, state proofs and signed finality.
"""
from collections import Counter
from contextlib import suppress
from email.utils import parsedate_to_datetime
import copy
import gzip
import hashlib
from http.server import BaseHTTPRequestHandler, HTTPServer
import io
import json
import math
import os
from pathlib import Path
import queue
import re
import selectors
import signal
import struct
import stat
import subprocess
import threading
import time
from urllib.parse import urlsplit
import uuid
import zlib

from current_v2_evidence import METHODS, canonical, pairs, provider_id


READ_METHODS = METHODS | {'eth_getRawTransactionByHash'}
MAGIC = b'XONE_RPC_CAPTURE_V1\n'
REQUEST_BYTES = 64 << 10
RESPONSE_BYTES = 16 << 20
STDERR_BYTES = 64 << 10
FETCH_SECONDS = 120
META_BYTES = 256 << 10
GZIP_OVERHEAD = 8192


def digest(value):
    return hashlib.sha256(value).hexdigest()


class CaptureError(ValueError):
    def __init__(self, code, detail_hash=None):
        super().__init__(code)
        self.code = code
        self.detail_hash = detail_hash


def fail(condition, code):
    if not condition:
        raise CaptureError(code)


def strict_json(raw):
    def non_json(_):
        raise CaptureError('CAPTURE_JSON_ENCODING')
    value = json.loads(raw, object_pairs_hook=pairs, parse_constant=non_json)
    def finite(item):
        if isinstance(item, float):
            fail(math.isfinite(item), 'CAPTURE_JSON_ENCODING')
        elif isinstance(item, list):
            for child in item:
                finite(child)
        elif isinstance(item, dict):
            for child in item.values():
                finite(child)
    finite(value)
    return value


def rpc_error(code):
    return json.dumps({'jsonrpc': '2.0', 'id': 1,
                       'error': {'code': -32000, 'message': code}},
                      separators=(',', ':')).encode()


def _stop_child(child):
    if child.poll() is None:
        with suppress(ProcessLookupError):
            os.killpg(child.pid, signal.SIGTERM)
        try:
            child.wait(timeout=.5)
        except subprocess.TimeoutExpired:
            with suppress(ProcessLookupError):
                os.killpg(child.pid, signal.SIGKILL)
            child.wait(timeout=1)


class ProviderPace:
    """Space starts at one host, including retries and different RPC roles."""
    def __init__(self, interval=1.0):
        self.interval = interval
        self.lock = threading.Lock()
        self.next_start = 0.0

    def acquire(self, cancel, deadline, expires_at):
        while True:
            fail(not cancel.is_set(), 'CAPTURE_CANCELLED')
            remaining = _fetch_remaining(deadline, expires_at)
            fail(remaining > 0, 'CAPTURE_FETCH_TIMEOUT')
            with self.lock:
                now = time.monotonic()
                delay = self.next_start - now
                if delay <= 0:
                    self.next_start = now + self.interval
                    return
            fail(not cancel.wait(min(delay, remaining)), 'CAPTURE_CANCELLED')

    def defer(self, seconds):
        with self.lock:
            self.next_start = max(self.next_start, time.monotonic() + seconds)


class ProviderBackoff(CaptureError):
    def __init__(self, detail_hash, status, retry_after):
        super().__init__('CAPTURE_HTTP_' + str(status), detail_hash)
        self.retry_after = retry_after


def _retry_seconds(value):
    if value.isdigit():
        # A server may specify a delay beyond the entire collection campaign.
        digits = value.lstrip(b'0')
        return int(digits or b'0') if len(digits) <= 10 else math.inf
    try:
        date = parsedate_to_datetime(value.decode('ascii'))
        if date.tzinfo is not None:
            return max(0, date.timestamp() - time.time())
    except (ValueError, TypeError, UnicodeError, OverflowError, OSError):
        pass
    return 0


def curl_fetch(url, method, params, cancel, pace=None):
    """Retry transport failures within one wall/byte budget; retain exact bytes."""
    request = json.dumps({'jsonrpc': '2.0', 'id': 1, 'method': method,
                          'params': params}, separators=(',', ':')).encode()
    fail(len(request) <= REQUEST_BYTES, 'CAPTURE_REQUEST_BUDGET')
    deadline = time.monotonic() + FETCH_SECONDS
    expires_at = time.time() + FETCH_SECONDS
    # Failed partial responses also consume the original budgets. Each attempt
    # owns a new output buffer, so no failed bytes can prefix a later response.
    budget = [RESPONSE_BYTES, STDERR_BYTES]
    if pace is None:
        pace = ProviderPace(interval=0)
    for attempt in range(3):
        pace.acquire(cancel, deadline, expires_at)
        try:
            return _curl_fetch_attempt(url, request, cancel, deadline, budget, expires_at)
        except ProviderBackoff as error:
            pace.defer(max(error.retry_after, 8 * (2 ** attempt)))
            if attempt == 2:
                raise
        except CaptureError as error:
            if error.code != 'CAPTURE_FETCH_FAILED' or attempt == 2:
                raise
            remaining = _fetch_remaining(deadline, expires_at)
            fail(remaining > 0, 'CAPTURE_FETCH_TIMEOUT')
            fail(not cancel.wait(min(.25 * (2 ** attempt), remaining)), 'CAPTURE_CANCELLED')
    raise CaptureError('CAPTURE_FETCH_FAILED')


def _fetch_remaining(deadline, expires_at):
    # macOS's monotonic clock can exclude system suspension. Neither a wall
    # clock jump backward nor suspension may extend the original request budget.
    return min(deadline - time.monotonic(), expires_at - time.time())


def _curl_fetch_attempt(url, request, cancel, deadline, budget, expires_at):
    args = ['curl', '--disable', '--silent', '--show-error', '--fail',
            '--compressed', '--proto', '=https', '--max-redirs', '0',
            '--max-time', str(max(.001, _fetch_remaining(deadline, expires_at))), '--connect-timeout', '15',
            '--request', 'POST', '--header', 'Content-Type: application/json',
            '--write-out', '%{stderr}\nXONE-HTTP %{http_code}\n%header{retry-after}\n',
            '--data-binary', '@-', '--url', url]
    try:
        child = subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                 stderr=subprocess.PIPE, start_new_session=True)
    except OSError as error:
        raise CaptureError('CAPTURE_FETCH_START', digest(type(error).__name__.encode())) from None
    output = bytearray()
    stderr_hash = hashlib.sha256()
    diagnostics = bytearray()
    sent = 0
    try:
        with selectors.DefaultSelector() as selector:
            for stream, event, kind in [(child.stdin, selectors.EVENT_WRITE, 'stdin'),
                                        (child.stdout, selectors.EVENT_READ, 'stdout'),
                                        (child.stderr, selectors.EVENT_READ, 'stderr')]:
                os.set_blocking(stream.fileno(), False)
                selector.register(stream, event, kind)
            while selector.get_map():
                fail(not cancel.is_set(), 'CAPTURE_CANCELLED')
                fail(_fetch_remaining(deadline, expires_at) > 0, 'CAPTURE_FETCH_TIMEOUT')
                for key, _ in selector.select(timeout=.1):
                    stream, kind = key.fileobj, key.data
                    try:
                        if kind == 'stdin':
                            sent += os.write(stream.fileno(), request[sent:])
                            if sent == len(request):
                                selector.unregister(stream)
                                stream.close()
                            continue
                        data = os.read(stream.fileno(), 65536)
                    except BlockingIOError:
                        continue
                    except BrokenPipeError:
                        selector.unregister(stream)
                        stream.close()
                        continue
                    if not data:
                        selector.unregister(stream)
                        stream.close()
                    elif kind == 'stdout':
                        fail(len(data) <= budget[0], 'CAPTURE_RESPONSE_BUDGET')
                        budget[0] -= len(data)
                        output.extend(data)
                    else:
                        stderr_hash.update(data)
                        fail(len(data) <= budget[1], 'CAPTURE_STDERR_BUDGET')
                        budget[1] -= len(data)
                        diagnostics.extend(data)
        while child.poll() is None:
            fail(not cancel.wait(.05), 'CAPTURE_CANCELLED')
            fail(_fetch_remaining(deadline, expires_at) > 0, 'CAPTURE_FETCH_TIMEOUT')
        if child.returncode:
            trailer = re.search(rb'\nXONE-HTTP (\d{3})\n([^\r\n]*)\n\Z', diagnostics)
            if trailer:
                status = int(trailer[1])
                if status in (429, 503):
                    raise ProviderBackoff(stderr_hash.hexdigest(), status, _retry_seconds(trailer[2].strip()))
                if 400 <= status < 500 and status != 408:
                    raise CaptureError('CAPTURE_HTTP_' + str(status), stderr_hash.hexdigest())
            raise CaptureError('CAPTURE_FETCH_FAILED', stderr_hash.hexdigest())
        return bytes(output)
    except CaptureError as error:
        if error.detail_hash is None:
            error.detail_hash = stderr_hash.hexdigest()
        raise
    finally:
        _stop_child(child)
        for stream in (child.stdin, child.stdout, child.stderr):
            with suppress(OSError):
                stream.close()


class SignedRPCCapture:
    MAX_WORKERS = 4
    MAX_PENDING = 64
    MAX_RECORDS = 10000
    MAX_DECODED_BYTES = 512 << 20
    MAX_DISK_BYTES = 512 << 20
    MAX_RESPONSE_BYTES = RESPONSE_BYTES

    def __init__(self, directory, upstreams, fetcher=None):
        self.directory = Path(directory)
        self.upstreams = tuple(upstreams)
        fail(1 <= len(self.upstreams) <= 3, 'CAPTURE_SOURCE_COUNT')
        for url in self.upstreams:
            fail(isinstance(url, str) and not any(c.isspace() or ord(c) < 32 for c in url),
                 'CAPTURE_SOURCE_URL')
            try:
                parsed = urlsplit(url)
            except ValueError:
                raise CaptureError('CAPTURE_SOURCE_URL') from None
            fail(parsed.scheme == 'https' and parsed.hostname and parsed.username is None
                 and parsed.password is None and not parsed.fragment, 'CAPTURE_SOURCE_URL')
        self.providers = tuple(provider_id(url) for url in self.upstreams)
        self.pacing = {urlsplit(url).hostname: ProviderPace() for url in self.upstreams}
        self.fetcher = fetcher or self._fetch
        self.cancel = threading.Event()
        self.condition = threading.Condition(threading.RLock())
        self.jobs = queue.Queue(maxsize=self.MAX_PENDING)
        self.pending = {}
        self.records = {}
        self.failures = {}
        self.errors = Counter()
        self.error_hashes = Counter()
        self.decoded_bytes = self.disk_bytes = self.reserved_bytes = self.reserved_disk = 0
        self.fetches = self.misses = self.enqueued = 0
        self.storage_failed = False
        self.closed = self.frozen = self.started = False
        self.servers = []
        self.server_threads = []
        self.workers = []

    def _fetch(self, url, method, params, cancel):
        return curl_fetch(url, method, params, cancel, self.pacing[urlsplit(url).hostname])

    def __enter__(self):
        return self.start()

    def __exit__(self, *_):
        self.close()

    def start(self, serve=True):
        """serve=False supports deterministic tests without any network I/O."""
        with self.condition:
            fail(not self.started and not self.closed, 'CAPTURE_ALREADY_STARTED_OR_CLOSED')
            fail(not self.directory.is_symlink(), 'CAPTURE_DIRECTORY')
            self.directory.mkdir(mode=0o700, parents=True, exist_ok=True)
            fail(self.directory.is_dir(), 'CAPTURE_DIRECTORY')
            self.directory.chmod(0o700)
            for path in self.directory.iterdir():
                fail(re.fullmatch(r'[0-9a-f]{64}\.rpc', path.name) is not None,
                     'CAPTURE_UNEXPECTED_FILE')
                metadata, _, size = self._read_file(path)
                fail(metadata['provider_id'] in self.providers, 'CAPTURE_PROVIDER_MISMATCH')
                key = path.stem
                self.records[key] = metadata['response_bytes']
                self.decoded_bytes += metadata['response_bytes']
                self.disk_bytes += size
                fail(len(self.records) <= self.MAX_RECORDS and
                     self.decoded_bytes <= self.MAX_DECODED_BYTES and
                     self.disk_bytes <= self.MAX_DISK_BYTES, 'CAPTURE_CACHE_BUDGET')
            self.started = True
        try:
            if serve:
                self._start_server()
            for index in range(self.MAX_WORKERS):
                worker = threading.Thread(target=self._worker, name=f'rpc-capture-{index}', daemon=True)
                self.workers.append(worker)
                worker.start()
            return self
        except BaseException:
            self.close()
            raise

    @property
    def endpoints(self):
        fail(len(self.servers) == len(self.upstreams) and not self.closed,
             'CAPTURE_HTTP_NOT_RUNNING')
        return tuple(f'http://127.0.0.1:{server.server_port}' for server in self.servers)

    def _start_server(self):
        owner = self

        class Handler(BaseHTTPRequestHandler):
            source_index = None

            def setup(self):
                super().setup()
                # Request-line/header parsing precedes do_POST (and applies to
                # every unsupported method). A silent peer must not hold the
                # single bounded handler or shutdown indefinitely.
                self.connection.settimeout(2)

            def log_message(self, *_):
                pass

            def do_POST(self):
                self.connection.settimeout(2)
                try:
                    fail(self.headers.get('Transfer-Encoding') is None, 'CAPTURE_REQUEST_ENCODING')
                    length = self.headers.get('Content-Length', '')
                    fail(re.fullmatch(r'[0-9]{1,6}', length) is not None,
                         'CAPTURE_REQUEST_BUDGET')
                    length = int(length)
                    fail(0 < length <= REQUEST_BYTES, 'CAPTURE_REQUEST_BUDGET')
                    fail(self.path == '/', 'CAPTURE_SOURCE_INDEX')
                    raw = self.rfile.read(length)
                    fail(len(raw) == length, 'CAPTURE_REQUEST_TRUNCATED')
                    body = owner.response(self.source_index, raw)
                except CaptureError as error:
                    body = rpc_error(error.code)
                except (OSError, ValueError):
                    body = rpc_error('CAPTURE_INVALID_REQUEST')
                try:
                    self.send_response(200)
                    self.send_header('Content-Type', 'application/json')
                    self.send_header('Content-Length', str(len(body)))
                    self.end_headers()
                    self.wfile.write(body)
                except OSError:
                    pass

        # A single short-lived handler bounds decompression memory and request
        # threads. Cache misses return immediately; HTTPS acquisition is async.
        # Separate ports preserve the ordinary owner's strict path-free
        # loopback source scope; all providers share one acquisition budget.
        for index in range(len(self.upstreams)):
            handler = type(f'Provider{index}Handler', (Handler,), {'source_index': index})
            server = HTTPServer(('127.0.0.1', 0), handler)
            self.servers.append(server)
            thread = threading.Thread(target=lambda s=server: s.serve_forever(poll_interval=.1),
                                      daemon=True)
            self.server_threads.append(thread)
            thread.start()

    def mapping(self):
        """Audit relay identities without disclosing original URL credentials."""
        endpoints = self.endpoints
        return [{'source_index': index, 'provider_id': provider, 'endpoint': endpoints[index]}
                for index, provider in enumerate(self.providers)]

    def _key(self, provider, method, params):
        return digest(canonical([provider, method, params]).encode())

    def response(self, source_index, raw):
        try:
            fail(isinstance(raw, bytes) and 0 < len(raw) <= REQUEST_BYTES,
                 'CAPTURE_REQUEST_BUDGET')
            request = strict_json(raw)
            fail(isinstance(request, dict) and set(request) == {'jsonrpc', 'id', 'method', 'params'}
                 and request['jsonrpc'] == '2.0' and type(request['id']) is int
                 and request['id'] == 1, 'CAPTURE_REQUEST_ENVELOPE')
            method, params = request['method'], request['params']
            fail(isinstance(method, str) and method in READ_METHODS,
                 'CAPTURE_READ_ONLY_METHOD')
            fail(isinstance(params, (list, dict)), 'CAPTURE_REQUEST_PARAMS')
            fail(type(source_index) is int and 0 <= source_index < len(self.upstreams),
                 'CAPTURE_SOURCE_INDEX')
            # Escaping Unicode may enlarge an otherwise bounded HTTP request.
            fail(len(json.dumps(request, separators=(',', ':')).encode()) <= REQUEST_BYTES,
                 'CAPTURE_REQUEST_BUDGET')
            provider = self.providers[source_index]
            key = self._key(provider, method, params)
            with self.condition:
                fail(self.started and not self.closed, 'CAPTURE_CLOSED')
                fail(not self.storage_failed, 'CAPTURE_STORAGE_FAILED')
                if key in self.records:
                    metadata, response, _ = self._read_file(self.directory/(key + '.rpc'))
                    fail(metadata['provider_id'] == provider and metadata['method'] == method
                         and canonical(metadata['params']) == canonical(params),
                         'CAPTURE_RECORD_BINDING')
                    return response
                self.misses += 1
                if self.frozen:
                    raise CaptureError('UNRECORDED')
                if key in self.pending:
                    raise CaptureError('CAPTURE_PENDING')
                if key in self.failures:
                    raise CaptureError(self.failures[key])
                fail(len(self.pending) < self.MAX_PENDING, 'CAPTURE_PENDING_BUDGET')
                fail(len(self.records) + len(self.pending) + len(self.failures) < self.MAX_RECORDS,
                     'CAPTURE_RECORD_BUDGET')
                reserve = self.MAX_RESPONSE_BYTES
                disk_reserve = reserve + META_BYTES + GZIP_OVERHEAD + len(MAGIC) + 4
                fail(self.decoded_bytes + self.reserved_bytes + reserve <= self.MAX_DECODED_BYTES
                     and self.disk_bytes + self.reserved_disk + disk_reserve <= self.MAX_DISK_BYTES,
                     'CAPTURE_BYTES_BUDGET')
                self.pending[key] = (reserve, disk_reserve)
                self.reserved_bytes += reserve
                self.reserved_disk += disk_reserve
                self.jobs.put_nowait((key, source_index, method, params))
                self.enqueued += 1
                self.condition.notify_all()
            return rpc_error('CAPTURE_PENDING')
        except CaptureError as error:
            return rpc_error(error.code)
        except (ValueError, TypeError, KeyError, RecursionError):
            return rpc_error('CAPTURE_INVALID_REQUEST')

    def _read_file(self, path):
        try:
            return self._read_file_impl(path)
        except CaptureError:
            raise
        except (OSError, ValueError, TypeError, KeyError, RecursionError, OverflowError):
            raise CaptureError('CAPTURE_RECORD_INVALID') from None

    def _read_file_impl(self, path):
        fail(path.is_file() and not path.is_symlink(), 'CAPTURE_RECORD_FILE')
        descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
        with os.fdopen(descriptor, 'rb') as stream:
            info = os.fstat(stream.fileno())
            size = info.st_size
            fail(stat.S_ISREG(info.st_mode) and info.st_mode & 0o777 == 0o600,
                 'CAPTURE_RECORD_FILE')
            fail(size <= self.MAX_RESPONSE_BYTES + META_BYTES + GZIP_OVERHEAD + len(MAGIC) + 4,
                 'CAPTURE_RECORD_BUDGET')
            fail(stream.read(len(MAGIC)) == MAGIC, 'CAPTURE_RECORD_MAGIC')
            length = stream.read(4)
            fail(len(length) == 4, 'CAPTURE_RECORD_METADATA')
            length = struct.unpack('>I', length)[0]
            fail(0 < length <= META_BYTES, 'CAPTURE_RECORD_METADATA')
            envelope = strict_json(stream.read(length))
            fail(set(envelope) == {'metadata', 'sha256'} and
                 digest(canonical(envelope['metadata']).encode()) == envelope['sha256'],
                 'CAPTURE_RECORD_METADATA')
            metadata = envelope['metadata']
            fail(set(metadata) == {'schema_version', 'provider_id', 'method', 'params',
                                  'response_bytes', 'response_sha256', 'gzip_bytes', 'gzip_sha256'}
                 and metadata['schema_version'] == 1 and metadata['method'] in READ_METHODS
                 and isinstance(metadata['params'], (list, dict))
                 and re.fullmatch(r'rpc-[0-9a-f]{24}', metadata['provider_id']) is not None
                 and type(metadata['response_bytes']) is int
                 and 0 < metadata['response_bytes'] <= self.MAX_RESPONSE_BYTES
                 and type(metadata['gzip_bytes']) is int
                 and 0 < metadata['gzip_bytes'] <= self.MAX_RESPONSE_BYTES + GZIP_OVERHEAD,
                 'CAPTURE_RECORD_METADATA')
            fail(path.stem == self._key(metadata['provider_id'], metadata['method'], metadata['params']),
                 'CAPTURE_RECORD_BINDING')
            compressed = stream.read(self.MAX_RESPONSE_BYTES + GZIP_OVERHEAD + 1)
            fail(len(compressed) == metadata['gzip_bytes'] and
                 digest(compressed) == metadata['gzip_sha256'], 'CAPTURE_GZIP_HASH')
        try:
            with gzip.GzipFile(fileobj=io.BytesIO(compressed)) as stream:
                raw = stream.read(self.MAX_RESPONSE_BYTES + 1)
        except (OSError, EOFError, zlib.error):
            raise CaptureError('CAPTURE_GZIP_INVALID') from None
        fail(len(raw) == metadata['response_bytes'] and len(raw) <= self.MAX_RESPONSE_BYTES
             and digest(raw) == metadata['response_sha256'], 'CAPTURE_RESPONSE_HASH')
        self._validate_response(raw)
        return metadata, raw, size

    def _validate_response(self, raw):
        fail(isinstance(raw, bytes) and 0 < len(raw) <= self.MAX_RESPONSE_BYTES,
             'CAPTURE_RESPONSE_BUDGET')
        value = strict_json(raw)
        fail(isinstance(value, dict) and value.get('jsonrpc') == '2.0'
             and type(value.get('id')) is int and value['id'] == 1
             and 'error' not in value and value.get('result') is not None,
             'CAPTURE_RESPONSE_ENVELOPE')

    def _publish(self, key, provider, method, params, raw):
        self._validate_response(raw)
        compressed_buffer = io.BytesIO()
        with gzip.GzipFile(filename='', mode='wb', fileobj=compressed_buffer, mtime=0) as stream:
            stream.write(raw)
        compressed = compressed_buffer.getvalue()
        metadata = {'schema_version': 1, 'provider_id': provider, 'method': method, 'params': params,
                    'response_bytes': len(raw), 'response_sha256': digest(raw),
                    'gzip_bytes': len(compressed), 'gzip_sha256': digest(compressed)}
        encoded = canonical({'metadata': metadata, 'sha256': digest(canonical(metadata).encode())}).encode()
        fail(len(encoded) <= META_BYTES, 'CAPTURE_RECORD_METADATA')
        record = MAGIC + struct.pack('>I', len(encoded)) + encoded + compressed
        with self.condition:
            if self.closed or key not in self.pending:
                return
            fail(not self.storage_failed, 'CAPTURE_STORAGE_FAILED')
            _, disk_reserve = self.pending[key]
            fail(len(record) <= disk_reserve, 'CAPTURE_DISK_RESERVATION')
            path = self.directory/(key + '.rpc')
            fail(not path.exists() and not path.is_symlink(), 'CAPTURE_RECORD_EXISTS')
            temporary = self.directory/('.pending-' + uuid.uuid4().hex)
            published = False
            try:
                descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
                with os.fdopen(descriptor, 'wb') as stream:
                    stream.write(record)
                    stream.flush()
                    os.fsync(stream.fileno())
                os.replace(temporary, path)
                descriptor = os.open(self.directory, os.O_RDONLY)
                try:
                    os.fsync(descriptor)
                finally:
                    os.close(descriptor)
                published = True
            except OSError:
                # Durability is uncertain. Stop all further publication rather
                # than refund an unknown disk write into a reusable budget.
                self.storage_failed = True
                raise CaptureError('CAPTURE_STORAGE_FAILED') from None
            finally:
                for leftover in (temporary, *(() if published else (path,))):
                    try:
                        leftover.unlink()
                    except FileNotFoundError:
                        pass
                    except OSError:
                        self.storage_failed = True
                        if leftover.is_file():
                            self.disk_bytes += leftover.stat().st_size
            self.records[key] = len(raw)
            self.decoded_bytes += len(raw)
            self.disk_bytes += len(record)

    def _worker(self):
        while not self.cancel.is_set():
            try:
                key, index, method, params = self.jobs.get(timeout=.1)
            except queue.Empty:
                continue
            try:
                with self.condition:
                    if self.closed or key not in self.pending:
                        continue
                    self.fetches += 1
                raw = self.fetcher(self.upstreams[index], method, params, self.cancel)
                self._publish(key, self.providers[index], method, params, raw)
            except Exception as error:
                code = error.code if isinstance(error, CaptureError) else 'CAPTURE_FETCH_INVALID'
                detail = error.detail_hash if isinstance(error, CaptureError) else digest(type(error).__name__.encode())
                with self.condition:
                    if key in self.pending:
                        self.failures[key] = code
                        self.errors[code] += 1
                        if detail:
                            self.error_hashes[detail] += 1
            finally:
                with self.condition:
                    reservation = self.pending.pop(key, None)
                    if reservation:
                        self.reserved_bytes -= reservation[0]
                        self.reserved_disk -= reservation[1]
                    self.condition.notify_all()
                self.jobs.task_done()

    def wait_pending(self, timeout=30):
        fail(type(timeout) in (int, float) and 0 <= timeout <= 30, 'CAPTURE_WAIT_BUDGET')
        deadline = time.monotonic() + timeout
        with self.condition:
            while self.pending:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    return False
                self.condition.wait(remaining)
            return True

    def freeze(self):
        with self.condition:
            fail(self.started and not self.closed, 'CAPTURE_CLOSED')
            fail(not self.pending, 'CAPTURE_PENDING')
            self.frozen = True

    def request_inventory(self):
        """Return authenticated request templates, never response replacements."""
        with self.condition:
            fail(self.started and not self.closed, 'CAPTURE_CLOSED')
            fail(not self.storage_failed, 'CAPTURE_STORAGE_FAILED')
            inventory = []
            for key in sorted(self.records):
                metadata, _, _ = self._read_file(self.directory/(key + '.rpc'))
                fail(metadata['provider_id'] in self.providers, 'CAPTURE_PROVIDER_MISMATCH')
                inventory.append({
                    'provider_id': metadata['provider_id'],
                    'source_index': self.providers.index(metadata['provider_id']),
                    'method': metadata['method'],
                    'params': copy.deepcopy(metadata['params']),
                    'response_sha256': metadata['response_sha256'],
                    'response_bytes': metadata['response_bytes'],
                })
            return inventory

    def stats(self):
        with self.condition:
            return {'pending': len(self.pending), 'completed': len(self.records),
                    'errors': sum(self.errors.values()), 'error_codes': dict(self.errors),
                    'error_hashes': dict(self.error_hashes), 'bytes': self.decoded_bytes,
                    'disk_bytes': self.disk_bytes, 'reserved_bytes': self.reserved_bytes,
                    'reserved_disk_bytes': self.reserved_disk, 'fetches': self.fetches,
                    'misses': self.misses, 'enqueued': self.enqueued,
                    'frozen': self.frozen, 'closed': self.closed,
                    'storage_failed': self.storage_failed,
                    'provider_ids': list(self.providers)}

    def close(self):
        with self.condition:
            if self.closed:
                return
            self.closed = True
            self.cancel.set()
            if self.pending:
                self.errors['CAPTURE_CANCELLED'] += len(self.pending)
            self.pending.clear()
            self.reserved_bytes = self.reserved_disk = 0
            self.condition.notify_all()
        for server, thread in zip(self.servers, self.server_threads):
            if thread.is_alive():
                server.shutdown()
            server.server_close()
        deadline = time.monotonic() + 5
        for thread in [*self.server_threads, *self.workers]:
            if thread is not None:
                thread.join(timeout=max(0, deadline - time.monotonic()))
