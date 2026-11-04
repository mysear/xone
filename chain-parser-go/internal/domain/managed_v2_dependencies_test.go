package domain

import (
	"encoding/json"
	"math/big"
	"os"
	"strings"
	"testing"
)

// Literal local source observations; cloned IDs and mutations have synthetic
// validation authority, never original shared-bank provenance.
func dependencySource(t *testing.T) Pool {
	t.Helper()
	raw, err := os.ReadFile("../../test/contracts/managed-v2-aum-pools.json")
	if err != nil {
		t.Fatal(err)
	}
	var bank struct {
		Cases []struct {
			Source Pool `json:"source_pool"`
		} `json:"cases"`
	}
	if err := json.Unmarshal(raw, &bank); err != nil {
		t.Fatal(err)
	}
	// Cloned registrations below are synthetic capacity declarations. Give
	// their shared Vault enough unallocated global cash without changing any
	// native registration or quoted pricing balance.
	p := bank.Cases[0].Source
	for i := range p.MultiAsset.ManagedV2.TokenInfo {
		info := &p.MultiAsset.ManagedV2.TokenInfo[i]
		info.VaultActualBalance = Amount(new(big.Int).Mul(info.Cash.Big(), big.NewInt(4)).String())
	}
	return p
}
func dependencyClone(t *testing.T, p Pool, id string) Pool {
	t.Helper()
	raw, err := json.Marshal(p)
	if err != nil {
		t.Fatal(err)
	}
	var q Pool
	if err := json.Unmarshal(raw, &q); err != nil {
		t.Fatal(err)
	}
	q.PoolID = id
	q.MultiAsset.PoolID = id + "000100000000000000000000"
	return q
}
func TestManagedDependenciesContradictions(t *testing.T) {
	for _, tc := range []struct {
		name, reason string
		mutate       func(*Pool)
	}{
		{"vault_code", "SHARED_VAULT_IDENTITY", func(p *Pool) { p.MultiAsset.VaultCodeSHA256 = strings.Repeat("b", 64) }},
		{"number", "SHARED_VAULT_SOURCE", func(p *Pool) { p.MultiAsset.ManagedV2.BlockNumber++ }},
		{"hash", "SHARED_VAULT_SOURCE", func(p *Pool) { p.MultiAsset.ManagedV2.BlockHash = "0x" + strings.Repeat("b", 64) }},
		{"time", "SHARED_VAULT_SOURCE", func(p *Pool) { p.MultiAsset.ManagedV2.Timestamp++ }},
		{"clock_number", "SHARED_VAULT_CLOCK", func(p *Pool) {
			m := p.MultiAsset.ManagedV2
			m.ExecutionContext = &ManagedV2ExecutionContext{BlockNumber: m.BlockNumber + 1, Timestamp: m.Timestamp}
		}},
		{"clock_time", "SHARED_VAULT_CLOCK", func(p *Pool) {
			m := p.MultiAsset.ManagedV2
			m.ExecutionContext = &ManagedV2ExecutionContext{BlockNumber: m.BlockNumber, Timestamp: m.Timestamp + 1}
		}},
		{"vault_paused", "SHARED_VAULT_PAUSE", func(p *Pool) { p.MultiAsset.ManagedV2.VaultPaused = true }},
		{"vault_pause_window", "SHARED_VAULT_PAUSE", func(p *Pool) { p.MultiAsset.ManagedV2.VaultPauseWindowEnd++ }},
		{"vault_buffer_window", "SHARED_VAULT_PAUSE", func(p *Pool) { p.MultiAsset.ManagedV2.VaultBufferPeriodEnd++ }},
		{"token_code", "SHARED_TOKEN_IDENTITY", func(p *Pool) { p.MultiAsset.Assets[0].CodeSHA256 = strings.Repeat("b", 64) }},
		{"scaling", "SHARED_TOKEN_IDENTITY", func(p *Pool) { p.MultiAsset.Assets[0].Scaling = "10000000000000000000" }},
		{"wallet", "SHARED_VAULT_WALLET", func(p *Pool) { p.MultiAsset.ManagedV2.TokenInfo[0].VaultActualBalance = "10000000000000000000001" }},
	} {
		t.Run(tc.name, func(t *testing.T) {
			p := dependencySource(t)
			q := dependencyClone(t, p, "0x0000000000000000000000000000000000000048")
			tc.mutate(&q)
			for _, row := range []Pool{p, q} {
				if e := row.Validate(); e != nil {
					t.Fatal("invalid individual", e)
				}
			}
			for _, rows := range [][]Pool{{p, q}, {q, p}} {
				if e := ValidatePoolDependencies(rows); e == nil || !strings.Contains(e.Error(), tc.reason) {
					t.Fatalf("wanted %s, got %v", tc.reason, e)
				}
				if _, e := ManagedV2ExecutionCapabilities(rows); e == nil || !strings.Contains(e.Error(), tc.reason) {
					t.Fatalf("capabilities wanted %s, got %v", tc.reason, e)
				}
			}
		})
	}
}

func dependencyUnblocked(t *testing.T, rows []Pool) {
	t.Helper()
	for _, p := range rows {
		if err := p.Validate(); err != nil {
			t.Fatal(err)
		}
	}
	if err := ValidateManagedV2PoolDependencies(rows); err != nil {
		t.Fatal(err)
	}
	if err := ValidatePoolDependencies(rows); err != nil {
		t.Fatal(err)
	}
	blocked, err := ManagedV2ExecutionCapabilities(rows)
	if err != nil || len(blocked) != 0 {
		t.Fatalf("blocked=%v error=%v", blocked, err)
	}
}
func dependencySnapshot(t *testing.T, rows []Pool) Snapshot {
	t.Helper()
	s := managedV2Snapshot(t, rows[0])
	s.Pools = rows
	var err error
	s.Reference.ScopeHash, err = ScopeHash(rows)
	if err != nil {
		t.Fatal(err)
	}
	return s
}
func dependencyWeighted(t *testing.T, p Pool) Pool {
	t.Helper()
	p = dependencyClone(t, p, "0x0000000000000000000000000000000000000048")
	p.Protocol = "balancer-v2-weighted-multi"
	p.AdapterVersion = "balancer-v2-weighted-multi/static-1"
	s := p.MultiAsset
	s.ManagedV2 = nil
	third := s.Assets[0]
	third.Token = "0xfffffffffffffffffffffffffffffffffffffffe"
	s.Assets = append(s.Assets, third)
	s.Weights = []Amount{"300000000000000000", "300000000000000000", "400000000000000000"}
	s.Assets[0].Scaling = "10000000000000000000"
	return p
}
func TestManagedDependenciesSourceContractAndLocalFacts(t *testing.T) {
	raw, err := os.ReadFile("../../test/contracts/managed-v2-aum-pools.json")
	if err != nil {
		t.Fatal(err)
	}
	var bank struct {
		Cases []struct {
			Source Pool `json:"source_pool"`
		} `json:"cases"`
	}
	if err := json.Unmarshal(raw, &bank); err != nil {
		t.Fatal(err)
	}
	for _, c := range bank.Cases {
		dependencyUnblocked(t, []Pool{c.Source})
		if err := dependencySnapshot(t, []Pool{c.Source}).Validate("test", 1); err != nil {
			t.Fatal(err)
		}
	}
	p := dependencySource(t)
	q := dependencyClone(t, p, "0x0000000000000000000000000000000000000048")
	m := q.MultiAsset.ManagedV2
	m.Paused = true
	m.PauseWindowEnd++
	m.BufferPeriodEnd++
	m.CachedProtocolAUMPercentage = "400000000000000000"
	rows := []Pool{p, q}
	dependencyUnblocked(t, rows)
	if err := dependencySnapshot(t, rows).Validate("test", 1); err != nil {
		t.Fatal(err)
	}
	q = dependencyClone(t, p, q.PoolID)
	m = q.MultiAsset.ManagedV2
	m.ExecutionContext = &ManagedV2ExecutionContext{BlockNumber: m.BlockNumber, Timestamp: m.Timestamp}
	rows = []Pool{p, q}
	dependencyUnblocked(t, rows)
	if err := dependencySnapshot(t, rows).Validate("test", 1); err == nil || !strings.Contains(err.Error(), "DERIVED_STATE_NOT_CANONICAL") {
		t.Fatal(err)
	}
	dependencyUnblocked(t, nil)
}
func TestManagedDependenciesChainVaultTokenIsolation(t *testing.T) {
	p := dependencySource(t)
	q := dependencyClone(t, p, "0x0000000000000000000000000000000000000048")
	s := q.MultiAsset
	s.Vault = "0x0000000000000000000000000000000000000049"
	s.VaultCodeSHA256 = strings.Repeat("b", 64)
	m := s.ManagedV2
	m.BlockNumber++
	m.Timestamp++
	m.BlockHash = "0x" + strings.Repeat("b", 64)
	m.VaultPaused = true
	m.TokenInfo[0].VaultActualBalance = "20000000000000000000000"
	dependencyUnblocked(t, []Pool{p, q})
	s.Assets[0].Scaling = "10000000000000000000"
	if err := ValidatePoolDependencies([]Pool{p, q}); err == nil || !strings.Contains(err.Error(), "SHARED_TOKEN_IDENTITY") {
		t.Fatal(err)
	}
	q.ChainKey = "evm:1"
	s.Vault = p.MultiAsset.Vault
	dependencyUnblocked(t, []Pool{p, q})
	if err := dependencySnapshot(t, []Pool{p, q}).Validate("test", 1); err == nil {
		t.Fatal("multi-chain snapshot accepted")
	}
	q = dependencyClone(t, p, q.PoolID)
	q.MultiAsset.Assets[0].Token = "0x0000000000000000000000000000000000000001"
	q.Token0 = q.MultiAsset.Assets[0].Token
	q.MultiAsset.ManagedV2.TokenInfo[0].VaultActualBalance = "20000000000000000000000"
	dependencyUnblocked(t, []Pool{p, q})
}
func TestManagedDependenciesIndividualManagerConstraintsFirst(t *testing.T) {
	// Existing synthetic helper exercises nonzero managed balances and manager.
	p := managedV2Synthetic(t)
	dependencyUnblocked(t, []Pool{p})
	p.MultiAsset.ManagedV2.TokenInfo[0].AssetManager = "0x0000000000000000000000000000000000000000"
	if err := ValidateManagedV2PoolDependencies([]Pool{p}); err == nil {
		t.Fatal("nonzero managed without manager")
	}
	if _, err := ManagedV2ExecutionCapabilities([]Pool{p}); err == nil {
		t.Fatal("capability bypassed individual validation")
	}
	p.MultiAsset.ManagedV2.TokenInfo[0].Managed = "0"
	p.MultiAsset.Assets[0].Balance = "700"
	p.Reserve0 = "700"
	dependencyUnblocked(t, []Pool{p})
	p.MultiAsset.ManagedV2.TokenInfo = p.MultiAsset.ManagedV2.TokenInfo[:1]
	if _, err := ManagedV2ExecutionCapabilities([]Pool{p}); err == nil {
		t.Fatal("unaligned token information")
	}
	p.MultiAsset.ManagedV2 = nil
	if _, err := ManagedV2ExecutionCapabilities([]Pool{p}); err == nil {
		t.Fatal("missing managed state")
	}
}
func TestManagedCapabilitiesMixedPreservesCanonicalBank(t *testing.T) {
	p := dependencySource(t)
	w := dependencyWeighted(t, p)
	other := dependencyClone(t, p, "0x0000000000000000000000000000000000000055")
	other.MultiAsset.Vault = "0x0000000000000000000000000000000000000056"
	rows := []Pool{p, w, other}
	for _, row := range rows {
		if err := row.Validate(); err != nil {
			t.Fatal(err)
		}
	}
	if err := dependencySnapshot(t, rows).Validate("test", 1); err != nil {
		t.Fatal(err)
	}
	for _, rows := range [][]Pool{rows, {other, w, p}} {
		blocked, err := ManagedV2ExecutionCapabilities(rows)
		if err != nil || len(blocked) != 2 {
			t.Fatalf("%v %v", blocked, err)
		}
		for _, id := range []string{p.PoolID, w.PoolID} {
			if blocked[id] != "SHARED_VAULT_MIXED_ADAPTER_UNSUPPORTED" {
				t.Fatal(blocked)
			}
		}
		if _, ok := blocked[other.PoolID]; ok {
			t.Fatal(blocked)
		}
	}
	w2 := dependencyClone(t, w, "0x0000000000000000000000000000000000000057")
	dependencyUnblocked(t, []Pool{w, w2})
	cross := dependencyClone(t, w, w.PoolID)
	cross.ChainKey = "evm:1"
	dependencyUnblocked(t, []Pool{p, cross})
	cross = dependencyClone(t, w, w.PoolID)
	cross.MultiAsset.Vault = other.MultiAsset.Vault
	dependencyUnblocked(t, []Pool{p, cross})
	w.MultiAsset.VaultCodeSHA256 = strings.Repeat("b", 64)
	if _, err := ManagedV2ExecutionCapabilities([]Pool{p, w}); err == nil || !strings.Contains(err.Error(), "SHARED_VAULT_IDENTITY") {
		t.Fatal(err)
	}
}
func TestManagedCapabilitiesBPTWholeGroupAndPrecedence(t *testing.T) {
	p := dependencySource(t)
	q := dependencyClone(t, p, "0x0000000000000000000000000000000000000048")
	p.MultiAsset.Assets[0].Token = q.PoolID
	p.MultiAsset.Assets[0].CodeSHA256 = q.MultiAsset.CodeSHA256
	p.Token0 = q.PoolID
	q.MultiAsset.ManagedV2.VaultBPTActualBalance = Amount(new(big.Int).Add(q.MultiAsset.ManagedV2.VaultBPTCash.Big(), p.MultiAsset.ManagedV2.TokenInfo[0].Cash.Big()).String())
	p.MultiAsset.ManagedV2.TokenInfo[0].VaultActualBalance = q.MultiAsset.ManagedV2.VaultBPTActualBalance
	w := dependencyWeighted(t, dependencySource(t))
	w = dependencyClone(t, w, "0x0000000000000000000000000000000000000057")
	other := dependencyClone(t, q, "0x0000000000000000000000000000000000000058")
	other.MultiAsset.Vault = "0x0000000000000000000000000000000000000059"
	rows := []Pool{p, q, w, other}
	if err := dependencySnapshot(t, rows).Validate("test", 1); err != nil {
		t.Fatal(err)
	}
	blocked, err := ManagedV2ExecutionCapabilities(rows)
	if err != nil || len(blocked) != 3 {
		t.Fatalf("%v %v", blocked, err)
	}
	for _, reason := range blocked {
		if reason != "SHARED_VAULT_BPT_UNSUPPORTED" {
			t.Fatal(blocked)
		}
	}
	if _, ok := blocked[other.PoolID]; ok {
		t.Fatal(blocked)
	}
	blocked, err = ManagedV2ExecutionCapabilities([]Pool{p, q})
	if err != nil || len(blocked) != 2 {
		t.Fatalf("%v %v", blocked, err)
	}
	p.MultiAsset.ManagedV2.TokenInfo[0].VaultActualBalance = "3000000000000000000000000000000000"
	if _, err := ManagedV2ExecutionCapabilities([]Pool{q, p}); err == nil || !strings.Contains(err.Error(), "SHARED_VAULT_WALLET") {
		t.Fatal(err)
	}
	q2 := dependencyClone(t, q, q.PoolID)
	q2.MultiAsset.Vault = other.MultiAsset.Vault
	dependencyUnblocked(t, []Pool{p, q2})
	q2 = dependencyClone(t, q, q.PoolID)
	q2.ChainKey = "evm:1"
	dependencyUnblocked(t, []Pool{p, q2})
}

func TestManagedCapabilitiesUnrelatedV2RemainsAvailable(t *testing.T) {
	raw, err := os.ReadFile("../../test/contracts/v2-complete.json")
	if err != nil {
		t.Fatal(err)
	}
	var bank struct {
		Cases []struct {
			Pool Pool `json:"pool"`
		} `json:"cases"`
	}
	if err := json.Unmarshal(raw, &bank); err != nil {
		t.Fatal(err)
	}
	v2 := bank.Cases[0].Pool
	if err := v2.Validate(); err != nil {
		t.Fatal(err)
	}
	p := dependencySource(t)
	w := dependencyWeighted(t, p)
	rows := []Pool{p, w, v2}
	if err := ValidatePoolDependencies(rows); err != nil {
		t.Fatal(err)
	}
	blocked, err := ManagedV2ExecutionCapabilities(rows)
	if err != nil || len(blocked) != 2 {
		t.Fatalf("%v %v", blocked, err)
	}
	if _, ok := blocked[v2.PoolID]; ok {
		t.Fatal(blocked)
	}
	if rows[2].PoolID != v2.PoolID || rows[2].V2 != v2.V2 {
		t.Fatal("source bank changed")
	}
}
func TestManagedDependenciesCommonDerivedClock(t *testing.T) {
	p := dependencySource(t)
	q := dependencyClone(t, p, "0x0000000000000000000000000000000000000048")
	rows := []Pool{p, q}
	for _, row := range rows {
		m := row.MultiAsset.ManagedV2
		m.ExecutionContext = &ManagedV2ExecutionContext{BlockNumber: m.BlockNumber + 1, Timestamp: m.Timestamp + 1}
	}
	dependencyUnblocked(t, rows)
	if err := dependencySnapshot(t, rows).Validate("test", 1); err == nil || !strings.Contains(err.Error(), "DERIVED_STATE_NOT_CANONICAL") {
		t.Fatal(err)
	}
}

func TestManagedSharedWalletCoversSumOfDeclaredRegistrations(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/managed-v2-shared-vault.json")
	if e != nil {
		t.Fatal(e)
	}
	var bank struct {
		Banks map[string]struct {
			Pools []struct {
				Pool Pool `json:"pool"`
			} `json:"pools"`
		} `json:"banks"`
	}
	if e = json.Unmarshal(raw, &bank); e != nil {
		t.Fatal(e)
	}
	var rows []Pool
	for _, row := range bank.Banks["H39"].Pools {
		rows = append(rows, row.Pool)
	}
	if len(rows) != 2 {
		t.Fatal("native shared bank incomplete")
	}
	if e = ValidatePoolDependencies(rows); e != nil {
		t.Fatal(e)
	}
	total := new(big.Int)
	for _, p := range rows {
		total.Add(total, p.MultiAsset.ManagedV2.TokenInfo[0].Cash.Big())
	}
	total.Sub(total, big.NewInt(1))
	for i := range rows {
		rows[i] = rows[i].Clone()
		rows[i].MultiAsset.ManagedV2.TokenInfo[0].VaultActualBalance = Amount(total.String())
		if e = rows[i].Validate(); e != nil {
			t.Fatal("each declaration is independently valid", e)
		}
	}
	if e = ValidatePoolDependencies(rows); e == nil {
		t.Fatal("shared actual wallet below known registration cash sum accepted")
	}
}

func TestManagedSharedRolesAndProviderGetterMustAgreeAtSameSource(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/managed-v2-shared-vault.json")
	if e != nil {
		t.Fatal(e)
	}
	var bank struct {
		Banks map[string]struct {
			Pools []struct {
				Pool Pool `json:"pool"`
			} `json:"pools"`
		} `json:"banks"`
	}
	if e = json.Unmarshal(raw, &bank); e != nil {
		t.Fatal(e)
	}
	p, q := bank.Banks["H38"].Pools[0].Pool, bank.Banks["H38"].Pools[1].Pool
	for name, mutate := range map[string]func(*BalancerManagedV2State){
		"factory code":         func(m *BalancerManagedV2State) { m.FactoryCodeSHA256 = strings.Repeat("b", 64) },
		"provider code":        func(m *BalancerManagedV2State) { m.FeeProviderCodeSHA256 = strings.Repeat("b", 64) },
		"collector code":       func(m *BalancerManagedV2State) { m.ProtocolCollectorCodeSHA256 = strings.Repeat("b", 64) },
		"provider live getter": func(m *BalancerManagedV2State) { m.LiveProviderAUMPercentage = "400000000000000000" },
		"dependency code":      func(m *BalancerManagedV2State) { m.Dependencies[0].CodeSHA256 = strings.Repeat("b", 64) },
		"dependency template":  func(m *BalancerManagedV2State) { m.Dependencies[0].TemplateSHA256 = strings.Repeat("b", 64) },
	} {
		t.Run(name, func(t *testing.T) {
			other := q.Clone()
			mutate(other.MultiAsset.ManagedV2)
			if e = other.Validate(); e != nil {
				t.Fatal(e)
			}
			for _, rows := range [][]Pool{{p, other}, {other, p}} {
				if e = ValidatePoolDependencies(rows); e == nil {
					t.Fatal("contradictory same-source shared role/getter accepted")
				}
			}
		})
	}
}
func TestManagedSharedDuplicateRegistrationCannotDependOnOrdering(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/managed-v2-shared-vault.json")
	if e != nil {
		t.Fatal(e)
	}
	var bank struct {
		Banks map[string]struct {
			Pools []struct {
				Pool Pool `json:"pool"`
			} `json:"pools"`
		} `json:"banks"`
	}
	if e = json.Unmarshal(raw, &bank); e != nil {
		t.Fatal(e)
	}
	p, q := bank.Banks["H38"].Pools[0].Pool, bank.Banks["H38"].Pools[1].Pool
	if e = ValidatePoolDependencies([]Pool{p, p, q}); e != nil {
		t.Fatal("identical registration must not be counted twice", e)
	}
	other := p.Clone()
	other.MultiAsset.ManagedV2.TokenInfo[0].Cash = Amount(new(big.Int).Add(other.MultiAsset.ManagedV2.TokenInfo[0].Cash.Big(), big.NewInt(1)).String())
	other.MultiAsset.Assets[0].Balance = Amount(new(big.Int).Add(other.MultiAsset.Assets[0].Balance.Big(), big.NewInt(1)).String())
	other.Reserve0 = other.MultiAsset.Assets[0].Balance
	if e = other.Validate(); e != nil {
		t.Fatal(e)
	}
	for _, rows := range [][]Pool{{p, other, q}, {other, p, q}} {
		if e = ValidatePoolDependencies(rows); e == nil {
			t.Fatal("conflicting duplicate physical registration accepted")
		}
	}
}
