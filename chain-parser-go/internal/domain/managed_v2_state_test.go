package domain

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"google.golang.org/protobuf/encoding/protowire"
	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/reflect/protoreflect"
	"math/big"
	"reflect"
	"strings"
	"testing"
	pb "xone/common/protoc/golang/dexpb"
)

// Synthetic validation authority only; no claim about original-chain bytecode.
const managedV2SyntheticJSON = `{"chain_key":"evm:1","pool_id":"0x0000000000000000000000000000000000000046","protocol":"balancer-v2-managed-aum","adapter_version":"balancer-v2/20230411-managed-aum-3","token0":"0x0000000000000000000000000000000000000001","token1":"0x0000000000000000000000000000000000000002","reserve0":"1000","reserve1":"1000","fee_numerator":1000000000000,"fee_denominator":1000000000000000000,"token_behavior":"standard","multi_asset":{"code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","vault":"0x0000000000000000000000000000000000000047","vault_code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","pool_id":"0x0000000000000000000000000000000000000046000100000000000000000000","assets":[{"token":"0x0000000000000000000000000000000000000001","balance":"1000","scaling":"1000000000000000000","code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},{"token":"0x0000000000000000000000000000000000000002","balance":"1000","scaling":"1000000000000000000","code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}],"weights":["500000000000000000","500000000000000000"],"amplification":"0","admin_fee":0,"managed_v2":{"timestamp":1000,"block_number":100,"block_hash":"0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","factory":"0x0000000000000000000000000000000000000050","factory_code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","pool_runtime_template_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","fee_provider":"0x0000000000000000000000000000000000000051","fee_provider_code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","protocol_collector":"0x0000000000000000000000000000000000000052","protocol_collector_code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","dependencies":[{"role":"circuit-breaker-lib","address":"0x000000000000000000000000000000000000005a","code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","template_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},{"role":"managed-pool-add-remove-token-lib","address":"0x000000000000000000000000000000000000005b","code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","template_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},{"role":"managed-pool-amm-lib","address":"0x000000000000000000000000000000000000005c","code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","template_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},{"role":"external-weighted-math","address":"0x000000000000000000000000000000000000005d","code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","template_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},{"role":"recovery-mode-helper","address":"0x000000000000000000000000000000000000005e","code_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","template_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}],"total_supply":"1000","virtual_supply":"900","actual_supply":"905","pending_aum":"5","vault_bpt_cash":"100","vault_bpt_managed":"0","bpt_index":0,"manager":"0x0000000000000000000000000000000000000053","manager_bpt_balance":"600","protocol_collector_bpt_balance":"200","management_aum_fee":"10000000000000000","last_collection_timestamp":0,"provider_fee_id":"3","live_provider_aum_percentage":"700000000000000000","cached_protocol_aum_percentage":"300000000000000000","swaps_enabled":true,"paused":false,"vault_paused":false,"vault_pause_window_end":0,"vault_buffer_period_end":0,"recovery_mode":false,"allowlist_enabled":false,"pause_window_end":0,"buffer_period_end":0,"gradual_weight_end":0,"gradual_fee_end":0,"token_info":[{"cash":"700","managed":"300","asset_manager":"0x0000000000000000000000000000000000000054","last_change_block":0,"circuit_breaker_enabled":false,"vault_actual_balance":"700"},{"cash":"700","managed":"300","asset_manager":"0x0000000000000000000000000000000000000054","last_change_block":0,"circuit_breaker_enabled":false,"vault_actual_balance":"700"}],"vault_bpt_actual_balance":"100"}}}`

func managedV2Synthetic(t *testing.T) Pool {
	t.Helper()
	var p Pool
	if e := json.Unmarshal([]byte(managedV2SyntheticJSON), &p); e != nil {
		t.Fatal(e)
	}
	return p
}
func TestManagedV2SyntheticProfile(t *testing.T) {
	p := managedV2Synthetic(t)
	if e := p.Validate(); e != nil {
		t.Fatal(e)
	}
}
func TestManagedV2JSONPresence(t *testing.T) {
	var p map[string]any
	_ = json.Unmarshal([]byte(managedV2SyntheticJSON), &p)
	m := p["multi_asset"].(map[string]any)["managed_v2"].(map[string]any)
	for key := range m {
		for _, null := range []bool{false, true} {
			var q Pool
			var v map[string]any
			_ = json.Unmarshal([]byte(managedV2SyntheticJSON), &v)
			mm := v["multi_asset"].(map[string]any)["managed_v2"].(map[string]any)
			if null {
				mm[key] = nil
			} else {
				delete(mm, key)
			}
			b, _ := json.Marshal(v)
			e := json.Unmarshal(b, &q)
			if e == nil {
				e = q.Validate()
			}
			if e == nil {
				t.Errorf("accepted absent/null %s", key)
			}
		}
	}
	for _, r := range []string{strings.Replace(managedV2SyntheticJSON, `"paused":false`, `"paused":false,"vault_paused":false,"vault_pause_window_end":0,"vault_buffer_period_end":0,"paused":true`, 1), strings.Replace(managedV2SyntheticJSON, `"paused":false`, `"Paused":false`, 1), strings.Replace(managedV2SyntheticJSON, `"paused":false`, `"paused":false,"vault_paused":false,"vault_pause_window_end":0,"vault_buffer_period_end":0,"unknown":0`, 1)} {
		var q Pool
		if e := json.Unmarshal([]byte(r), &q); e == nil {
			t.Error("accepted duplicate/alias/unknown")
		}
	}
}
func TestManagedV2PublishedSwapFeeAndVaultAliases(t *testing.T) {
	p := managedV2Synthetic(t)
	p.FeeNumerator = 950000000000000000
	if e := p.Validate(); e != nil {
		t.Errorf("published 95 percent swap fee: %v", e)
	}
	q := managedV2Synthetic(t)
	q.MultiAsset.ManagedV2.Manager = q.MultiAsset.Vault
	q.MultiAsset.ManagedV2.VaultBPTActualBalance = q.MultiAsset.ManagedV2.ManagerBPTBalance
	if e := q.Validate(); e != nil {
		t.Error("Vault wallet is not this pool registration", e)
	}
	q = managedV2Synthetic(t)
	q.MultiAsset.ManagedV2.ProtocolCollector = q.MultiAsset.Vault
	q.MultiAsset.ManagedV2.VaultBPTActualBalance = q.MultiAsset.ManagedV2.ProtocolCollectorBPTBalance
	if e := q.Validate(); e != nil {
		t.Error("Vault wallet is not this pool registration", e)
	}
}

func TestManagedV2BoundariesAndIsolation(t *testing.T) {
	cases := map[string]func(*Pool){
		"version":       func(p *Pool) { p.AdapterVersion = "balancer-v2-managed-fees/e281ff40-bpt-1" },
		"family":        func(p *Pool) { p.Protocol = "balancer-v2-managed-snapshot" },
		"missing":       func(p *Pool) { p.MultiAsset.ManagedV2 = nil },
		"swap_fee_high": func(p *Pool) { p.FeeNumerator = 950000000000000001 },
		"swap_fee_low":  func(p *Pool) { p.FeeNumerator = 999999999999 },
		"denom":         func(p *Pool) { p.FeeDenominator-- },
		"u256": func(p *Pool) {
			p.MultiAsset.ManagedV2.ProviderFeeID = Amount(new(big.Int).Lsh(big.NewInt(1), 256).String())
		},
		"aum":                         func(p *Pool) { p.MultiAsset.ManagedV2.ManagementAUMFee = "950000000000000001" },
		"live_cache_independent_high": func(p *Pool) { p.MultiAsset.ManagedV2.LiveProviderAUMPercentage = "1000000000000000001" },
		"cache_high":                  func(p *Pool) { p.MultiAsset.ManagedV2.CachedProtocolAUMPercentage = "1000000000000000001" },
		"actual":                      func(p *Pool) { p.MultiAsset.ManagedV2.ActualSupply = "906" },
		"virtual":                     func(p *Pool) { p.MultiAsset.ManagedV2.VirtualSupply = "901" },
		"own_registered":              func(p *Pool) { p.MultiAsset.ManagedV2.VaultBPTManaged = "1" },
		"recipient_supply":            func(p *Pool) { p.MultiAsset.ManagedV2.ManagerBPTBalance = "1001" },
		"distinct_total":              func(p *Pool) { p.MultiAsset.ManagedV2.ManagerBPTBalance = "950" },
		"recipient_alias":             func(p *Pool) { p.MultiAsset.ManagedV2.Manager = p.MultiAsset.ManagedV2.ProtocolCollector },
		"role_unknown":                func(p *Pool) { p.MultiAsset.ManagedV2.Dependencies[0].Role = "CircuitBreakerLib" },
		"role_duplicate": func(p *Pool) {
			p.MultiAsset.ManagedV2.Dependencies[1].Role = p.MultiAsset.ManagedV2.Dependencies[0].Role
		},
		"role_missing":         func(p *Pool) { p.MultiAsset.ManagedV2.Dependencies = p.MultiAsset.ManagedV2.Dependencies[1:] },
		"template":             func(p *Pool) { p.MultiAsset.ManagedV2.Dependencies[0].TemplateSHA256 = "" },
		"runtime":              func(p *Pool) { p.MultiAsset.CodeSHA256 = strings.Repeat("A", 64) },
		"source_hash":          func(p *Pool) { p.MultiAsset.ManagedV2.BlockHash = "0x" + strings.Repeat("0", 64) },
		"source_time_overflow": func(p *Pool) { p.MultiAsset.ManagedV2.Timestamp = ^uint64(0) },
		"token_total":          func(p *Pool) { p.MultiAsset.ManagedV2.TokenInfo[0].Managed = "301" },
		"u112": func(p *Pool) {
			p.MultiAsset.ManagedV2.TokenInfo[0].Managed = Amount(new(big.Int).Lsh(big.NewInt(1), 112).String())
		},
		"weight_min":    func(p *Pool) { p.MultiAsset.Weights[0] = "9999999999999999" },
		"weight_sum":    func(p *Pool) { p.MultiAsset.Weights[0] = "500000000000000001" },
		"own_bpt_route": func(p *Pool) { p.MultiAsset.Assets[1].Token = p.PoolID; p.Token1 = p.PoolID },
		"order": func(p *Pool) {
			p.MultiAsset.Assets[0], p.MultiAsset.Assets[1] = p.MultiAsset.Assets[1], p.MultiAsset.Assets[0]
		},
		"specialization":  func(p *Pool) { p.MultiAsset.PoolID = p.PoolID + "0000" + strings.Repeat("0", 20) },
		"bpt_index":       func(p *Pool) { p.MultiAsset.ManagedV2.BPTIndex = 3 },
		"weight_schedule": func(p *Pool) { p.MultiAsset.ManagedV2.GradualWeightEnd = 1001 },
		"fee_schedule":    func(p *Pool) { p.MultiAsset.ManagedV2.GradualFeeEnd = 1001 },
		"breaker":         func(p *Pool) { p.MultiAsset.ManagedV2.TokenInfo[0].CircuitBreakerEnabled = true },
	}
	for name, mutate := range cases {
		t.Run(name, func(t *testing.T) {
			p := managedV2Synthetic(t)
			mutate(&p)
			if p.Validate() == nil {
				t.Fatal("accepted invalid state")
			}
		})
	}
	for _, n := range []int{2, 38, 39, 50, 51} {
		p := managedV2Synthetic(t)
		s := p.MultiAsset
		m := s.ManagedV2
		s.Assets = nil
		s.Weights = nil
		m.TokenInfo = nil
		for i := 0; i < n; i++ {
			s.Assets = append(s.Assets, MultiAsset{Token: fmt.Sprintf("0x%040x", i+1), Balance: "1000", Scaling: "1000000000000000000", CodeSHA256: strings.Repeat("a", 64)})
			w := big.NewInt(10000000000000000)
			if i == n-1 {
				w.Sub(big.NewInt(1000000000000000000), new(big.Int).Mul(w, big.NewInt(int64(n-1))))
			}
			s.Weights = append(s.Weights, Amount(w.String()))
			m.TokenInfo = append(m.TokenInfo, ManagedV2TokenInfo{VaultActualBalance: "700", Cash: "700", Managed: "300", AssetManager: fmt.Sprintf("0x%040x", 84)})
		}
		if e := p.Validate(); (e == nil) != (n <= 50) {
			t.Fatalf("new capacity %d: %v", n, e)
		}
		if n >= 39 {
			p.Protocol = "balancer-v2-managed-snapshot"
			p.AdapterVersion = "balancer-v2-managed-snapshot/no-bpt-fees-1"
			s.ManagedV2 = nil
			if p.Validate() == nil {
				t.Fatal("expanded old 38 token range")
			}
		}
	}
	p := managedV2Synthetic(t)
	m := p.MultiAsset.ManagedV2
	m.ManagementAUMFee = "950000000000000000"
	m.Manager = p.MultiAsset.Vault
	m.ManagerBPTBalance = "900"
	m.ProtocolCollectorBPTBalance = "50"
	m.VaultBPTCash = "900"
	m.VaultBPTActualBalance = "900"
	m.VirtualSupply = "100"
	m.ActualSupply = "105"
	if e := p.Validate(); e != nil {
		t.Fatal("Vault manager balance greater than virtual must be allowed", e)
	}
}
func TestManagedV2ZeroSupplyAndClone(t *testing.T) {
	p := managedV2Synthetic(t)
	q := p.Clone()
	q.MultiAsset.ManagedV2.Dependencies[0].Role = "mutated"
	q.MultiAsset.ManagedV2.TokenInfo[0].Cash = "1"
	if p.MultiAsset.ManagedV2.Dependencies[0].Role == "mutated" || p.MultiAsset.ManagedV2.TokenInfo[0].Cash != "700" {
		t.Fatal("shallow clone")
	}
	m := p.MultiAsset.ManagedV2
	for _, a := range []*Amount{&m.TotalSupply, &m.VirtualSupply, &m.ActualSupply, &m.PendingAUM, &m.VaultBPTCash, &m.VaultBPTManaged, &m.VaultBPTActualBalance, &m.ManagerBPTBalance, &m.ProtocolCollectorBPTBalance} {
		*a = "0"
	}
	for i := range m.TokenInfo {
		m.TokenInfo[i].Cash = "0"
		m.TokenInfo[i].Managed = "0"
		p.MultiAsset.Assets[i].Balance = "0"
	}
	p.Reserve0 = "0"
	p.Reserve1 = "0"
	if e := p.Validate(); e != nil {
		t.Fatal("uninitialized pool", e)
	}
	for _, a := range []string{"last_collection_timestamp", "bpt_index"} {
		raw := strings.Replace(managedV2SyntheticJSON, `"`+a+`":0`, `"`+a+`":4294967296`, 1)
		if a == "bpt_index" {
			raw = strings.Replace(managedV2SyntheticJSON, `"bpt_index":0`, `"bpt_index":4294967296`, 1)
		}
		var q Pool
		if json.Unmarshal([]byte(raw), &q) == nil {
			t.Fatal("accepted u32 overflow", a)
		}
	}
}
func managedV2Snapshot(t *testing.T, p Pool) Snapshot {
	t.Helper()
	scope, _ := ScopeHash([]Pool{p})
	m := p.MultiAsset.ManagedV2
	return Snapshot{Pools: []Pool{p}, Reference: SnapshotRef{Block: BlockRef{ChainKey: p.ChainKey, Number: m.BlockNumber, Hash: m.BlockHash, TimestampMS: m.Timestamp * 1000}, SchemaVersion: 1, OwnerService: "test", OwnerAssignmentVersion: 1, WriterEpoch: 1, CanonicalEpoch: 1, OnlineGeneration: 1, JournalSeq: 1, AdapterSetHash: "synthetic", ScopeHash: scope}}
}
func TestManagedV2SourceBankAndDerived(t *testing.T) {
	p := managedV2Synthetic(t)
	snap := managedV2Snapshot(t, p)
	if e := snap.Validate("test", 1); e != nil {
		t.Fatal(e)
	}
	for i := 0; i < 3; i++ {
		s := managedV2Snapshot(t, p)
		switch i {
		case 0:
			s.Reference.Block.Number++
		case 1:
			s.Reference.Block.Hash = "0x" + strings.Repeat("b", 64)
		case 2:
			s.Reference.Block.TimestampMS++
		}
		if s.Validate("test", 1) == nil {
			t.Fatal("accepted foreign bank", i)
		}
	}
	p.MultiAsset.ManagedV2.TokenInfo[0].LastChangeBlock = 101
	p.MultiAsset.ManagedV2.LastCollectionTimestamp = 1001
	if e := p.Validate(); e != nil {
		t.Fatal("explicit derived execution state", e)
	}
	if managedV2Snapshot(t, p).Validate("test", 1) == nil {
		t.Fatal("future token block in canonical snapshot")
	}
}
func TestManagedV2WirePresenceWidthsAndCopy(t *testing.T) {
	p := managedV2Synthetic(t)
	w, e := MultiAssetToProto(p.MultiAsset)
	if e != nil {
		t.Fatal(e)
	}
	if w.ProtoReflect().Descriptor().Fields().ByName("managed_v2").Number() != 13 {
		t.Fatal("tag")
	}
	for name, tag := range map[string]int{"curve_ng": 9, "managed_fees": 10, "curve_meta": 11, "curve_crypto": 12} {
		if int(w.ProtoReflect().Descriptor().Fields().ByName(protoreflect.Name(name)).Number()) != tag {
			t.Fatal("changed legacy tag", name)
		}
	}
	b, e := proto.Marshal(w)
	if e != nil {
		t.Fatal(e)
	}
	sum := sha256.Sum256(b)
	if hex.EncodeToString(sum[:]) != "c681408d1fe8bcf3b52a3083b2684db036f8fadc1830c83f728e5cd3daf4751b" {
		t.Fatal("shared Go/Rust wire digest changed")
	}
	var decoded pb.MultiAssetState
	if e = proto.Unmarshal(b, &decoded); e != nil {
		t.Fatal(e)
	}
	q, e := MultiAssetFromProto(&decoded)
	if e != nil || !reflect.DeepEqual(q, p.MultiAsset) {
		t.Fatal("wire round trip", e)
	}
	for i := 0; i < reflect.TypeOf(w.ManagedV2).Elem().NumField(); i++ {
		typ := reflect.TypeOf(w.ManagedV2).Elem().Field(i)
		if typ.PkgPath != "" || typ.Type.Kind() != reflect.Pointer {
			continue
		}
		x := proto.Clone(w).(*pb.MultiAssetState)
		reflect.ValueOf(x.ManagedV2).Elem().Field(i).SetZero()
		if _, e := MultiAssetFromProto(x); e == nil {
			t.Error("accepted missing proto scalar/amount", typ.Name)
		}
	}
	*w.ManagedV2.Timestamp = 42
	*w.ManagedV2.Paused = true
	w.ManagedV2.Dependencies[0].CodeSha256[0] = 0
	if p.MultiAsset.ManagedV2.Timestamp != 1000 || p.MultiAsset.ManagedV2.Paused || p.MultiAsset.ManagedV2.Dependencies[0].CodeSHA256 != strings.Repeat("a", 64) {
		t.Fatal("ToProto shares mutable storage")
	}
	good, _ := MultiAssetToProto(p.MultiAsset)
	for _, mut := range []func(*pb.BalancerManagedV2State){func(s *pb.BalancerManagedV2State) { s.ProviderFeeId.Uint256Be = s.ProviderFeeId.Uint256Be[1:] }, func(s *pb.BalancerManagedV2State) { s.BlockHash = nil }, func(s *pb.BalancerManagedV2State) { s.Dependencies[0].TemplateSha256 = nil }, func(s *pb.BalancerManagedV2State) { s.TokenInfo[0].Cash = nil }, func(s *pb.BalancerManagedV2State) { s.TokenInfo[0].LastChangeBlock = nil }, func(s *pb.BalancerManagedV2State) { s.TokenInfo[0].CircuitBreakerEnabled = nil }} {
		x := proto.Clone(good).(*pb.MultiAssetState)
		mut(x.ManagedV2)
		if _, e := MultiAssetFromProto(x); e == nil {
			t.Fatal("accepted malformed wire")
		}
	}
}

func TestManagedV2KnownZeroAndExecutionFlags(t *testing.T) {
	p := managedV2Synthetic(t)
	m := p.MultiAsset.ManagedV2
	m.Timestamp = 0
	m.BlockNumber = 0
	m.LastCollectionTimestamp = 0
	m.BPTIndex = 0
	m.ProviderFeeID = "0"
	m.ManagementAUMFee = "0"
	m.PendingAUM = "0"
	m.ActualSupply = m.VirtualSupply
	m.LiveProviderAUMPercentage = "0"
	m.CachedProtocolAUMPercentage = "0"
	m.SwapsEnabled = false
	m.Paused = true
	m.RecoveryMode = true
	m.AllowlistEnabled = true
	if e := p.Validate(); e != nil {
		t.Fatal("complete non-executable state must still parse", e)
	}
	w, e := MultiAssetToProto(p.MultiAsset)
	if e != nil {
		t.Fatal(e)
	}
	if w.ManagedV2.Timestamp == nil || *w.ManagedV2.Timestamp != 0 || w.ManagedV2.BlockNumber == nil || *w.ManagedV2.BlockNumber != 0 || w.ManagedV2.BptIndex == nil || *w.ManagedV2.BptIndex != 0 || w.ManagedV2.SwapsEnabled == nil || *w.ManagedV2.SwapsEnabled {
		t.Fatal("Known(0) lost")
	}
	q, e := MultiAssetFromProto(w)
	if e != nil || !reflect.DeepEqual(q, p.MultiAsset) {
		t.Fatal("known zero wire", e)
	}
	if e = managedV2Snapshot(t, p).Validate("test", 1); e != nil {
		t.Fatal(e)
	}
}

func TestManagedV2CanonicalCollectionClock(t *testing.T) {
	p := managedV2Synthetic(t)
	p.MultiAsset.ManagedV2.LastCollectionTimestamp = 1001
	if e := p.Validate(); e != nil {
		t.Fatal("derived collection state", e)
	}
	if managedV2Snapshot(t, p).Validate("test", 1) == nil {
		t.Fatal("future collection clock in canonical source snapshot")
	}
}

func TestManagedV2VaultRecipientUsesGlobalWalletLedger(t *testing.T) {
	for _, recipient := range []string{"manager", "collector", "both"} {
		p := managedV2Synthetic(t)
		m := p.MultiAsset.ManagedV2
		m.VaultBPTCash = "900"
		m.VaultBPTActualBalance = "950"
		m.VirtualSupply = "100"
		m.ActualSupply = "105"
		m.ManagerBPTBalance = "30"
		m.ProtocolCollectorBPTBalance = "30"
		if recipient != "collector" {
			m.Manager = p.MultiAsset.Vault
			m.ManagerBPTBalance = "950"
		}
		if recipient != "manager" {
			m.ProtocolCollector = p.MultiAsset.Vault
			m.ProtocolCollectorBPTBalance = "950"
		}
		if e := p.Validate(); e != nil {
			t.Errorf("global Vault BPT wallet may include other pools (%s): %v", recipient, e)
		}
	}
}

// Protobuf uint32 decoding truncates out-of-range varints. Decode these two
// source storage words at full wire width and enforce their u32 domain width.
func TestManagedV2RejectsOverflowingWireStorageWords(t *testing.T) {
	p := managedV2Synthetic(t)
	w, e := ManagedV2ToProto(p.MultiAsset.ManagedV2)
	if e != nil {
		t.Fatal(e)
	}
	for _, tag := range []protowire.Number{18, 23} {
		b, e := proto.Marshal(w)
		if e != nil {
			t.Fatal(e)
		}
		b = protowire.AppendTag(b, tag, protowire.VarintType)
		b = protowire.AppendVarint(b, 1<<32)
		var x pb.BalancerManagedV2State
		if e = proto.Unmarshal(b, &x); e != nil {
			continue
		}
		if _, e = ManagedV2FromProto(&x); e == nil {
			t.Error("u32 source word truncated into known zero", tag)
		}
	}
}

func TestManagedV2VaultHistoryUint32(t *testing.T) {
	p := managedV2Synthetic(t)
	m := p.MultiAsset.ManagedV2
	m.BlockNumber = uint64(1)<<32 + 8
	m.TokenInfo[0].LastChangeBlock = uint64(1)<<32 + 1
	if p.Validate() == nil {
		t.Fatal("accepted history wider than original Vault packed uint32")
	}
	m.TokenInfo[0].LastChangeBlock = uint64(1)<<32 - 1
	if err := p.Validate(); err != nil {
		t.Fatal(err)
	}
	m.TokenInfo[0].LastChangeBlock = 7
	if err := p.Validate(); err != nil {
		t.Fatal("rejected original packed history after wrap:", err)
	}
}

func TestManagedV2DerivedExecutionContext(t *testing.T) {
	raw := strings.Replace(managedV2SyntheticJSON, `"token_info":`, `"execution_context":{"block_number":4294967297,"timestamp":1001},"token_info":`, 1)
	var p Pool
	if e := json.Unmarshal([]byte(raw), &p); e != nil {
		t.Fatal("derived context JSON", e)
	}
	if e := p.Validate(); e != nil {
		t.Fatal(e)
	}
	q := p.Clone()
	encoded, e := json.Marshal(q)
	if e != nil {
		t.Fatal(e)
	}
	if !strings.Contains(string(encoded), `"execution_context":{"block_number":4294967297,"timestamp":1001}`) {
		t.Fatal("lost context")
	}
	var round Pool
	if e := json.Unmarshal(encoded, &round); e != nil || !reflect.DeepEqual(round, p) {
		t.Fatal("roundtrip context", e)
	}
	if managedV2Snapshot(t, p).Validate("test", 1) == nil {
		t.Fatal("derived snapshot accepted")
	}
	if _, e := ManagedV2ToProto(p.MultiAsset.ManagedV2); e == nil {
		t.Fatal("wire silently discarded context")
	}
	if _, e := MultiAssetToProto(p.MultiAsset); e == nil {
		t.Fatal("multi wire discarded context")
	}
	q.MultiAsset.ManagedV2.ExecutionContext.Timestamp = 1002
	encoded, _ = json.Marshal(p)
	if !strings.Contains(string(encoded), `"timestamp":1001`) {
		t.Fatal("clone aliases execution context")
	}
	for _, field := range []string{"collection", "history"} {
		invalid := p.Clone()
		invalid.MultiAsset.ManagedV2.ExecutionContext.BlockNumber = 101
		if field == "collection" {
			invalid.MultiAsset.ManagedV2.LastCollectionTimestamp = 1002
		} else {
			invalid.MultiAsset.ManagedV2.TokenInfo[0].LastChangeBlock = 102
		}
		if invalid.Validate() == nil {
			t.Fatal("context precedes updated ledger", field)
		}
	}
	for _, invalid := range []string{`null`, `{}`, `{"block_number":101}`, `{"block_number":101,"timestamp":null}`, `{"block_number":101,"timestamp":1001,"unknown":0}`, `{"block_number":99,"timestamp":1001}`, `{"block_number":101,"timestamp":999}`, `{"block_number":101,"timestamp":18446744073709551615}`, `{"block_number":101,"timestamp":1001,"Timestamp":1002}`, `{"block_number":101,"timestamp":1001,"timestamp":1002}`} {
		candidate := strings.Replace(raw, `{"block_number":4294967297,"timestamp":1001}`, invalid, 1)
		var v Pool
		if e := json.Unmarshal([]byte(candidate), &v); e == nil && v.Validate() == nil {
			t.Fatal("invalid context accepted", invalid)
		}
	}
	equal := strings.Replace(raw, `{"block_number":4294967297,"timestamp":1001}`, `{"block_number":100,"timestamp":1000}`, 1)
	if e := json.Unmarshal([]byte(equal), &p); e != nil {
		t.Fatal(e)
	}
	if e := p.Validate(); e != nil {
		t.Fatal(e)
	}
	if managedV2Snapshot(t, p).Validate("test", 1) == nil {
		t.Fatal("source-equal derived snapshot accepted")
	}
	if _, e := ManagedV2ToProto(p.MultiAsset.ManagedV2); e == nil {
		t.Fatal("source-equal wire discarded context")
	}
}

func TestManagedV2ActualVaultWalletLedger(t *testing.T) {
	var value map[string]any
	if err := json.Unmarshal([]byte(managedV2SyntheticJSON), &value); err != nil {
		t.Fatal(err)
	}
	m := value["multi_asset"].(map[string]any)["managed_v2"].(map[string]any)
	m["vault_bpt_actual_balance"] = "200"
	for _, item := range m["token_info"].([]any) {
		item.(map[string]any)["vault_actual_balance"] = "2000"
	}
	decode := func(v map[string]any) error {
		raw, err := json.Marshal(v)
		if err != nil {
			return err
		}
		var p Pool
		if err = json.Unmarshal(raw, &p); err != nil {
			return err
		}
		return p.Validate()
	}
	if err := decode(value); err != nil {
		t.Fatal("legitimate global wallets above this pool's cash rejected:", err)
	}
	cases := map[string]func(map[string]any){
		"underlying below cash": func(m map[string]any) { m["token_info"].([]any)[0].(map[string]any)["vault_actual_balance"] = "699" },
		"underlying overflow": func(m map[string]any) {
			m["token_info"].([]any)[0].(map[string]any)["vault_actual_balance"] = new(big.Int).Lsh(big.NewInt(1), 256).String()
		},
		"BPT below registered cash":   func(m map[string]any) { m["vault_bpt_actual_balance"] = "99" },
		"BPT wallets exceed supply":   func(m map[string]any) { m["vault_bpt_actual_balance"] = "201" },
		"Vault manager alias differs": func(m map[string]any) { m["manager"] = value["multi_asset"].(map[string]any)["vault"] },
	}
	original, _ := json.Marshal(value)
	for name, mutate := range cases {
		t.Run(name, func(t *testing.T) {
			var v map[string]any
			if err := json.Unmarshal(original, &v); err != nil {
				t.Fatal(err)
			}
			mutate(v["multi_asset"].(map[string]any)["managed_v2"].(map[string]any))
			if decode(v) == nil {
				t.Fatal("invalid actual wallet accepted")
			}
		})
	}
	m["manager"] = value["multi_asset"].(map[string]any)["vault"]
	m["protocol_collector"] = m["manager"]
	m["manager_bpt_balance"] = "200"
	m["protocol_collector_bpt_balance"] = "200"
	if err := decode(value); err != nil {
		t.Fatal("same Vault wallet counted more than once:", err)
	}
}

func TestManagedV2RejectsOlderWalletlessAdapter(t *testing.T) {
	p := managedV2Synthetic(t)
	p.AdapterVersion = "balancer-v2/20230411-managed-aum-1"
	if p.Validate() == nil {
		t.Fatal("accepted older adapter without required wallet semantics")
	}
}

func TestManagedV2SeparatePauseWindowsAndEffectiveClock(t *testing.T) {
	for _, vault := range []bool{false, true} {
		t.Run(fmt.Sprintf("vault=%t", vault), func(t *testing.T) {
			p := managedV2Synthetic(t)
			m := p.MultiAsset.ManagedV2
			m.PauseWindowEnd, m.BufferPeriodEnd = 999, 1000
			m.VaultPauseWindowEnd, m.VaultBufferPeriodEnd = 998, 1000
			if vault {
				m.VaultPaused = true
			} else {
				m.Paused = true
			}
			if e := p.Validate(); e != nil {
				t.Fatal("equal buffer end is still paused", e)
			}
			if _, e := MultiAssetToProto(p.MultiAsset); e != nil {
				t.Fatal(e)
			}
			m.Timestamp = 1001
			if p.Validate() == nil {
				t.Fatal("accepted expired source effective flag")
			}
			if _, e := MultiAssetToProto(p.MultiAsset); e == nil {
				t.Fatal("wire accepted expired source effective flag")
			}
			m.Timestamp = 1000
			m.ExecutionContext = &ManagedV2ExecutionContext{BlockNumber: 101, Timestamp: 1001}
			if p.Validate() == nil {
				t.Fatal("accepted effective flag expired at derived clock")
			}
			if vault {
				m.VaultPaused = false
			} else {
				m.Paused = false
			}
			if e := p.Validate(); e != nil {
				t.Fatal("cleared derived flag", e)
			}
			if m.Timestamp != 1000 || m.BlockNumber != 100 {
				t.Fatal("source H changed")
			}
		})
	}
	for _, vault := range []bool{false, true} {
		p := managedV2Synthetic(t)
		if vault {
			p.MultiAsset.ManagedV2.VaultPauseWindowEnd = 1
		} else {
			p.MultiAsset.ManagedV2.PauseWindowEnd = 1
		}
		if p.Validate() == nil {
			t.Fatal("accepted unordered pause windows", vault)
		}
	}
}
func TestManagedV2VaultPauseWirePresenceAndTags(t *testing.T) {
	p := managedV2Synthetic(t)
	m := p.MultiAsset.ManagedV2
	m.VaultPaused = true
	m.VaultPauseWindowEnd, m.VaultBufferPeriodEnd = 999, 1000
	w, e := MultiAssetToProto(p.MultiAsset)
	if e != nil {
		t.Fatal(e)
	}
	for name, tag := range map[string]int{"vault_paused": 37, "vault_pause_window_end": 38, "vault_buffer_period_end": 39} {
		field := w.ManagedV2.ProtoReflect().Descriptor().Fields().ByName(protoreflect.Name(name))
		if field == nil || int(field.Number()) != tag || !field.HasPresence() {
			t.Fatal("vault scalar descriptor", name)
		}
		q := proto.Clone(w).(*pb.MultiAssetState)
		q.ManagedV2.ProtoReflect().Clear(field)
		if _, e := MultiAssetFromProto(q); e == nil {
			t.Fatal("accepted absent vault wire scalar", name)
		}
	}
	q, e := MultiAssetFromProto(w)
	if e != nil || !reflect.DeepEqual(q, p.MultiAsset) {
		t.Fatal("separate pause wire roundtrip", e)
	}
	for _, version := range []string{"balancer-v2/20230411-managed-aum-1", "balancer-v2/20230411-managed-aum-2"} {
		p.AdapterVersion = version
		if p.Validate() == nil {
			t.Fatal("accepted pre-separation adapter", version)
		}
	}
}

func TestManagedV2CanonicalOriginalStorageAndRegistrationWidths(t *testing.T) {
	s, _, _ := managedV2WireGolden(t)
	for name, mutate := range map[string]func(*BalancerManagedV2State){
		"BPT index": func(m *BalancerManagedV2State) { m.BPTIndex = 1 },
		"BPT managed": func(m *BalancerManagedV2State) {
			m.VaultBPTCash = Amount(new(big.Int).Sub(m.VaultBPTCash.Big(), big.NewInt(1)).String())
			m.VaultBPTManaged = "1"
		},
		"provider uint8":          func(m *BalancerManagedV2State) { m.ProviderFeeID = "256" },
		"recovery nonzero getter": func(m *BalancerManagedV2State) { m.RecoveryMode = true },
		"recovery nonzero pending": func(m *BalancerManagedV2State) {
			m.RecoveryMode = true
			m.ManagementAUMFee = "0"
			m.PendingAUM = "1"
			m.ActualSupply = Amount(new(big.Int).Add(m.VirtualSupply.Big(), big.NewInt(1)).String())
		},
		"zero fee nonzero pending": func(m *BalancerManagedV2State) {
			m.ManagementAUMFee = "0"
			m.PendingAUM = "1"
			m.ActualSupply = Amount(new(big.Int).Add(m.VirtualSupply.Big(), big.NewInt(1)).String())
		},
		"underlying without manager": func(m *BalancerManagedV2State) {
			m.TokenInfo[0].Cash = Amount(new(big.Int).Sub(m.TokenInfo[0].Cash.Big(), big.NewInt(1)).String())
			m.TokenInfo[0].Managed = "1"
		},
	} {
		t.Run(name, func(t *testing.T) {
			bad := s
			bad.Pools = []Pool{s.Pools[0].Clone()}
			mutate(bad.Pools[0].MultiAsset.ManagedV2)
			if bad.Pools[0].Validate() == nil {
				t.Fatal("impossible original registration/storage accepted as Pool")
			}
			if bad.Validate(s.Reference.OwnerService, s.Reference.OwnerAssignmentVersion) == nil {
				t.Fatal("impossible original registration/storage accepted as Snapshot")
			}
			w, err := MultiAssetToProto(bad.Pools[0].MultiAsset)
			if err == nil {
				if _, err = MultiAssetFromProto(w); err == nil {
					t.Fatal("impossible original registration/storage accepted over wire")
				}
			}
		})
	}
}
