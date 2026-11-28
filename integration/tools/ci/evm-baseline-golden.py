#!/usr/bin/env python3
"""Generate the synthetic EVM route bank and cross-language wire vectors."""

import hashlib
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]
SERVICES = ("chain-parser-go", "chain-parser-rust", "dex-router")
CHAIN = "evm:31337"
ADAPTER = "uniswap-v2/1"


def digest(value):
    canonical = json.dumps(value, sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(canonical.encode("ascii")).hexdigest()


def write_fixture(directory, name, value):
    directory.mkdir(parents=True, exist_ok=True)
    (directory / name).write_text(json.dumps(value, indent=2) + "\n", encoding="ascii")


def main():
    pools = [
        {
            "chain_key": CHAIN,
            "pool_id": f"0x{pool_number:040x}",
            "protocol": "uniswap-v2",
            "token0": token0,
            "token1": token1,
            "reserve0": str(reserve0),
            "reserve1": str(reserve1),
            "fee_numerator": 3,
            "fee_denominator": 1000,
            "adapter_version": ADAPTER,
            "token_behavior": "standard",
        }
        for pool_number, token0, token1, reserve0, reserve1 in [
            (1, "A", "B", 1_000_000, 2_000_000),
            (2, "B", "C", 1_500_000, 1_505_000),
            (3, "A", "C", 1_000_000, 2_000_000),
            (4, "A", "C", 2_000_000, 4_000_000),
        ]
    ]
    reference = {
        "block": {
            "chain_key": CHAIN,
            "number_or_slot": 100,
            "hash": "0x" + "aa" * 32,
            "parent_hash": "0x" + "bb" * 32,
            "timestamp_ms": 1_793_577_600_000,
        },
        "canonical_epoch": 1,
        "journal_seq": 1,
        "schema_version": 1,
        "adapter_set_hash": digest([ADAPTER]),
        "online_generation": 1,
        "scope_hash": digest(sorted(pool["pool_id"] for pool in pools)),
        "writer_epoch": 1,
        "owner_service": "chain-parser-go",
        "owner_assignment_version": 1,
    }
    snapshot = {
        "reference": reference,
        "pools": pools,
        "source": "SYNTHETIC_EVM_BASELINE",
        "history_gaps": [
            {"from": 1, "to": 99, "reason": "HEAD_FIRST_HISTORY_DECOUPLED"},
        ],
    }
    proto = ROOT / "common/protoc/proto"
    amount = subprocess.run(
        ["protoc", f"--proto_path={proto}", "--encode=dex.v1.Amount", str(proto / "dex/v1/state.proto")],
        input=('uint256_be: "' + "\\xff" * 32 + '"\n').encode("ascii"),
        check=True,
        capture_output=True,
    ).stdout
    golden = {
        "snapshot_sha256": digest(snapshot),
        "scope_hash": reference["scope_hash"],
        "protobuf_amount_hex": amount.hex(),
    }
    write_fixture(ROOT / "integration/test/evm", "baseline.json", snapshot)
    write_fixture(ROOT / "integration/test/contracts", "snapshot-golden.json", golden)
    for service in SERVICES:
        write_fixture(ROOT / service / "test/fixtures/evm", "baseline.json", snapshot)
        write_fixture(ROOT / service / "test/contracts", "snapshot-golden.json", golden)
    subprocess.run([sys.executable, str(Path(__file__).with_name("journal-golden.py"))], check=True)


if __name__ == "__main__":
    main()
