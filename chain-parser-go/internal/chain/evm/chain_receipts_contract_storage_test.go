package evm

import (
	"context"
	"crypto/sha1"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/rpc"
)

type contractFixtureBinding struct {
	Logical  string `json:"logical_slot"`
	Physical string `json:"physical_slot"`
}
type contractFixtureAccount struct {
	Address string
	Proof   json.RawMessage
}
type contractStorageFixture struct {
	full     json.RawMessage
	headers  map[string]json.RawMessage
	target   RawBlock
	anchor   ExecutionStateAnchor
	source   ContractStorageSource
	accounts []contractFixtureAccount
	bindings map[string][]contractFixtureBinding
	receipts []Receipt
	proofs   atomic.Int32
	lastKeys []string
}

func loadContractStorageFixture(t *testing.T) *contractStorageFixture {
	t.Helper()
	base := "../../../test/fixtures/native-contract-storage/testdata/avalanche-07"
	read := func(name string) []byte {
		t.Helper()
		b, e := os.ReadFile(filepath.Join(base, name))
		if e != nil {
			t.Fatal(e)
		}
		return b
	}
	var provenance struct {
		Commit string `json:"upstream_commit"`
		Blob   string `json:"partition_git_blob"`
		Files  []struct {
			Path  string
			Bytes int
			SHA   string `json:"sha256"`
		}
	}
	if e := json.Unmarshal(read("provenance.json"), &provenance); e != nil {
		t.Fatal(e)
	}
	if provenance.Commit != "983815fd993acf846a27749b598f0fea4d876a75" {
		t.Fatal("primary pin changed")
	}
	for _, row := range provenance.Files {
		b := read(row.Path)
		h := sha256.Sum256(b)
		if len(b) != row.Bytes || hex.EncodeToString(h[:]) != row.SHA {
			t.Fatal("public source fixture changed", row.Path)
		}
	}
	primary := read("primary/coreth_partition.txt")
	blob := sha1.Sum(append([]byte(fmt.Sprintf("blob %d\x00", len(primary))), primary...))
	if hex.EncodeToString(blob[:]) != provenance.Blob || provenance.Blob != "aa0a63bd3fcac7e435048c23850a03a852417310" || !strings.Contains(string(primary), "key[0] &^= 0x01") {
		t.Fatal("partition source identity changed")
	}
	var input struct {
		Block    json.RawMessage
		Expected string                   `json:"expected_block_hash"`
		Headers  []json.RawMessage        `json:"native_headers"`
		Accounts []contractFixtureAccount `json:"deployments"`
	}
	if e := json.Unmarshal(read("prestate-evidence.json"), &input); e != nil {
		t.Fatal(e)
	}
	f := &contractStorageFixture{full: input.Block, headers: map[string]json.RawMessage{}, accounts: input.Accounts, bindings: map[string][]contractFixtureBinding{}}
	genesis, _ := knownBodyGenesis("evm:43114")
	f.source = ContractStorageSource{ChainKey: "evm:43114", GenesisHash: genesis}
	var previous *ChainHeaderCommitment
	for _, raw := range input.Headers {
		var fields struct{ Transactions []json.RawMessage }
		json.Unmarshal(raw, &fields)
		if len(fields.Transactions) > 0 && fields.Transactions[0][0] == '{' {
			var e error
			raw, _, e = projectFullRPCBlock(raw)
			if e != nil {
				t.Fatal(e)
			}
		}
		h, e := VerifyChainHeaderJSON(context.Background(), f.source.ChainKey, raw)
		if e != nil {
			t.Fatal(e)
		}
		n, _ := canonicalIndex(h.Block.Number)
		if previous != nil {
			p, _ := canonicalIndex(previous.Block.Number)
			if n != p+1 || h.Block.ParentHash != previous.Block.Hash {
				t.Fatal("native successor ancestry")
			}
		}
		if n == 99661976 {
			f.target = h.Block
			if h.Block.Hash != input.Expected {
				t.Fatal("target identity")
			}
		}
		if h.SettledHeight != nil && *h.SettledHeight == 99661976 {
			f.anchor = ExecutionStateAnchor{AccountEncodingID: "coreth-account-held-xchain-rlp/v1", Height: 99661976, BlockHash: input.Expected, StateRoot: h.StateRoot, SettlingHeight: n, SettlingHash: h.Block.Hash, Authority: "hashed-native-successor/public-fixture"}
		}
		f.headers[h.Block.Number] = raw
		copy := h
		previous = &copy
	}
	if f.anchor.SettlingHeight != 99661982 {
		t.Fatal("missing exact settled anchor")
	}
	for _, account := range input.Accounts {
		var b struct {
			Address  string
			Height   uint64
			Rule     string
			Mappings []contractFixtureBinding
		}
		if e := json.Unmarshal(read("storage-key-bindings/"+account.Address+".json"), &b); e != nil {
			t.Fatal(e)
		}
		if b.Address != account.Address || b.Height != 99661976 || b.Rule != "coreth-partitioned-storage/v1" {
			t.Fatal("bad captured binding")
		}
		f.bindings[account.Address] = b.Mappings
	}
	var capture struct{ Response struct{ Result []Receipt } }
	if e := json.Unmarshal(read("canonical-native-receipts.json"), &capture); e != nil {
		t.Fatal(e)
	}
	f.receipts = capture.Response.Result
	return f
}
func (f *contractStorageFixture) adapter(t *testing.T) *Adapter {
	t.Helper()
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var q struct {
			ID     any
			Method string
			Params []json.RawMessage
		}
		if e := json.NewDecoder(r.Body).Decode(&q); e != nil {
			t.Error(e)
			return
		}
		var result any
		switch q.Method {
		case "eth_chainId":
			result = "0xa86a"
		case "eth_getBlockByNumber":
			var tag string
			var full bool
			json.Unmarshal(q.Params[0], &tag)
			json.Unmarshal(q.Params[1], &full)
			if tag == "0x0" {
				result = map[string]string{"hash": f.source.GenesisHash}
			} else if full && tag == f.target.Number {
				result = f.full
			} else {
				result = f.headers[tag]
			}
		case "eth_getBlockReceipts":
			result = f.receipts
		case "eth_getProof":
			f.proofs.Add(1)
			var address string
			var selector struct {
				BlockHash        string
				RequireCanonical bool
			}
			json.Unmarshal(q.Params[0], &address)
			json.Unmarshal(q.Params[1], &f.lastKeys)
			json.Unmarshal(q.Params[2], &selector)
			if selector.BlockHash != f.target.Hash || !selector.RequireCanonical {
				t.Error("proof not pinned to execution target")
			}
			for _, account := range f.accounts {
				if account.Address == address {
					var p emptyStateProof
					json.Unmarshal(account.Proof, &p)
					selected := map[string]bool{}
					for _, k := range f.lastKeys {
						if selected[k] {
							t.Error("physical RPC duplicate")
						}
						selected[k] = true
					}
					slots := []emptyStateSlot{}
					for _, s := range p.StorageProof {
						if selected[s.Key] {
							slots = append(slots, s)
						}
					}
					p.StorageProof = slots
					result = p
					break
				}
			}
		default:
			t.Error("unexpected RPC", q.Method)
		}
		json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": q.ID, "result": result})
	}))
	t.Cleanup(server.Close)
	chain := config.Chain{Key: f.source.ChainKey, Genesis: f.source.GenesisHash, BlockIntegrity: ChainNativeCommitmentsV2, ReceiptsMode: "bulk-auto", NativeProof: &config.NativeProofConfig{MaxBytes: 16 << 20, MaxHeaders: 64, MaxWindowBlocks: 16, MaxTransactions: 4096, MaxLookahead: 32}}
	client := rpc.New([]string{server.URL}, 20*time.Second, 16<<20)
	client.SetRecorder(func(ctx context.Context, _ string, _ string, _ json.RawMessage, _ []byte) error {
		if ctx.Value(nativeDutyKey{}) == nil {
			return fmt.Errorf("contract read lost duty context")
		}
		return nil
	})
	return &Adapter{Client: client, Chain: chain}
}
func TestContractStoragePublicNativeReplayAndSessionConsumer(t *testing.T) {
	f := loadContractStorageFixture(t)
	a := f.adapter(t)
	ctx := context.WithValue(context.Background(), nativeDutyKey{}, "contract-storage")
	block, err := a.Block(ctx, f.target.Number)
	if err != nil {
		t.Fatal(err)
	}
	block, _, err = a.ReadQualifiedReceipts(ctx, block, true)
	if err != nil {
		t.Fatal(err)
	}
	count, shifted := 0, 0
	type row struct{ Address, StateRoot, StorageRoot, Logical, Physical, TriePath, Value string }
	rows := []row{}
	for _, account := range f.accounts {
		keys := []string{}
		for _, b := range f.bindings[account.Address] {
			keys = append(keys, b.Logical)
		}
		replay, err := VerifyContractAccountProof(ctx, f.source, f.anchor, keys, account.Proof, 4<<20)
		if err != nil {
			t.Fatal("public native logical replay", account.Address, err)
		}
		before := block.nativeSession.remaining
		actual, err := a.ReadNativeContractAccountProof(ctx, block, account.Address, keys)
		if err != nil {
			t.Fatal("normal contract account reader", account.Address, err)
		}
		if actual.StorageEncodingID != "coreth-partitioned-storage/v1" || actual.Scope != "contract-storage-only" || before-block.nativeSession.remaining != actual.ProofBytesUsed || len(actual.Bindings) != len(keys) {
			t.Fatal("session/typed rule accounting", account.Address)
		}
		for i, b := range actual.Bindings {
			captured := f.bindings[account.Address][i]
			if b.LogicalKey != captured.Logical || b.PhysicalKey != captured.Physical || b.TriePath != crypto.Keccak256Hash(common.FromHex(captured.Physical)).Hex() {
				t.Fatal("logical/physical/path evidence mismatch", b, captured)
			}
			value, err := actual.StorageValue(captured.Logical)
			expected, why := replay.StorageValue(captured.Logical)
			if err != nil || why != nil || value != expected {
				t.Fatal("logical query differs from actual replay", value, expected, err, why)
			}
			if b.LogicalKey != b.PhysicalKey {
				shifted++
			}
			count++
			rows = append(rows, row{account.Address, actual.Account.StateAnchor.StateRoot, actual.Account.StorageRoot, b.LogicalKey, b.PhysicalKey, b.TriePath, value})
		}
	}
	if len(f.accounts) != 13 || count != 32 || shifted == 0 || f.proofs.Load() != 13 {
		t.Fatal("public consumption scope changed", len(f.accounts), count, shifted, f.proofs.Load())
	}
	if err = a.RecheckQualifiedBlock(ctx, block); err != nil {
		t.Fatal(err)
	}
	if path := os.Getenv("XONE_CONTRACT_STORAGE_REPORT"); path != "" {
		raw, _ := json.MarshalIndent(struct {
			Source                       ContractStorageSource
			Anchor                       ExecutionStateAnchor
			Accounts, Slots, Transformed int
			Rows                         []row
		}{f.source, f.anchor, len(f.accounts), count, shifted, rows}, "", "  ")
		if err = os.WriteFile(path, append(raw, '\n'), 0600); err != nil {
			t.Fatal(err)
		}
	}
}

func TestContractStorageAliasIdentityAndSealedQueries(t *testing.T) {
	source := ContractStorageSource{ChainKey: "evm:43114"}
	source.GenesisHash, _ = knownBodyGenesis(source.ChainKey)
	anchor, proof := syntheticStateProof(t, "coreth-account-held-xchain-rlp/v1", true, false)
	raw, _ := json.Marshal(proof)
	alias := "0x01" + strings.Repeat("00", 30) + "01"
	result, err := VerifyContractAccountProof(context.Background(), source, anchor, []string{"0x1", alias, "0x3"}, raw, 1<<20)
	if err != nil {
		t.Fatal(err)
	}
	if len(result.Bindings) != 3 || result.Bindings[0].PhysicalKey != result.Bindings[1].PhysicalKey {
		t.Fatal("logical aliases were not preserved/deduplicated")
	}
	for _, key := range []string{"0x1", alias} {
		v, e := result.StorageValue(key)
		if e != nil || v != "0x7" {
			t.Fatal(key, v, e)
		}
	}
	result.StorageEncodingID = "identity"
	result.Scope = "asset"
	result.Bindings[1].PhysicalKey = common.HexToHash("0x2").Hex()
	result.LogicalStorageValues[alias] = "0x99"
	result.Account.StorageRoot = types.EmptyRootHash.Hex()
	if v, e := result.StorageValue(alias); e != nil || v != "0x7" {
		t.Fatal("editable projection changed contract authority", v, e)
	}
	if _, e := result.StorageValue("0x2"); e == nil {
		t.Fatal("unproved nonempty-root logical key accepted")
	}
	encoded, _ := json.Marshal(result)
	var unverified VerifiedContractAccountCommitment
	json.Unmarshal(encoded, &unverified)
	if _, e := unverified.StorageValue(alias); e == nil {
		t.Fatal("serialized logical projection became proof")
	}
	for _, key := range []string{"evm:1", "evm:10", "evm:56", "evm:8453", "evm:137", "evm:42161"} {
		source := ContractStorageSource{ChainKey: key}
		source.GenesisHash, _ = knownBodyGenesis(key)
		anchor, proof := syntheticStateProof(t, "ethereum-account-rlp/v1", true, false)
		raw, _ := json.Marshal(proof)
		result, e := VerifyContractAccountProof(context.Background(), source, anchor, []string{"0x1", "0x3"}, raw, 1<<20)
		if e != nil {
			t.Fatal(key, e)
		}
		for _, b := range result.Bindings {
			if b.LogicalKey != b.PhysicalKey {
				t.Fatal("non-Coreth logical storage changed", key, b)
			}
		}
		if _, e = result.StorageValue(alias); e == nil {
			t.Fatal("non-Coreth query silently applied Coreth mapping", key)
		}
	}
}

func TestContractStorageEncodingShapeEmptyRootAndBudgets(t *testing.T) {
	source := ContractStorageSource{ChainKey: "evm:43114"}
	source.GenesisHash, _ = knownBodyGenesis(source.ChainKey)
	for _, fault := range []string{"duplicate-logical", "too-many-keys", "bad-key", "long-key", "unknown-source", "wrong-genesis", "unknown-encoding", "wrong-chain-encoding", "extra-proof-key", "physical-duplicate", "nonzero-empty", "budget", "canceled"} {
		t.Run(fault, func(t *testing.T) {
			a, p := syntheticStateProof(t, "coreth-account-held-xchain-rlp/v1", true, true)
			p.StorageProof = []emptyStateSlot{}
			src := source
			keys := []string{"0x1"}
			limit := 1 << 20
			ctx := context.Background()
			switch fault {
			case "duplicate-logical":
				keys = []string{"0x1", common.HexToHash("0x1").Hex()}
			case "too-many-keys":
				keys = make([]string, 4097)
			case "bad-key":
				keys = []string{"0x00"}
			case "long-key":
				keys = []string{"0x" + strings.Repeat("ff", 33)}
			case "unknown-source":
				src.ChainKey = "evm:12345"
			case "wrong-genesis":
				src.GenesisHash = common.Hash{}.Hex()
			case "unknown-encoding":
				a.AccountEncodingID = "future-account/v9"
			case "wrong-chain-encoding":
				a.AccountEncodingID = "ethereum-account-rlp/v1"
			case "extra-proof-key":
				p.StorageProof = []emptyStateSlot{{Key: "0x2", Value: "0x0", Proof: []string{}}}
			case "physical-duplicate":
				p.StorageProof = []emptyStateSlot{{Key: "0x1", Value: "0x0", Proof: []string{}}, {Key: common.HexToHash("0x1").Hex(), Value: "0x0", Proof: []string{}}}
			case "nonzero-empty":
				p.StorageProof = []emptyStateSlot{{Key: "0x1", Value: "0x1", Proof: []string{}}}
			case "budget":
				limit = 128
			case "canceled":
				var cancel context.CancelFunc
				ctx, cancel = context.WithCancel(ctx)
				cancel()
			}
			raw, _ := json.Marshal(p)
			if _, err := VerifyContractAccountProof(ctx, src, a, keys, raw, limit); err == nil {
				t.Fatal("contract source/proof guard bypassed", fault)
			}
		})
	}
	a, p := syntheticStateProof(t, "coreth-account-held-xchain-rlp/v1", false, true)
	p.StorageProof = []emptyStateSlot{}
	raw, _ := json.Marshal(p)
	logical := "0x" + strings.Repeat("ff", 32)
	result, err := VerifyContractAccountProof(context.Background(), source, a, []string{logical}, raw, 1<<20)
	if err != nil {
		t.Fatal(err)
	}
	if result.Bindings[0].PhysicalKey != "0xfe"+strings.Repeat("ff", 31) {
		t.Fatal("wrong bit/byte was transformed", result.Bindings[0])
	}
	if value, err := result.StorageValue("0x200"); err != nil || value != "0x0" {
		t.Fatal("unlisted empty contract root not zero", value, err)
	}
	for _, slack := range []int{-1, 0, 1} {
		out, e := VerifyContractAccountProof(context.Background(), source, a, []string{logical}, raw, result.ProofBytesUsed+slack)
		if slack < 0 {
			if e == nil {
				t.Fatal("pure contract proof overspent")
			}
		} else if e != nil || out.ProofBytesUsed != result.ProofBytesUsed {
			t.Fatal("pure contract exact budget rejected", slack, e)
		}
	}
}

func TestContractStorageNormalSessionAliasBudgetAndContext(t *testing.T) {
	f := loadContractStorageFixture(t)
	a := f.adapter(t)
	ctx := context.WithValue(context.Background(), nativeDutyKey{}, "contract-session")
	block, err := a.Block(ctx, f.target.Number)
	if err != nil {
		t.Fatal(err)
	}
	block, _, err = a.ReadQualifiedReceipts(ctx, block, true)
	if err != nil {
		t.Fatal(err)
	}
	address := "0xb31f66aa3c1e785363f0875a1b74e27b85fd66c7"
	keys := []string{}
	for _, b := range f.bindings[address] {
		keys = append(keys, b.Logical)
	}
	keys = append(keys, f.bindings[address][0].Physical)
	before := block.nativeSession.remaining
	result, err := a.ReadNativeContractAccountProof(ctx, block, address, keys)
	if err != nil {
		t.Fatal(err)
	}
	if len(result.Bindings) != len(keys) || len(f.lastKeys) != len(keys)-1 || before-block.nativeSession.remaining != result.ProofBytesUsed {
		t.Fatal("physical alias not deduplicated in normal RPC/session")
	}
	for _, slack := range []int{-1, 0, 1} {
		block.nativeSession.mu.Lock()
		block.nativeSession.remaining = result.ProofBytesUsed + slack
		block.nativeSession.mu.Unlock()
		out, e := a.ReadNativeContractAccountProof(ctx, block, address, keys)
		if slack < 0 {
			if e == nil || block.nativeSession.remaining != 0 {
				t.Fatal("failed contract proof refunded native budget", e, block.nativeSession.remaining)
			}
		} else if e != nil || out.ProofBytesUsed != result.ProofBytesUsed || block.nativeSession.remaining != slack {
			t.Fatal("contract proof exact shared budget rejected", slack, e, block.nativeSession.remaining)
		}
	}
	block.nativeSession.mu.Lock()
	block.nativeSession.remaining = 16 << 20
	block.nativeSession.mu.Unlock()
	calls := f.proofs.Load()
	if _, e := a.ReadNativeContractAccountProof(context.Background(), block, address, keys); e == nil {
		t.Fatal("detached responsibility context accepted")
	}
	canceled, cancel := context.WithCancel(ctx)
	cancel()
	if _, e := a.ReadNativeContractAccountProof(canceled, block, address, keys); e != context.Canceled {
		t.Fatal("cancellation cause lost", e)
	}
	if _, e := a.ReadNativeContractAccountProof(ctx, block, address, []string{"0x1", common.HexToHash("0x1").Hex()}); e == nil {
		t.Fatal("logical duplicates reached RPC")
	}
	if f.proofs.Load() != calls {
		t.Fatal("invalid contract requests performed proof IO")
	}
}

func TestContractStorageOldBadPhysicalProofCannotBeReinterpreted(t *testing.T) {
	raw, err := os.ReadFile("../../../test/fixtures/native-state-proof/testdata/avalanche-04/prestate-evidence.json")
	if err != nil {
		t.Fatal(err)
	}
	var input struct {
		Expected string                   `json:"expected_block_hash"`
		Headers  []json.RawMessage        `json:"native_headers"`
		Accounts []contractFixtureAccount `json:"deployments"`
	}
	if err = json.Unmarshal(raw, &input); err != nil {
		t.Fatal(err)
	}
	var anchor ExecutionStateAnchor
	for _, header := range input.Headers {
		var fields struct{ Transactions []json.RawMessage }
		json.Unmarshal(header, &fields)
		if len(fields.Transactions) > 0 && fields.Transactions[0][0] == '{' {
			header, _, err = projectFullRPCBlock(header)
			if err != nil {
				t.Fatal(err)
			}
		}
		verified, err := VerifyChainHeaderJSON(context.Background(), "evm:43114", header)
		if err != nil {
			t.Fatal(err)
		}
		if verified.SettledHeight != nil && *verified.SettledHeight == 99656725 {
			height, _ := canonicalIndex(verified.Block.Number)
			anchor = ExecutionStateAnchor{AccountEncodingID: "coreth-account-held-xchain-rlp/v1", Height: 99656725, BlockHash: input.Expected, StateRoot: verified.StateRoot, SettlingHeight: height, SettlingHash: verified.Block.Hash, Authority: "hashed-native-successor/old-negative-fixture"}
		}
	}
	if anchor.SettlingHeight != 99656733 {
		t.Fatal("old contradiction anchor changed")
	}
	source := ContractStorageSource{ChainKey: "evm:43114"}
	source.GenesisHash, _ = knownBodyGenesis(source.ChainKey)
	bad := map[string]bool{"0xb97ef9ef8734c71904d8002f8b6bc66dd9c48a6e": true, "0xd446eb1660f766d533beceef890df7a69d26f7d1": true, "0xfae3f424a0a47706811521e3ee268f00cfb5c45e": true, "0xb31f66aa3c1e785363f0875a1b74e27b85fd66c7": true}
	count := 0
	for _, account := range input.Accounts {
		if !bad[account.Address] {
			continue
		}
		count++
		var proof emptyStateProof
		json.Unmarshal(account.Proof, &proof)
		keys := []string{}
		for _, slot := range proof.StorageProof {
			keys = append(keys, slot.Key)
		}
		if _, err = VerifyExecutionAccountProof(context.Background(), anchor, account.Proof, 4<<20); err == nil || err.Error() != "STATE_STORAGE_VALUE_MISMATCH" {
			t.Fatal("physical negative changed", account.Address, err)
		}
		if _, err = VerifyContractAccountProof(context.Background(), source, anchor, keys, account.Proof, 4<<20); err == nil || err.Error() != "STATE_STORAGE_VALUE_MISMATCH" {
			t.Fatal("old proof was reinterpreted as a transformed-key proof", account.Address, err)
		}
	}
	if count != 4 {
		t.Fatal("old contradiction coverage changed", count)
	}
}

func TestContractStorageSessionCancellationAndRecorderFailureStayTerminal(t *testing.T) {
	for _, failure := range []string{"canceled-during-proof", "recorder"} {
		t.Run(failure, func(t *testing.T) {
			f := loadContractStorageFixture(t)
			a := f.adapter(t)
			parent := context.WithValue(context.Background(), nativeDutyKey{}, "contract-failure")
			ctx, cancel := context.WithCancel(parent)
			defer cancel()
			block, err := a.Block(ctx, f.target.Number)
			if err != nil {
				t.Fatal(err)
			}
			block, _, err = a.ReadQualifiedReceipts(ctx, block, true)
			if err != nil {
				t.Fatal(err)
			}
			address := "0xb31f66aa3c1e785363f0875a1b74e27b85fd66c7"
			keys := []string{}
			for _, binding := range f.bindings[address] {
				keys = append(keys, binding.Logical)
			}
			sentinel := errors.New("contract proof recording unavailable")
			if failure == "recorder" {
				a.Client.SetRecorder(func(ctx context.Context, _ string, method string, _ json.RawMessage, _ []byte) error {
					if ctx.Value(nativeDutyKey{}) != "contract-failure" {
						t.Error("proof recorder lost context")
					}
					if method == "eth_getProof" {
						return sentinel
					}
					return nil
				})
				_, err = a.ReadNativeContractAccountProof(ctx, block, address, keys)
				if !errors.Is(err, rpc.ErrRecordingFailed) || !errors.Is(err, sentinel) {
					t.Fatal("contract proof recorder failure lost", err)
				}
			} else {
				started := make(chan struct{})
				base := a.Client.HTTP.Transport
				a.Client.HTTP.Transport = proofBudgetTestTransport(func(request *http.Request) (*http.Response, error) {
					body, e := request.GetBody()
					if e != nil {
						return nil, e
					}
					var envelope struct{ Method string }
					json.NewDecoder(body).Decode(&envelope)
					body.Close()
					if envelope.Method == "eth_getProof" {
						close(started)
						<-request.Context().Done()
						return nil, request.Context().Err()
					}
					return base.RoundTrip(request)
				})
				done := make(chan error, 1)
				go func() { _, err := a.ReadNativeContractAccountProof(ctx, block, address, keys); done <- err }()
				select {
				case <-started:
				case <-time.After(5 * time.Second):
					t.Fatal("contract proof not reached")
				}
				cancel()
				select {
				case err = <-done:
				case <-time.After(5 * time.Second):
					t.Fatal("contract proof ignored cancellation")
				}
				if !errors.Is(err, context.Canceled) {
					t.Fatal("contract proof cancellation lost", err)
				}
			}
			if block.nativeSession.remaining != 0 {
				t.Fatal("failed proof scope refunded unknown/partial IO", block.nativeSession.remaining)
			}
		})
	}
}
