#!/usr/bin/env python3
"""Read finalized mainnet pool state and cross-check normal development parsers."""
import argparse
import hashlib
import json
import os
import re
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
    if (not isinstance(reply, dict) or reply.get("jsonrpc") != "2.0" or type(reply.get("id")) is not int
            or reply["id"] != 1 or reply.get("result") is None or "error" in reply):
        raise ValueError(f"{method} invalid RPC reply {reply.get('error') if isinstance(reply, dict) else 'envelope'}")
    return reply["result"]


def block_from_provider(target, url, height):
    """Always check genesis on the same endpoint that supplies the block."""
    if target["family"] == "evm":
        if rpc(url, "eth_chainId", []) != hex(target["chain_id"]):
            raise RuntimeError("wrong EVM chain ID")
        genesis = rpc(url, "eth_getBlockByNumber", ["0x0", False])
        if genesis["number"] != "0x0" or genesis["hash"] != target["genesis_hash"]:
            raise RuntimeError("wrong EVM genesis")
        block = rpc(url, "eth_getBlockByNumber", ["finalized" if height is None else hex(height), False])
        number, timestamp = int(block["number"], 16), int(block["timestamp"], 16)
        block_hash, parent = block["hash"], block["parentHash"]
        if any(not isinstance(value, str) or not re.fullmatch(r"0x[0-9a-fA-F]{64}", value)
               for value in (block_hash, parent)):
            raise RuntimeError("invalid EVM block hash")
    else:
        if rpc(url, "getGenesisHash", []) != target["genesis_hash"]:
            raise RuntimeError("wrong Solana genesis")
        number = height if height is not None else rpc(url, "getSlot", [{"commitment": "finalized"}])
        block = rpc(url, "getBlock", [number, SOL_BLOCK])
        timestamp, block_hash, parent = block["blockTime"], block["blockhash"], block["previousBlockhash"]
    if number <= 0 or timestamp <= 0 or not block_hash or not parent or (height is not None and number != height):
        raise RuntimeError("invalid block reference")
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


def abi_words(url, pool, selector, height, count):
    raw = rpc(url, "eth_call", [{"to": pool, "data": selector}, height])
    if not isinstance(raw, str) or not re.fullmatch("0x[0-9a-fA-F]{" + str(count * 64) + "}", raw):
        raise RuntimeError("invalid ABI response for " + selector)
    return [int(raw[i:i+64], 16) for i in range(2, len(raw), 64)]


def check_public_pool(target, expected=None, urls=None):
    """Pin the deployed contracts and read their state on one finalized block."""
    probe = target["public_pool_probe"]
    pool = probe["pool"]
    failures = []
    for url in (urls if urls is not None else target["rpc_urls"]):
        try:
            ref = block_from_provider(target, url, expected["number_or_slot"] if expected else None)
            if expected is not None and ref != expected:
                raise RuntimeError(f"parser block differs from {url} at {expected['number_or_slot']}")
            height = hex(ref["number_or_slot"])
            code = rpc(url, "eth_getCode", [pool, height])
            if not isinstance(code, str) or not re.fullmatch(r"0x(?:[0-9a-fA-F]{2})+", code):
                raise RuntimeError("missing deployed pool code")
            if hashlib.sha256(bytes.fromhex(code[2:])).hexdigest() != probe["runtime_sha256"]:
                raise RuntimeError(f"pool runtime differs at {height} on {url}")
            for key, selector in (("token0", "0x0dfe1681"), ("token1", "0xd21220a7"), ("factory", "0xc45a0155")):
                value, = abi_words(url, pool, selector, height, 1)
                if value != int(probe[key], 16):
                    raise RuntimeError(f"pool {key} differs at {height} on {url}")
            if probe["protocol"] == "pancake-v2":
                r0, r1, stamp = abi_words(url, pool, "0x0902f1ac", height, 3)
                if r0 >= 1 << 112 or r1 >= 1 << 112 or stamp >= 1 << 32:
                    raise RuntimeError("invalid V2 reserve encoding")
                state = dict(reserve0=str(r0), reserve1=str(r1))
            elif probe["protocol"] == "uniswap-v3":
                slot = abi_words(url, pool, "0x3850c7bd", height, 7)
                liquidity, = abi_words(url, pool, "0x1a686502", height, 1)
                tick = slot[1] - (1 << 256) if slot[1] >= 1 << 255 else slot[1]
                if not 0 < slot[0] < 1 << 160 or not -887272 <= tick < 887272 or liquidity >= 1 << 128:
                    raise RuntimeError("invalid V3 price, tick or liquidity")
                state = dict(sqrt_price_x96=str(slot[0]), tick=tick, liquidity=str(liquidity))
            else:
                raise RuntimeError("unsupported public pool probe")
            closing = rpc(url, "eth_getBlockByNumber", [height, False])
            if (closing["number"], closing["hash"], closing["parentHash"], int(closing["timestamp"], 16) * 1000) != (
                    height, ref["hash"], ref["parent_hash"], ref["timestamp_ms"]):
                raise RuntimeError(f"provider changed block at {height} on {url}")
            return dict(rpc=url, block=ref, pool=pool, protocol=probe["protocol"], contract_state=state)
        except (ValueError, KeyError, TypeError, OSError, subprocess.TimeoutExpired) as error:
            failures.append(f"{url} {error}")
    raise RuntimeError(" | ".join(failures) or "empty RPC list")


def parser_baselines(binary, config, directory):
    token = secrets.token_urlsafe(32)
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        listen = f"127.0.0.1:{sock.getsockname()[1]}"
    config["state"]["path"] = str(directory / "state")
    config["api"].update(listen=listen, auth_token=token)
    path = directory / "config-dev.yaml"
    path.write_text(json.dumps(config))
    path.chmod(0o600)
    requests = {chain["key"]: urllib.request.Request(f"http://{listen}/internal/chains/{chain['key']}/baseline",
                                                      headers={"Authorization": f"Bearer {token}"})
                for chain in config["chains"]}
    snapshots = {}
    poll_interval = max(0.5, 60 * len(requests) / config["api"]["requests_per_minute"])
    local = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    with (directory / "parser.log").open("w+b") as log:
        proc = subprocess.Popen([str(binary), "--config", str(path)], stdout=log, stderr=log)
        try:
            deadline = time.monotonic() + 150
            while proc.poll() is None and time.monotonic() < deadline:
                for key, request in requests.items():
                    if key in snapshots:
                        continue
                    try:
                        with local.open(request, timeout=2) as response:
                            snapshots[key] = json.load(response)
                    except (urllib.error.URLError, TimeoutError, ConnectionError):
                        continue
                if len(snapshots) == len(requests):
                    return snapshots
                time.sleep(min(poll_interval, max(0, deadline - time.monotonic())))
            end = log.seek(0, 2)
            log.seek(max(0, end - 2048))
            missing = sorted(requests.keys() - snapshots.keys())
            raise RuntimeError(f"no parser baseline for {missing} (exit {proc.poll()}) {log.read().decode(errors='replace')}")
        finally:
            if proc.poll() is None:
                proc.terminate()
                try:
                    proc.wait(timeout=8)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()


def probe_owner_chain(base, target):
    """Build a chain-specific owner scope from the pinned public deployment."""
    probe = target["public_pool_probe"]
    pool = {key: probe[key] for key in ("protocol", "token0", "token1", "fee_numerator", "fee_denominator")}
    pool["id"] = probe["pool"]
    identity = dict(expected_factory=probe["factory"], expected_code_sha256=probe["runtime_sha256"])
    if pool["protocol"] == "uniswap-v3":
        pool["v3"] = dict(identity, tick_word_radius=0, tick_read_concurrency=4, max_initialized_ticks=256, max_steps=8192)
    elif pool["protocol"] == "pancake-v2":
        pool["v2"] = identity
    else:
        raise ValueError("unsupported public pool protocol")
    if base["block_integrity"] != "known-evm-native-commitments-v2" or base["receipts_mode"] != "bulk-auto":
        raise ValueError("expected native EVM commitments and receipts")
    chain = {key: base[key] for key in ("owner_service", "assignment_version", "confirmation_tag", "block_integrity",
                                         "native_proof", "receipts_mode", "auto_reorg_window")}
    chain.update(key=target["chain_key"], genesis_hash=target["genesis_hash"], rpc_urls=target["rpc_urls"][:2],
                 pools=[pool])
    return chain


def check_owners(targets, binary):
    project = targets[0]["owner"]
    if any(target["owner"] != project for target in targets):
        raise ValueError("a shared parser must own every selected chain")
    config = json.loads((ROOT / project / "config/config-dev.yaml").read_bytes())
    base, = config["chains"]
    chains = [probe_owner_chain(base, t) if "public_pool_probe" in t else base for t in targets]
    for target, chain in zip(targets, chains):
        if (chain["key"], chain["genesis_hash"], chain["confirmation_tag"]) != (
                target["chain_key"], target["genesis_hash"], "finalized"):
            raise ValueError("development config identity or commitment differs from target")
    config["chains"] = chains
    with tempfile.TemporaryDirectory(prefix="xone-public-rpc-") as tmp:
        snapshots = parser_baselines(binary, config, Path(tmp))
    return [check_snapshot(target, chain, snapshots[chain["key"]], config["startup"]["max_baseline_age_ms"])
            for target, chain in zip(targets, chains)]


def check_snapshot(target, chain, snapshot, max_age_ms):
    project = target["owner"]
    expected = {(p["id"], p["protocol"], p["token0"], p["token1"]) for p in chain["pools"]}
    observed = {(p["pool_id"], p["protocol"], p["token0"], p["token1"]) for p in snapshot["pools"]}
    if (not expected or observed != expected or len(observed) != len(snapshot["pools"])
            or snapshot["reference"]["owner_service"] != project
            or any(p["chain_key"] != target["chain_key"] for p in snapshot["pools"])
            or snapshot["reference"]["block"]["chain_key"] != target["chain_key"]):
        raise ValueError("parser pool scope differs from configured pools")
    ref = snapshot["reference"]["block"]
    age = int(time.time() * 1000) - ref["timestamp_ms"]
    if not -30000 <= age <= max_age_ms:
        raise ValueError(f"parser baseline outside configured freshness window ({age} ms)")
    # Prefer the secondary for the cross-check. Report the actual responding URL.
    urls = list(reversed(chain["rpc_urls"]))
    if "public_pool_probe" in target:
        row = check_public_pool(target, ref, urls)
        pool, = snapshot["pools"]
        values = pool["v3"] if pool["protocol"] == "uniswap-v3" else pool
        if any(values[key] != observed for key, observed in row["contract_state"].items()):
            raise ValueError("parser pool state differs from the same-block contract reads")
        if pool["protocol"] == "uniswap-v3":
            row["initialized_ticks"] = len(values["ticks"])
    else:
        row = check_block(target, urls, ref)
    row.update(parser=project, pools=[p["pool_id"] for p in snapshot["pools"]])
    return row


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--go-binary", type=Path, default=ROOT / "chain-parser-go/bin/chain-parser-go")
    rust_target = ROOT / "chain-parser-rust" / os.environ.get("CARGO_TARGET_DIR", "target")
    parser.add_argument("--rust-binary", type=Path, default=rust_target / "debug/chain-parser-rust")
    targets = json.loads((ROOT / "integration/config/chain-targets.json").read_bytes())["targets"]
    parser.add_argument("--all-owners", action="store_true", help="require a live parser baseline for each selected chain")
    parser.add_argument("--chain", choices=targets, action="append", help="limit this run to the selected chain(s)")
    parser.add_argument("--shared-owner", action="store_true", help="run the selected chains in a single owner process")
    args = parser.parse_args()
    binaries = {"chain-parser-go": args.go_binary.resolve(), "chain-parser-rust": args.rust_binary.resolve()}
    names = list(dict.fromkeys(args.chain or targets))
    if args.shared_owner:
        selected = [targets[name] for name in names]
        if len(names) < 2 or len({t["owner"] for t in selected}) != 1:
            parser.error("--shared-owner requires at least two chains owned by the same parser")
        try:
            for row in check_owners(selected, binaries[selected[0]["owner"]]):
                print(json.dumps(row), flush=True)
        except (ValueError, KeyError, TypeError, OSError, RuntimeError) as exc:
            print(f"shared owner {names} FAILED {exc}", file=sys.stderr, flush=True)
            return 1
        return 0
    failed = False
    for name in names:
        target = targets[name]
        try:
            if args.all_owners or "public_pool_probe" not in target:
                row, = check_owners([target], binaries[target["owner"]])
            else:
                row = check_public_pool(target)
            print(json.dumps(row), flush=True)
        except (ValueError, KeyError, TypeError, OSError, RuntimeError) as exc:
            print(f"{name} FAILED {exc}", file=sys.stderr, flush=True)
            failed = True
    return int(failed)


if __name__ == "__main__":
    sys.exit(main())
