import copy
import hashlib
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import public_rpc_smoke as smoke


class PublicPoolProbe(unittest.TestCase):
    def setUp(self):
        self.pool = dict(protocol="uniswap-v3", pool="0x" + "1"*40, token0="0x" + "2"*40,
                         token1="0x" + "3"*40, factory="0x" + "4"*40,
                         runtime_sha256=hashlib.sha256(b"\x60\x00").hexdigest())
        self.target = dict(family="evm", chain_id=10, chain_key="evm:10", genesis_hash="0x" + "b"*64,
                           rpc_urls=["https://one.example", "https://two.example"], public_pool_probe=self.pool)
        self.block = dict(number="0x64", timestamp="0xa", hash="0x" + "a"*64, parentHash="0x" + "c"*64)
        self.balance = [37, 55, 10]
        self.change = None
        self.servers = []
        self.chain_id = 10

    def rpc(self, url, method, params):
        self.servers.append(url)
        if self.change == "offline" and url == self.target["rpc_urls"][0]:
            raise OSError("unavailable")
        if method == "eth_chainId":
            return "0x99" if self.change == "chain" else hex(self.chain_id)
        if method == "eth_getBlockByNumber":
            selector, full = params
            self.assertIs(full, False)
            if selector == "0x0":
                return dict(number="0x0", hash="0x" + "0"*64 if self.change == "genesis" else self.target["genesis_hash"])
            self.assertIn(selector, ("finalized", self.block["number"]))
            if self.change == "reorg" and selector != "finalized":
                return dict(self.block, hash="0x" + "f"*64)
            return self.block
        if method == "eth_getCode":
            self.assertEqual(params, [self.pool["pool"], self.block["number"]])
            return "0x6001" if self.change == "code" else "0x6000"
        self.assertEqual(method, "eth_call")
        call, height = params
        self.assertEqual((call["to"], height), (self.pool["pool"], "0x64"))
        words = {
            "0x0dfe1681": [int(self.pool["token0"], 16)],
            "0xd21220a7": [int(self.pool["token1"], 16)],
            "0xc45a0155": [0 if self.change == "factory" else int(self.pool["factory"], 16)],
            "0x0902f1ac": self.balance, "0x3850c7bd": [1 << 96, (1 << 256)-3, 0, 1, 1, 0, 1],
            "0x1a686502": [60],
        }[call["data"]]
        return "0x" + "".join(f"{v:064x}" for v in words)

    def test_reads_pinned_v3_price_and_signed_tick_and_v2_reserves(self):
        with patch.object(smoke, "rpc", self.rpc):
            row = smoke.check_public_pool(self.target)
            self.assertEqual(row["contract_state"], dict(sqrt_price_x96=str(1 << 96), tick=-3, liquidity="60"))
            self.pool["protocol"] = "pancake-v2"
            self.assertEqual(smoke.check_public_pool(self.target)["contract_state"], dict(reserve0="37", reserve1="55"))

    def test_conflicting_runtime_factory_or_header_cannot_be_masked_by_fallback(self):
        for self.change in ("chain", "genesis", "code", "factory", "reorg"):
            self.servers.clear()
            with self.subTest(change=self.change), patch.object(smoke, "rpc", self.rpc), self.assertRaises(RuntimeError):
                smoke.check_public_pool(self.target)
            self.assertEqual(set(self.servers), {self.target["rpc_urls"][0]})

    def test_fallback_rechecks_identity_and_reads_state_from_the_responding_provider(self):
        self.change = "offline"
        with patch.object(smoke, "rpc", self.rpc):
            row = smoke.check_public_pool(self.target)
        self.assertEqual(row["rpc"], self.target["rpc_urls"][1])
        self.assertEqual(row["block"]["hash"], self.block["hash"])

    def test_fixed_height_must_equal_parser_reference(self):
        with patch.object(smoke, "rpc", self.rpc):
            expected = smoke.block_from_provider(self.target, self.target["rpc_urls"][0], None)
            row = smoke.check_public_pool(self.target, expected=expected)
            self.assertEqual(row["block"], expected)
            with self.assertRaisesRegex(RuntimeError, "parser block differs"):
                smoke.check_public_pool(self.target, expected=dict(expected, parent_hash="0x" + "e" * 64))

    def test_ethereum_owner_reserves_must_match_finalized_contract(self):
        self.chain_id = 1
        target = dict(self.target, chain_key="evm:1", chain_id=1, owner="chain-parser-go")
        target.pop("public_pool_probe")
        chain = dict(rpc_urls=target["rpc_urls"], pools=[dict(
            id=self.pool["pool"], protocol="uniswap-v2-complete", token0=self.pool["token0"], token1=self.pool["token1"],
            v2=dict(expected_factory=self.pool["factory"], expected_code_sha256=self.pool["runtime_sha256"]))])
        with patch.object(smoke, "rpc", self.rpc), patch.object(smoke.time, "time", return_value=11):
            block = smoke.block_from_provider(target, chain["rpc_urls"][0], None)
            pool = dict(pool_id=self.pool["pool"], chain_key=target["chain_key"], protocol="uniswap-v2-complete",
                        token0=self.pool["token0"], token1=self.pool["token1"], reserve0="37", reserve1="55")
            snapshot = dict(reference=dict(owner_service=target["owner"], block=block), pools=[pool])
            row = smoke.check_snapshot(target, chain, snapshot, 10000)
            self.assertEqual(row["contract_state"], dict(reserve0="37", reserve1="55"))
            self.assertEqual(row["rpc"], chain["rpc_urls"][1])
            pool["reserve1"] = "56"
            with self.assertRaisesRegex(ValueError, "same-block contract"):
                smoke.check_snapshot(target, chain, snapshot, 10000)

    def test_invalid_block_hash_cannot_bind_a_contract_observation(self):
        self.block["hash"] = "0xabc"
        with patch.object(smoke, "rpc", self.rpc), self.assertRaisesRegex(RuntimeError, "invalid EVM block hash"):
            smoke.check_public_pool(self.target)

    def test_invalid_v2_width_is_rejected(self):
        self.pool["protocol"] = "pancake-v2"
        self.balance[0] = 1 << 112
        with patch.object(smoke, "rpc", self.rpc), self.assertRaisesRegex(RuntimeError, "invalid V2 reserve encoding"):
            smoke.check_public_pool(self.target)

    def test_owner_templates_pin_the_deployment_and_keep_native_commitments(self):
        import json
        targets = json.loads((smoke.ROOT / "integration/config/chain-targets.json").read_bytes())["targets"]
        config = json.loads((smoke.ROOT / "chain-parser-go/config/config-dev.yaml").read_bytes())
        for target in targets.values():
            if "public_pool_probe" not in target:
                continue
            with self.subTest(chain=target["chain_key"]):
                base, = config["chains"]
                chain = smoke.probe_owner_chain(base, target)
                pool, = chain["pools"]
                probe = target["public_pool_probe"]
                self.assertEqual((chain["key"], chain["genesis_hash"]), (target["chain_key"], target["genesis_hash"]))
                self.assertEqual((pool["id"], pool["protocol"]), (probe["pool"], probe["protocol"]))
                self.assertEqual(chain["rpc_urls"], target["rpc_urls"][:2])
                self.assertNotIn("proof_rpc_url", chain)
                self.assertEqual(chain["block_integrity"], "known-evm-native-commitments-v2")
                self.assertEqual(chain["confirmation_tag"], "finalized")
                if pool["protocol"] == "uniswap-v3":
                    self.assertEqual(pool["v3"]["tick_read_concurrency"], 4)
                    self.assertEqual(pool["v3"]["tick_read_batch_size"], 1 if chain["key"] == "evm:8453" else 16)

    def test_pool_from_different_chain_cannot_match_by_address(self):
        chain = {"pools": [{"id": "pool", "token0": "a", "token1": "b", "protocol": "pancake-v2"}]}
        pool = dict(pool_id="pool", protocol="pancake-v2", token0="a", token1="b", chain_key="evm:10")
        target = dict(owner="chain-parser-go", chain_key="evm:8453")
        snapshot = dict(pools=[pool], reference=dict(owner_service="chain-parser-go", block={"chain_key": "evm:8453"}))
        with self.assertRaisesRegex(ValueError, "pool scope differs"):
            smoke.check_snapshot(target, chain, snapshot, 1200000)

    def test_solana_snapshot_binds_vault_reserves_to_its_account_bank(self):
        key = "solana:mainnet-beta"
        target = dict(owner="chain-parser-rust", chain_key=key)
        chain = dict(rpc_urls=["primary", "secondary"], pools=[
            dict(id="pool", protocol="raydium-cpmm-creator", token0="a", token1="b")])
        state = dict(bank_slot=44, raw_vault0=40, raw_vault1=50, fund_fees0=1, fund_fees1=1,
                     protocol_fees0=2, protocol_fees1=2, creator_fees0=3, creator_fees1=3)
        block = dict(chain_key=key, number_or_slot=44, timestamp_ms=10000)
        pool = dict(pool_id="pool", protocol="raydium-cpmm-creator", chain_key=key, token0="a", token1="b",
                    reserve0="34", reserve1="44", raydium_cpmm=state)
        snapshot = dict(pools=[pool], reference=dict(owner_service=target["owner"], block=block))
        with patch.object(smoke, "check_block", return_value={"block": block}), patch.object(smoke.time, "time", return_value=10):
            row = smoke.check_snapshot(target, chain, snapshot, 120000)
            self.assertEqual(row["parser_pool_state"], dict(bank_slot=44, reserve0="34", reserve1="44"))
            for changed in (dict(bank_slot=45), dict(fund_fees0=0), dict(raw_vault1=1 << 64), dict(creator_fees0=50)):
                with self.subTest(changed=changed), patch.dict(state, changed), self.assertRaises(ValueError):
                    smoke.check_snapshot(target, chain, snapshot, 120000)

    def test_offline_read_binds_published_state_call_count_and_provider(self):
        live = dict(source="RPC", reference=dict(writer_epoch=1, scope_hash="scope", block=dict(number_or_slot=44)),
                    pools=[dict(reserve0="34")], history_gaps=[])
        replay = dict(scope="offline-rpc-baseline-read", evidence_only=True, read_number=3, snapshot=copy.deepcopy(live),
                      read=dict(session_id=2, source_id=hashlib.sha256(b"provider").hexdigest(),
                                sequences=[1, 2, 3, 3], snapshot_sha256="f"*64))
        receipt = smoke.check_replayed_snapshot(live, replay, "provider", True)
        self.assertEqual((receipt["calls"], receipt["raw_records"], receipt["read_number"]), (4, 3, 3))
        live["reference"]["writer_epoch"] = 2
        live["history_gaps"].append({"from": 40})
        smoke.check_replayed_snapshot(live, replay, "provider", False)
        with self.assertRaises(ValueError):
            smoke.check_replayed_snapshot(live, replay, "provider", True)
        for obj, changes in [(replay, dict(evidence_only=False)), (replay["read"], dict(sequences=[1, 2, 3])),
                             (replay["read"], dict(source_id="0"*64)), (replay["snapshot"]["reference"], dict(scope_hash="new")),
                             (replay["snapshot"], dict(pools=[dict(reserve0="35")]))]:
            with self.subTest(changes=changes), patch.dict(obj, changes), self.assertRaises(ValueError):
                smoke.check_replayed_snapshot(live, replay, "provider", False)

    def test_evm_replay_remaps_proof_provider_and_requires_every_source(self):
        from replay_beacon_window import ArchiveRPC, provider_id

        urls = ["https://one.example", "https://two.example", "https://proof.example"]
        pool = dict(id="pool", v3=dict(tick_read_batch_size=16))
        config = dict(state=dict(path="/live"), chains=[dict(key="evm:1", rpc_urls=urls[:2], proof_rpc_url=urls[2], pools=[pool])])
        expected = {"evm:1": {"pools": [dict(pool_id=pool["id"], v3=dict(ticks=[-1, 1]))]}}
        sources = {provider_id(url): ArchiveRPC(provider_id(url)) for url in urls}
        local = {source: f"http://127.0.0.1:{3000+i}" for i, source in enumerate(sources)}
        for source in sources.values():
            source.served = 1
        primary = sources[provider_id(urls[0])]
        primary.served_batches = 1

        def baseline(binary, cfg, directory):
            chain, = cfg["chains"]
            self.assertEqual(chain["rpc_urls"], [local[provider_id(url)] for url in urls[:2]])
            self.assertEqual(chain["proof_rpc_url"], local[provider_id(urls[2])])
            return copy.deepcopy(expected)

        with patch("replay_beacon_window.read_window", return_value=(sources, {})), \
                patch.object(ArchiveRPC, "start", lambda rpc: local[rpc.source]), \
                patch.object(smoke, "parser_baselines", side_effect=baseline) as parser:
            args = (Path("parser"), config, expected, Path("/offline"))
            row = smoke.replay_evm_baselines(*args)["evm:1"]
            self.assertEqual(len(row["providers"]), 3)
            self.assertEqual([item["replayed_batches"] for item in row["providers"]], [1, 0, 0])
            self.assertEqual(config["chains"][0]["rpc_urls"], urls[:2])
            primary.served_batches = 0
            with self.assertRaisesRegex(ValueError, "did not use a recorded tick batch"):
                smoke.replay_evm_baselines(*args)
            primary.served_batches = 1
            source = sources[provider_id(urls[1])]
            for served, trace in [(0, []), (1, [{"error": "UNRECORDED_RPC_REQUEST"}])]:
                source.served, source.trace = served, trace
                with self.assertRaisesRegex(ValueError, "unrecorded read or skipped"):
                    smoke.replay_evm_baselines(*args)
            parser.side_effect = lambda *_: {"evm:1": {"pools": []}}
            with self.assertRaisesRegex(ValueError, "differs"):
                smoke.replay_evm_baselines(*args)
