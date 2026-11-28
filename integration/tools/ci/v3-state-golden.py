#!/usr/bin/env python3
"""Write the synthetic Uniswap V3 state inputs used by the wire and SDK oracles."""

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
CONSUMERS = ("integration", "chain-parser-go", "chain-parser-rust", "dex-router")
TICK_SPACING = 60


def state(price, tick, positions):
    bitmaps = {tick // TICK_SPACING // 256: 0}
    ticks = []
    liquidity = 0
    for index, net in positions:
        word, bit = divmod(index // TICK_SPACING, 256)
        bitmaps[word] = bitmaps.get(word, 0) | (1 << bit)
        ticks.append({"tick": index, "liquidity_gross": str(abs(net)), "liquidity_net": str(net)})
        if index <= tick:
            liquidity += net
    return {
        "chain_key": "evm:31337",
        "pool_id": "0x" + "03" * 20,
        "protocol": "uniswap-v3",
        "token0": "0x" + "01" * 20,
        "token1": "0x" + "02" * 20,
        "reserve0": "0",
        "reserve1": "0",
        "fee_numerator": 3000,
        "fee_denominator": 1000000,
        "adapter_version": "uniswap-v3/core-1",
        "token_behavior": "standard",
        "v3": {
            "sqrt_price_x96": str(price),
            "tick": tick,
            "liquidity": str(liquidity),
            "tick_spacing": TICK_SPACING,
            "factory": "0x" + "04" * 20,
            "code_sha256": "aa" * 32,
            "max_steps": 512,
            "tick_words": [
                {"word_pos": word, "bitmap": str(bitmap)}
                for word, bitmap in sorted(bitmaps.items())
            ],
            "ticks": ticks,
        },
    }


def main():
    canonical = state(
        1 << 96,
        0,
        [(-600, 10**9), (-120, 2 * 10**9), (120, -2 * 10**9), (600, -(10**9))],
    )
    minimum = state(
        4295128739,
        -887272,
        [(-887220, 10**24), (-887160, -(10**24))],
    )
    for name, pool in (("v3-state.json", canonical), ("v3-minimum-state.json", minimum)):
        contents = json.dumps(pool, indent=2) + "\n"
        for consumer in CONSUMERS:
            destination = ROOT / consumer / "test/contracts" / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_text(contents, encoding="ascii")


if __name__ == "__main__":
    main()
