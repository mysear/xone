package evm

// Test-only independent original-bytecode reference. The online parser does not
// build user routes or run a swap. Only proved state and declared local funding
// enter this overlay; no RPC or public transaction method exists in this runner.
import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"math/big"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/state"
	"github.com/ethereum/go-ethereum/core/tracing"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/core/vm"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/params"
	"github.com/holiman/uint256"
	"xone/chain-parser-go/internal/domain"
)

const publicExecutionRoot = "../../../test/fixtures/current-v2-execution-public"

var publicSender = common.HexToAddress("0x1111111111111111111111111111111111111111")
var publicRecipient = common.HexToAddress("0x2222222222222222222222222222222222222222")

type publicStorageKey struct {
	Address common.Address
	Key     common.Hash
}
type publicExecutionBank struct {
	Manifest      publicV2Manifest
	Accounts      map[common.Address]VerifiedAccountCommitment
	Codes         map[common.Address][]byte
	Slots         map[publicStorageKey]common.Hash
	Input, Quoted *big.Int
	ExtraRaw      []json.RawMessage
}
type publicExecutionStop struct{ err error }
type publicExecutionDB struct {
	*state.StateDB
	accounts map[common.Address]VerifiedAccountCommitment
	slots    map[publicStorageKey]common.Hash
}

func (d *publicExecutionDB) account(a common.Address) {
	if a == publicSender {
		return
	}
	if _, ok := d.accounts[a]; !ok {
		panic(publicExecutionStop{fmt.Errorf("UNPROVEN_EXECUTION_ACCOUNT:%s", a)})
	}
}
func (d *publicExecutionDB) slot(a common.Address, k common.Hash) {
	d.account(a)
	if _, ok := d.slots[publicStorageKey{a, k}]; !ok {
		panic(publicExecutionStop{fmt.Errorf("UNPROVEN_EXECUTION_STORAGE:%s:%s", a, k)})
	}
}
func (d *publicExecutionDB) GetState(a common.Address, k common.Hash) common.Hash {
	d.slot(a, k)
	return d.StateDB.GetState(a, k)
}
func (d *publicExecutionDB) GetStateAndCommittedState(a common.Address, k common.Hash) (common.Hash, common.Hash) {
	d.slot(a, k)
	return d.StateDB.GetStateAndCommittedState(a, k)
}
func (d *publicExecutionDB) SetState(a common.Address, k, v common.Hash) common.Hash {
	d.slot(a, k)
	return d.StateDB.SetState(a, k, v)
}
func (d *publicExecutionDB) GetBalance(a common.Address) *uint256.Int {
	d.account(a)
	return d.StateDB.GetBalance(a)
}
func (d *publicExecutionDB) GetNonce(a common.Address) uint64 {
	d.account(a)
	return d.StateDB.GetNonce(a)
}
func (d *publicExecutionDB) GetCode(a common.Address) []byte {
	d.account(a)
	return d.StateDB.GetCode(a)
}
func (d *publicExecutionDB) GetCodeSize(a common.Address) int {
	d.account(a)
	return d.StateDB.GetCodeSize(a)
}
func (d *publicExecutionDB) GetCodeHash(a common.Address) common.Hash {
	d.account(a)
	return d.StateDB.GetCodeHash(a)
}
func (d *publicExecutionDB) Exist(a common.Address) bool { d.account(a); return d.StateDB.Exist(a) }
func (d *publicExecutionDB) Empty(a common.Address) bool { d.account(a); return d.StateDB.Empty(a) }
func (d *publicExecutionDB) AddBalance(a common.Address, v *uint256.Int, why tracing.BalanceChangeReason) uint256.Int {
	d.account(a)
	return d.StateDB.AddBalance(a, v, why)
}
func (d *publicExecutionDB) SubBalance(a common.Address, v *uint256.Int, why tracing.BalanceChangeReason) uint256.Int {
	d.account(a)
	return d.StateDB.SubBalance(a, v, why)
}
func (d *publicExecutionDB) GetStorageRoot(a common.Address) common.Hash {
	d.account(a)
	return d.StateDB.GetStorageRoot(a)
}
func publicWord(n *big.Int) []byte { return common.LeftPadBytes(n.Bytes(), 32) }
func publicABI(signature string, words ...[]byte) []byte {
	out := append([]byte{}, crypto.Keccak256([]byte(signature))[:4]...)
	for _, word := range words {
		out = append(out, common.LeftPadBytes(word, 32)...)
	}
	return out
}
func loadPublicExecutionBank(t *testing.T) publicExecutionBank {
	t.Helper()
	m, rows := publicV2FixtureAt(t, publicExecutionRoot)
	bank := publicExecutionBank{Manifest: m, Accounts: map[common.Address]VerifiedAccountCommitment{}, Codes: map[common.Address][]byte{}, Slots: map[publicStorageKey]common.Hash{}}
	var header json.RawMessage
	proofs := []json.RawMessage{}
	for _, provider := range rows {
		for _, row := range provider {
			var response struct{ Result json.RawMessage }
			json.Unmarshal(row.Body, &response)
			switch row.Method {
			case "eth_getCode":
				var address, code string
				json.Unmarshal(row.Params[0], &address)
				json.Unmarshal(response.Result, &code)
				bank.Codes[common.HexToAddress(address)] = common.FromHex(code)
			case "eth_getProof":
				proofs = append(proofs, response.Result)
			case "eth_getBlockByNumber":
				var fields map[string]json.RawMessage
				json.Unmarshal(response.Result, &fields)
				var hash string
				json.Unmarshal(fields["hash"], &hash)
				if hash == m.Block.Hash {
					var txs []json.RawMessage
					json.Unmarshal(fields["transactions"], &txs)
					hashes := []string{}
					for _, tx := range txs {
						var h string
						if len(tx) > 0 && tx[0] == '{' {
							var v struct{ Hash string }
							json.Unmarshal(tx, &v)
							h = v.Hash
						} else {
							json.Unmarshal(tx, &h)
						}
						hashes = append(hashes, h)
					}
					fields["transactions"], _ = json.Marshal(hashes)
					header, _ = json.Marshal(fields)
				}
			}
		}
	}
	verified, err := VerifyChainHeaderJSON(context.Background(), "evm:1", header)
	if err != nil || verified.Block.Hash != m.Block.Hash {
		t.Fatal("public execution header", err)
	}
	raw, err := os.ReadFile(filepath.Join(publicExecutionRoot, "manifest.json"))
	if err != nil {
		t.Fatal(err)
	}
	var meta struct {
		Extras struct {
			File, SHA string
			Bytes     int
		} `json:"execution_extras"`
		Request struct {
			Sell string `json:"sellAmount"`
		} `json:"quote_request"`
		Response struct {
			Output string `json:"grossBuyAmount"`
		} `json:"quote_response"`
	}
	// The source metadata uses explicit sha256 rather than a generalized trust flag.
	var metadata map[string]json.RawMessage
	json.Unmarshal(raw, &metadata)
	if err = json.Unmarshal(raw, &meta); err != nil {
		t.Fatal(err)
	}
	var extras struct {
		File    string `json:"file"`
		SHA     string `json:"sha256"`
		Bytes   int    `json:"bytes"`
		Records int    `json:"records"`
	}
	json.Unmarshal(metadata["execution_extras"], &extras)
	if extras.File != "extra-proofs.json" || extras.Bytes <= 0 || extras.Bytes > 1<<20 || extras.Records != 2 {
		t.Fatal("execution extra proof inventory")
	}
	additional, err := os.ReadFile(filepath.Join(publicExecutionRoot, extras.File))
	if err != nil || len(additional) != extras.Bytes || publicV2Digest(additional) != extras.SHA {
		t.Fatal("extra proof bytes", err)
	}
	if err = json.Unmarshal(additional, &bank.ExtraRaw); err != nil || len(bank.ExtraRaw) != 2 {
		t.Fatal("extra proof results", err)
	}
	proofs = append(proofs, bank.ExtraRaw...)
	anchor := ExecutionStateAnchor{AccountEncodingID: "ethereum-account-rlp/v1", Height: m.Block.Number, BlockHash: m.Block.Hash, StateRoot: verified.StateRoot, SettlingHeight: m.Block.Number, SettlingHash: m.Block.Hash, Authority: "PINNED_PUBLIC_HEADER_NOT_INDEPENDENT_CONSENSUS"}
	if len(proofs) > 32 {
		t.Fatal("proof inventory resource cap")
	}
	for _, raw := range proofs {
		p, err := VerifyExecutionAccountProof(context.Background(), anchor, raw, 64<<20)
		if err != nil {
			t.Fatal("independent original execution proof", err)
		}
		a := common.HexToAddress(p.Address)
		if !p.Exists || len(bank.Codes[a]) == 0 || crypto.Keccak256Hash(bank.Codes[a]).Hex() != p.CodeHash {
			t.Fatal("unproved original code", a)
		}
		if old, ok := bank.Accounts[a]; ok && (old.CodeHash != p.CodeHash || old.StorageRoot != p.StorageRoot || old.Balance != p.Balance || old.Nonce != p.Nonce) {
			t.Fatal("inconsistent public account proof")
		}
		bank.Accounts[a] = p
		for key := range p.StorageValues {
			value, err := p.StorageValue(key)
			if err != nil {
				t.Fatal(err)
			}
			slot := publicStorageKey{a, common.HexToHash(key)}
			v := common.HexToHash(value)
			if old, ok := bank.Slots[slot]; ok && old != v {
				t.Fatal("contradictory proved storage")
			}
			bank.Slots[slot] = v
		}
	}
	if len(bank.Accounts) != 4 || len(bank.Slots) > 64 {
		t.Fatal("public execution account/slot resource cap")
	}
	var ok bool
	bank.Input, ok = new(big.Int).SetString(meta.Request.Sell, 10)
	if !ok || bank.Input.Sign() <= 0 {
		t.Fatal("quote input")
	}
	bank.Quoted, ok = new(big.Int).SetString(meta.Response.Output, 10)
	if !ok || bank.Quoted.Sign() <= 0 {
		t.Fatal("quote output")
	}
	return bank
}

type publicExecutionResult struct {
	Pool              domain.Pool `json:"pool"`
	Input             string      `json:"input"`
	Output            string      `json:"output"`
	Before            string      `json:"recipient_before"`
	After             string      `json:"recipient_after"`
	Next              domain.Pool `json:"next_pool"`
	Steps             int         `json:"instructions"`
	Calls             int         `json:"calls"`
	OutputAboveFailed bool        `json:"one_more_output_reverts_k"`
}

func executePublicSwap(ctx context.Context, bank publicExecutionBank, requested *big.Int, stepLimit int) (result publicExecutionResult, returned []byte, err error) {
	defer func() {
		if value := recover(); value != nil {
			stop, ok := value.(publicExecutionStop)
			if !ok {
				panic(value)
			}
			err = stop.err
		}
	}()
	if ctx.Err() != nil {
		return result, nil, ctx.Err()
	}
	if stepLimit <= 0 || stepLimit > 1000000 {
		return result, nil, errors.New("EXECUTION_INSTRUCTION_LIMIT")
	}
	db := state.NewDatabaseForTesting()
	defer db.TrieDB().Disk().Close()
	defer db.TrieDB().Close()
	base, e := state.New(types.EmptyRootHash, db)
	if e != nil {
		return result, nil, e
	}
	for a, p := range bank.Accounts {
		balance, ok := new(big.Int).SetString(p.Balance[2:], 16)
		if !ok || balance.BitLen() > 256 {
			return result, nil, errors.New("ACCOUNT_BALANCE_WIDTH")
		}
		base.CreateAccount(a)
		base.SetBalance(a, uint256.MustFromBig(balance), tracing.BalanceChangeUnspecified)
		base.SetNonce(a, p.Nonce, tracing.NonceChangeUnspecified)
		base.SetCode(a, bank.Codes[a], tracing.CodeChangeUnspecified)
	}
	for key, value := range bank.Slots {
		base.SetState(key.Address, key.Key, value)
	}
	base.CreateAccount(publicSender)
	base.SetBalance(publicSender, uint256.MustFromBig(new(big.Int).Exp(big.NewInt(10), big.NewInt(20), nil)), tracing.BalanceChangeUnspecified)
	base.Finalise(false)
	overlay := &publicExecutionDB{base, bank.Accounts, bank.Slots}
	var random common.Hash
	steps := 0
	pair := common.HexToAddress(bank.Manifest.Expected.PoolID)
	token0 := common.HexToAddress(bank.Manifest.Expected.Token0)
	token1 := common.HexToAddress(bank.Manifest.Expected.Token1)
	hooks := &tracing.Hooks{OnOpcode: func(pc uint64, op byte, gas, cost uint64, scope tracing.OpContext, data []byte, depth int, fault error) {
		steps++
		if ctx.Err() != nil {
			panic(publicExecutionStop{ctx.Err()})
		}
		if steps > stepLimit {
			panic(publicExecutionStop{errors.New("EXECUTION_INSTRUCTION_LIMIT")})
		}
		switch vm.OpCode(op) {
		case vm.BLOCKHASH, vm.CREATE, vm.CREATE2, vm.SELFDESTRUCT, vm.DELEGATECALL, vm.CALLCODE:
			panic(publicExecutionStop{errors.New("EXECUTION_UNSUPPORTED_STATE_DEPENDENCY")})
		}
	}}
	machine := vm.NewEVM(vm.BlockContext{CanTransfer: func(s vm.StateDB, a common.Address, v *uint256.Int) bool { return s.GetBalance(a).Cmp(v) >= 0 }, Transfer: func(s vm.StateDB, a, b common.Address, v *uint256.Int) {
		s.SubBalance(a, v, tracing.BalanceChangeTransfer)
		s.AddBalance(b, v, tracing.BalanceChangeTransfer)
	}, BlockNumber: new(big.Int).SetUint64(bank.Manifest.Block.Number), Time: bank.Manifest.Block.TimestampMS / 1000, GasLimit: 30000000, Difficulty: new(big.Int), BaseFee: big.NewInt(1), Random: &random}, overlay, params.MainnetChainConfig, vm.Config{Tracer: hooks})
	machine.SetTxContext(vm.TxContext{Origin: publicSender, GasPrice: big.NewInt(1)})
	balanceOf := func(token, who common.Address) (domain.Amount, error) {
		raw, _, err := machine.StaticCall(publicSender, token, publicABI("balanceOf(address)", who.Bytes()), 200000)
		if err != nil {
			return "", err
		}
		if len(raw) != 32 {
			return "", errors.New("BALANCE_RETURN_SIZE")
		}
		return domain.AmountFromBytes(raw)
	}
	before, err := balanceOf(token0, publicRecipient)
	if err != nil {
		return result, nil, err
	}
	call := func(to common.Address, input []byte, value *big.Int) ([]byte, error) {
		result.Calls++
		raw, _, err := machine.Call(publicSender, to, input, 2000000, uint256.MustFromBig(value))
		return raw, err
	}
	if returned, err = call(token1, publicABI("deposit()"), bank.Input); err != nil {
		return result, returned, err
	}
	if returned, err = call(token1, publicABI("transfer(address,uint256)", pair.Bytes(), publicWord(bank.Input)), new(big.Int)); err != nil {
		return result, returned, err
	}
	if len(returned) != 32 || new(big.Int).SetBytes(returned).Cmp(big.NewInt(1)) != 0 {
		return result, returned, errors.New("ORIGINAL_INPUT_TRANSFER_FAILED")
	}
	returned, err = call(pair, publicABI("swap(uint256,uint256,address,bytes)", publicWord(requested), publicWord(new(big.Int)), publicRecipient.Bytes(), publicWord(big.NewInt(128)), publicWord(new(big.Int))), new(big.Int))
	if err != nil {
		return result, returned, err
	}
	after, err := balanceOf(token0, publicRecipient)
	if err != nil {
		return result, nil, err
	}
	delta := new(big.Int).Sub(after.Big(), before.Big())
	if delta.Cmp(requested) != 0 {
		return result, nil, errors.New("ORIGINAL_RECIPIENT_DELTA_MISMATCH")
	}
	next := bank.Manifest.Expected
	swap := *next.V2.Swap
	identity := *next.V2
	next.V2 = &identity
	next.V2.Swap = &swap
	packed := overlay.GetState(pair, common.HexToHash("0x8")).Big()
	mask := new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), 112), big.NewInt(1))
	next.Reserve0 = domain.Amount(new(big.Int).And(new(big.Int).Set(packed), mask).String())
	next.Reserve1 = domain.Amount(new(big.Int).And(new(big.Int).Rsh(new(big.Int).Set(packed), 112), mask).String())
	swap.BlockTimestampLast = uint32(new(big.Int).Rsh(packed, 224).Uint64())
	swap.Price0CumulativeLast = domain.Amount(overlay.GetState(pair, common.HexToHash("0x9")).Big().String())
	swap.Price1CumulativeLast = domain.Amount(overlay.GetState(pair, common.HexToHash("0xa")).Big().String())
	swap.Unlocked = overlay.GetState(pair, common.HexToHash("0xc")).Big().Cmp(big.NewInt(1)) == 0
	swap.RawBalance0, err = balanceOf(token0, pair)
	if err != nil {
		return result, nil, err
	}
	swap.RawBalance1, err = balanceOf(token1, pair)
	if err != nil {
		return result, nil, err
	}
	if err = next.Validate(); err != nil {
		return result, nil, err
	}
	result.Pool = bank.Manifest.Expected
	result.Next = next
	result.Input = bank.Input.String()
	result.Output = delta.String()
	result.Before = before.String()
	result.After = after.String()
	result.Steps = steps
	return result, returned, nil
}
func TestV2RealPublicOriginalExecutionMatchesQuoteAndMaximalIntegerOutput(t *testing.T) {
	bank := loadPublicExecutionBank(t)
	result, _, err := executePublicSwap(context.Background(), bank, bank.Quoted, 1000000)
	if err != nil {
		t.Fatal(err)
	}
	_, returned, err := executePublicSwap(context.Background(), bank, new(big.Int).Add(bank.Quoted, big.NewInt(1)), 1000000)
	if err == nil || !bytes.Contains(returned, []byte("UniswapV2: K")) {
		t.Fatal("quoted amount not maximal under original K", err, fmt.Sprintf("%x", returned))
	}
	result.OutputAboveFailed = true
	if out := os.Getenv("XONE_PUBLIC_V2_EXECUTION_OUTPUT"); out != "" {
		if err = os.MkdirAll(out, 0700); err != nil {
			t.Fatal(err)
		}
		raw, _ := json.MarshalIndent(map[string]any{"schema_version": 1, "authority": "PROVED_PUBLIC_BANK_ORIGINAL_CODE_LOCAL_CALL_SEQUENCE_COUNTERFACTUAL_NATIVE_FUNDING", "result": result, "consensus_verified": false, "production_ready": false, "chain_fees_complete": false, "public_transactions_submitted": false, "go_ethereum_version": "v1.16.7", "source_manifest_sha256": func() string {
			raw, _ := os.ReadFile(filepath.Join(publicExecutionRoot, "manifest.json"))
			return publicV2Digest(raw)
		}()}, "", "  ")
		if err = os.WriteFile(filepath.Join(out, "original-public-v2-execution.json"), raw, 0600); err != nil {
			t.Fatal(err)
		}
	}
}
func TestV2RealPublicOriginalExecutionRejectsMissingProofAndBoundsWork(t *testing.T) {
	for _, fault := range []string{"missing-slot", "unproved-account", "instructions", "canceled"} {
		t.Run(fault, func(t *testing.T) {
			bank := loadPublicExecutionBank(t)
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			limit := 1000000
			switch fault {
			case "missing-slot":
				for key := range bank.Slots {
					if key.Address == common.HexToAddress(bank.Manifest.Expected.Token0) {
						delete(bank.Slots, key)
					}
				}
			case "unproved-account":
				delete(bank.Accounts, common.HexToAddress(bank.Manifest.Expected.Token0))
			case "instructions":
				limit = 10
			case "canceled":
				cancel()
			}
			_, _, err := executePublicSwap(ctx, bank, bank.Quoted, limit)
			if err == nil {
				t.Fatal("unsafe execution passed")
			}
			if fault == "missing-slot" && !strings.Contains(err.Error(), "UNPROVEN_EXECUTION_STORAGE") {
				t.Fatal(err)
			}
			if fault == "unproved-account" && !strings.Contains(err.Error(), "UNPROVEN_EXECUTION_ACCOUNT") {
				t.Fatal(err)
			}
			if fault == "canceled" && !errors.Is(err, context.Canceled) {
				t.Fatal(err)
			}
		})
	}
}
