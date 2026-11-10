package evm

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"math/big"
	"os"
	"reflect"
	"strings"
	"testing"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
)

type bptThinOriginal struct {
	OriginalRaw struct {
		SHA256 string
		Bytes  int
	} `json:"original_raw"`
	Banks []struct {
		ID    string
		Pools []struct {
			Name string
			Pool domain.Pool
		}
	}
}

func bptOriginalData(t *testing.T) (scheduledSharedOriginal, bptThinOriginal) {
	t.Helper()
	raw, e := os.ReadFile("../../../test/contracts/managed-v2-bpt.json")
	if e != nil {
		t.Fatal(e)
	}
	data, e := os.ReadFile("../../../test/contracts/managed-v2-bpt-pools.json")
	if e != nil {
		t.Fatal(e)
	}
	var bank scheduledSharedOriginal
	var thin bptThinOriginal
	if e = json.Unmarshal(raw, &bank); e != nil {
		t.Fatal(e)
	}
	if e = json.Unmarshal(data, &thin); e != nil {
		t.Fatal(e)
	}
	h := sha256.Sum256(raw)
	if thin.OriginalRaw.SHA256 != hex.EncodeToString(h[:]) || thin.OriginalRaw.Bytes != len(raw) {
		t.Fatal("BPT projection source binding")
	}
	return bank, thin
}
func bptOriginalConfig(p domain.Pool) config.PoolConfig {
	s := p.MultiAsset
	m := s.ManagedV2
	c := &config.ManagedV2PoolConfig{ExpectedFactory: m.Factory, ExpectedFactoryCodeSHA256: m.FactoryCodeSHA256, ExpectedPoolRuntimeTemplateSHA256: m.PoolRuntimeTemplateSHA256, ExpectedFeeProvider: m.FeeProvider, ExpectedFeeProviderCodeSHA256: m.FeeProviderCodeSHA256, ExpectedProtocolCollector: m.ProtocolCollector, ExpectedProtocolCollectorCodeSHA256: m.ProtocolCollectorCodeSHA256, ExpectedManager: m.Manager, ExpectedProviderFeeID: m.ProviderFeeID, ExpectedManagementAUMFee: m.ManagementAUMFee, Dependencies: append([]domain.ManagedV2Dependency{}, m.Dependencies...)}
	// Supply finite observation addresses through JSON so the test first fails on
	// the old admission/parser, without depending on a not-yet-defined Go type.
	raw, _ := json.Marshal(c)
	var v map[string]any
	_ = json.Unmarshal(raw, &v)
	subjects := []string{}
	for _, member := range m.BPT.AllowlistMembers {
		subjects = append(subjects, member.Address)
	}
	v["bpt"] = map[string]any{"allowlist_subjects": subjects}
	raw, _ = json.Marshal(v)
	_ = json.Unmarshal(raw, c)
	multi := &config.MultiAssetConfig{ExpectedCodeSHA256: s.CodeSHA256, ExpectedVault: s.Vault, ExpectedVaultCodeSHA256: s.VaultCodeSHA256, PoolID: s.PoolID, TokenPolicy: "standard-bpt", ManagedV2: c}
	for _, a := range s.Assets {
		multi.Tokens = append(multi.Tokens, config.MultiTokenConfig{Token: a.Token, ExpectedCodeSHA256: a.CodeSHA256})
	}
	return config.PoolConfig{ID: p.PoolID, Protocol: p.Protocol, Token0: p.Token0, Token1: p.Token1, FeeNumerator: p.FeeNumerator, FeeDenominator: p.FeeDenominator, MultiAsset: multi}
}
func TestManagedV2BptEverySupportedOriginalFixedBank(t *testing.T) {
	native, thin := bptOriginalData(t)
	if len(native.Banks) != 145 || len(thin.Banks) != 145 {
		t.Fatal("BPT inventory")
	}
	count := 0
	for _, bank := range native.Banks {
		var expected []domain.Pool
		for _, row := range thin.Banks {
			if row.ID == bank.ID {
				for _, p := range row.Pools {
					expected = append(expected, p.Pool)
				}
				break
			}
		}
		t.Run(bank.ID, func(t *testing.T) {
			configs := []config.PoolConfig{}
			for _, p := range expected {
				configs = append(configs, bptOriginalConfig(p))
			}
			if len(configs) == 0 {
				t.Fatal("source bank lost all pools")
			}
			a := scheduledOriginalAdapter(t, native.Branches[bank.Namespace], scheduledSharedState(bank), configs[0])
			a.Chain.Pools = configs
			ref, actual, e := a.ReadState(context.Background(), bank.Header["number"].(string), false)
			if e != nil {
				t.Fatal(e)
			}
			if ref.Hash != bank.Header["hash"] || !reflect.DeepEqual(actual, expected) {
				got, _ := json.Marshal(actual)
				want, _ := json.Marshal(expected)
				t.Fatalf("complete fixed-H BPT state mismatch\ngot=%s\nwant=%s", got, want)
			}
		})
		count += len(expected)
	}
	if count != 286 {
		t.Fatal("supported physical Pool count", count)
	}
}

func TestManagedV2BptSourceControlsFailAtomically(t *testing.T) {
	native, thin := bptOriginalData(t)
	bank := native.Banks[0]
	row := thin.Banks[0]
	expected := row.Pools[0].Pool
	cfg := bptOriginalConfig(expected)
	p := cfg.ID
	subject := cfg.MultiAsset.ManagedV2.BPT.AllowlistSubjects[0]
	call := managedV2Selector("isAddressOnAllowlist(address)") + hex.EncodeToString(managedV2AddressWord(subject))
	encoded := append(managedV2AddressWord(subject), managedV2Word(big.NewInt(13))...)
	key := "0x" + hex.EncodeToString(managedV2Hash(string(encoded)))
	for _, kind := range []string{"member-getter-raw-mismatch", "member-storage-not-bool", "locked-minimum", "join-exit-getter", "registration-block-width", "aum-fee-raw", "aum-clock-raw", "aum-reserved-raw"} {
		t.Run(kind, func(t *testing.T) {
			state := scheduledSharedState(bank)
			state.Storage = append(state.Storage[:0:0], state.Storage...)
			changed := false
			switch kind {
			case "member-getter-raw-mismatch":
				for i := range state.Getters {
					g := &state.Getters[i]
					if g.To == p && g.Data == call {
						g.Result = "0x" + strings.Repeat("0", 63) + "1"
						changed = true
					}
				}
			case "member-storage-not-bool":
				for i := range state.Storage {
					word := &state.Storage[i]
					if word.Address == p && word.Key == key {
						word.Value = "0x2"
						changed = true
					}
				}
			case "aum-fee-raw", "aum-clock-raw", "aum-reserved-raw":
				bit := uint(0)
				if kind == "aum-clock-raw" {
					bit = 60
				}
				if kind == "aum-reserved-raw" {
					bit = 92
				}
				for i := range state.Storage {
					word := &state.Storage[i]
					if word.Address == p && word.Key == "0xa" {
						raw, ok := new(big.Int).SetString(strings.TrimPrefix(word.Value, "0x"), 16)
						if !ok {
							t.Fatal("original AUM word")
						}
						raw.Xor(raw, new(big.Int).Lsh(big.NewInt(1), bit))
						word.Value = "0x" + raw.Text(16)
						changed = true
					}
				}
			case "locked-minimum":
				target := managedV2Selector("balanceOf(address)") + hex.EncodeToString(managedV2AddressWord("0x0000000000000000000000000000000000000000"))
				for i := range state.Getters {
					g := &state.Getters[i]
					if g.To == p && g.Data == target {
						g.Result = "0x" + strings.Repeat("0", 64)
						changed = true
					}
				}
			case "join-exit-getter":
				for i := range state.Getters {
					g := &state.Getters[i]
					if g.To == p && g.Data == managedV2Selector("getJoinExitEnabled()") {
						g.Result = "0x" + strings.Repeat("0", 64)
						changed = true
					}
				}
			case "registration-block-width":
				target := managedV2Selector("getPoolTokenInfo(bytes32,address)") + cfg.MultiAsset.PoolID[2:] + hex.EncodeToString(managedV2AddressWord(p))
				for i := range state.Getters {
					g := &state.Getters[i]
					if g.To == cfg.MultiAsset.ExpectedVault && g.Data == target {
						word := managedV2Word(new(big.Int).Lsh(big.NewInt(1), 32))
						g.Result = g.Result[:2+128] + hex.EncodeToString(word) + g.Result[2+192:]
						changed = true
					}
				}
			}
			if !changed {
				t.Fatal("counterexample did not mutate original source")
			}
			a := scheduledOriginalAdapter(t, native.Branches[bank.Namespace], state, cfg)
			_, pools, e := a.ReadState(context.Background(), bank.Header["number"].(string), false)
			if e == nil || len(pools) != 0 {
				t.Fatal("contradictory BPT source published", e, len(pools))
			}
		})
	}
}

// Synthetic source consistency overlay, not a new original-VM recovery proof.
func TestManagedV2BptRecoveryRetainsStoredFeeAndUsesEffectiveZeroGetter(t *testing.T) {
	native, thin := bptOriginalData(t)
	bank := native.Banks[0]
	expected := thin.Banks[0].Pools[0].Pool.Clone()
	cfg := bptOriginalConfig(expected)
	if expected.MultiAsset.ManagedV2.PendingAUM != "0" {
		t.Fatal("initial source must have no elapsed AUM")
	}
	state := scheduledSharedState(bank)
	state.Storage = append(state.Storage[:0:0], state.Storage...)
	for i := range state.Getters {
		g := &state.Getters[i]
		if g.To != cfg.ID {
			continue
		}
		switch g.Method {
		case "inRecoveryMode":
			g.Result = "0x" + strings.Repeat("0", 63) + "1"
		case "getManagementAumFeeParams":
			g.Result = "0x" + strings.Repeat("0", 64) + g.Result[66:]
		}
	}
	for i := range state.Storage {
		word := &state.Storage[i]
		if word.Address == cfg.ID && word.Key == "0x9" {
			n, _ := new(big.Int).SetString(strings.TrimPrefix(word.Value, "0x"), 16)
			n.Or(n, new(big.Int).Lsh(big.NewInt(1), 254))
			word.Value = "0x" + n.Text(16)
		}
	}
	m := expected.MultiAsset.ManagedV2
	m.RecoveryMode = true
	m.ManagementAUMFee = "0"
	word := m.Schedule.PoolStateWord.Big()
	word.Or(word, new(big.Int).Lsh(big.NewInt(1), 254))
	m.Schedule.PoolStateWord = domain.Amount(word.String())
	a := scheduledOriginalAdapter(t, native.Branches[bank.Namespace], state, cfg)
	_, actual, e := a.ReadState(context.Background(), bank.Header["number"].(string), false)
	if e != nil || len(actual) != 1 || !reflect.DeepEqual(actual[0], expected) {
		t.Fatal("nonzero stored AUM fee is valid with effective recovery zero", e)
	}
	for i := range state.Storage {
		word := &state.Storage[i]
		if word.Address == cfg.ID && word.Key == "0xa" {
			n, _ := new(big.Int).SetString(strings.TrimPrefix(word.Value, "0x"), 16)
			n.Or(n, new(big.Int).Lsh(big.NewInt(1), 92))
			word.Value = "0x" + n.Text(16)
		}
	}
	a = scheduledOriginalAdapter(t, native.Branches[bank.Namespace], state, cfg)
	_, actual, e = a.ReadState(context.Background(), bank.Header["number"].(string), false)
	if e == nil || len(actual) != 0 {
		t.Fatal("recovery cannot hide invalid AUM reserved bits", e)
	}
}
