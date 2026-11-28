#!/usr/bin/env python3
"""Generate the synthetic Solana scope used by offline parser and Router tests."""

import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
CHAIN = "solana:localnet"


def digest(value):
    encoded = json.dumps(value, separators=(",", ":")).encode("ascii")
    return hashlib.sha256(encoded).hexdigest()


def main():
    pool = {
        "chain_key": CHAIN,
        "pool_id": "11111111111111111111111111111114",
        "protocol": "fixture-cpmm",
        "token0": "A",
        "token1": "B",
        "reserve0": "1000000",
        "reserve1": "1000000",
        "fee_numerator": 3,
        "fee_denominator": 1000,
        "adapter_version": "fixture-cpmm/1",
        "token_behavior": "standard",
    }
    snapshot = {
        "reference": {
            "block": {
                "chain_key": CHAIN,
                "number_or_slot": 800000,
                "hash": "11111111111111111111111111111112",
                "parent_hash": "11111111111111111111111111111113",
                "timestamp_ms": 1792022401000,
            },
            "canonical_epoch": 1,
            "journal_seq": 1,
            "schema_version": 1,
            "online_generation": 1,
            "writer_epoch": 1,
            "owner_service": "chain-parser-rust",
            "owner_assignment_version": 1,
            "scope_hash": digest([pool["pool_id"]]),
            "adapter_set_hash": digest([pool["adapter_version"]]),
        },
        "pools": [pool],
        "source": "SYNTHETIC_SOLANA_BASELINE",
        "history_gaps": [],
    }
    output = json.dumps(snapshot, indent=2) + "\n"
    for path in ("integration/test/solana", "chain-parser-rust/test/fixtures/solana"):
        directory = ROOT / path
        directory.mkdir(parents=True, exist_ok=True)
        (directory / "baseline.json").write_text(output, encoding="ascii")


if __name__ == "__main__":
    main()
