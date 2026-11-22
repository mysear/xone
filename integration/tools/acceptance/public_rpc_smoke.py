#!/usr/bin/env python3
"""Read finalized mainnet blocks and cross-check the normal development parsers."""
import argparse
import json
import os
import secrets
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
SOL_BLOCK = {"commitment": "finalized", "transactionDetails": "none", "rewards": False,
             "maxSupportedTransactionVersion": 0}


def rpc(url, method, params):
    request = json.dumps({"id": 1, "jsonrpc": "2.0", "method": method, "params": params})
    result = subprocess.run(
        ["curl", "-q", "--silent", "--show-error", "--fail", "--proto", "=https",
         "--max-time", "15", "--max-filesize", "4194304", "--compressed",
         "-H", "Content-Type: application/json", "--data-binary", "@-", url],
        input=request, capture_output=True, text=True, timeout=18)
    if result.returncode:
        raise ValueError(f"{method} {result.stderr.strip()}")
    reply = json.loads(result.stdout)
    if reply.get("jsonrpc") != "2.0" or reply.get("id") != 1 or reply.get("result") is None or "error" in reply:
        raise ValueError(f"{method} invalid RPC reply {reply.get('error')}")
    return reply["result"]


def block_from_provider(target, url, height):
    """Always check genesis on the same endpoint that supplies the block."""
    if target["family"] == "evm":
        if rpc(url, "eth_chainId", []) != hex(target["chain_id"]):
            raise ValueError("wrong EVM chain ID")
        genesis = rpc(url, "eth_getBlockByNumber", ["0x0", False])
        if genesis["number"] != "0x0" or genesis["hash"] != target["genesis_hash"]:
            raise ValueError("wrong EVM genesis")
        block = rpc(url, "eth_getBlockByNumber", ["finalized" if height is None else hex(height), False])
        number, timestamp = int(block["number"], 16), int(block["timestamp"], 16)
        block_hash, parent = block["hash"], block["parentHash"]
    else:
        if rpc(url, "getGenesisHash", []) != target["genesis_hash"]:
            raise ValueError("wrong Solana genesis")
        number = height if height is not None else rpc(url, "getSlot", [{"commitment": "finalized"}])
        block = rpc(url, "getBlock", [number, SOL_BLOCK])
        timestamp, block_hash, parent = block["blockTime"], block["blockhash"], block["previousBlockhash"]
    if number <= 0 or timestamp <= 0 or not block_hash or not parent or (height is not None and number != height):
        raise ValueError("invalid block reference")
    return dict(chain_key=target["chain_key"], number_or_slot=number, timestamp_ms=timestamp * 1000,
                hash=block_hash, parent_hash=parent)


def check_block(target, urls, expected=None):
    failures = []
    for url in urls:
        try:
            ref = block_from_provider(target, url, expected["number_or_slot"] if expected else None)
        except (OSError, ValueError, KeyError, TypeError, subprocess.TimeoutExpired) as exc:
            failures.append(f"{url} {exc}")
            continue
        if expected is not None and ref != expected:
            raise ValueError(f"parser block differs from {url} at {expected['number_or_slot']}")
        return dict(rpc=url, block=ref)
    raise RuntimeError(" | ".join(failures) or "empty RPC list")


def parser_baseline(binary, config, directory):
    chain, = config["chains"]
    token = secrets.token_urlsafe(32)
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        listen = f"127.0.0.1:{sock.getsockname()[1]}"
    config["state"]["path"] = str(directory / "state")
    config["api"].update(listen=listen, auth_token=token)
    path = directory / "config-dev.yaml"
    path.write_text(json.dumps(config))
    path.chmod(0o600)
    request = urllib.request.Request(f"http://{listen}/internal/chains/{chain['key']}/baseline",
                                     headers={"Authorization": f"Bearer {token}"})
    local = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    with (directory / "parser.log").open("w+b") as log:
        proc = subprocess.Popen([str(binary), "--config", str(path)], stdout=log, stderr=log)
        try:
            deadline = time.monotonic() + 150
            while proc.poll() is None and time.monotonic() < deadline:
                try:
                    with local.open(request, timeout=2) as response:
                        return json.load(response)
                except (urllib.error.URLError, TimeoutError, ConnectionError):
                    time.sleep(0.5)
            end = log.seek(0, 2)
            log.seek(max(0, end - 2048))
            raise RuntimeError(f"no parser baseline (exit {proc.poll()}) {log.read().decode(errors='replace')}")
        finally:
            if proc.poll() is None:
                proc.terminate()
                try:
                    proc.wait(timeout=8)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()


def check_owner(target, binary):
    project = target["owner"]
    config = json.loads((ROOT / project / "config/config-dev.yaml").read_bytes())
    chain, = config["chains"]
    if (chain["key"], chain["genesis_hash"], chain["confirmation_tag"]) != (
            target["chain_key"], target["genesis_hash"], "finalized"):
        raise ValueError("development config identity or commitment differs from target")
    with tempfile.TemporaryDirectory(prefix="xone-public-rpc-") as tmp:
        snapshot = parser_baseline(binary, config, Path(tmp))
    expected = {(p["id"], p["protocol"], p["token0"], p["token1"]) for p in chain["pools"]}
    observed = {(p["pool_id"], p["protocol"], p["token0"], p["token1"]) for p in snapshot["pools"]}
    if (not expected or observed != expected or len(observed) != len(snapshot["pools"])
            or snapshot["reference"]["owner_service"] != project):
        raise ValueError("parser pool scope differs from configured pools")
    ref = snapshot["reference"]["block"]
    age = int(time.time() * 1000) - ref["timestamp_ms"]
    if not -30000 <= age <= config["startup"]["max_baseline_age_ms"]:
        raise ValueError(f"parser baseline outside configured freshness window ({age} ms)")
    # Prefer the secondary for the cross-check. Report the actual responding URL.
    row = check_block(target, list(reversed(chain["rpc_urls"])), ref)
    row.update(parser=project, pools=[p["pool_id"] for p in snapshot["pools"]])
    return row


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--go-binary", type=Path, default=ROOT / "chain-parser-go/bin/chain-parser-go")
    rust_target = ROOT / "chain-parser-rust" / os.environ.get("CARGO_TARGET_DIR", "target")
    parser.add_argument("--rust-binary", type=Path, default=rust_target / "debug/chain-parser-rust")
    args = parser.parse_args()
    binaries = {"ethereum": args.go_binary.resolve(), "solana": args.rust_binary.resolve()}
    targets = json.loads((ROOT / "integration/config/chain-targets.json").read_bytes())["targets"]
    failed = False
    for name, target in targets.items():
        try:
            row = check_owner(target, binaries[name]) if name in binaries else check_block(target, target["rpc_urls"])
            print(json.dumps(row), flush=True)
        except (ValueError, KeyError, TypeError, OSError, RuntimeError) as exc:
            print(f"{name} FAILED {exc}", file=sys.stderr, flush=True)
            failed = True
    return int(failed)


if __name__ == "__main__":
    sys.exit(main())
