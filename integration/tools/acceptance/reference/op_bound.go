// Local BOUND input decoding and observations only. All EVM transitions execute
// the complete unchanged pinned op-geth runtime, not a reconstructed Router.
package main

import (
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math/big"
	"os"
	"strings"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core"
	"github.com/ethereum/go-ethereum/core/state"
	"github.com/ethereum/go-ethereum/core/tracing"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/core/vm"
	"github.com/ethereum/go-ethereum/params"
	"github.com/holiman/uint256"
)

type slot struct {
	Slot  string `json:"slot"`
	Value string `json:"value"`
}
type account struct {
	Address string `json:"address"`
	Exists  bool   `json:"exists"`
	Balance string `json:"balance"`
	Nonce   uint64 `json:"nonce"`
	Code    string `json:"code"`
	Storage []slot `json:"storage"`
}
type environment struct {
	ChainID  uint64 `json:"chain_id"`
	Number   uint64 `json:"block_number"`
	Time     uint64 `json:"timestamp"`
	Coinbase string `json:"coinbase"`
	GasLimit uint64 `json:"gas_limit"`
	BaseFee  string `json:"base_fee"`
	Random   string `json:"prevrandao"`
}
type fixture struct {
	Accounts    []account   `json:"accounts"`
	Environment environment `json:"execution_environment"`
}
type inputCase struct {
	ID        string  `json:"id"`
	Rule      string  `json:"rule"`
	Fixture   fixture `json:"fixture"`
	Wire      string  `json:"signed_wire"`
	Hash      string  `json:"transaction_hash"`
	Caller    string  `json:"caller"`
	TokenIn   string  `json:"token_in"`
	TokenOut  string  `json:"token_out"`
	Recipient string  `json:"recipient"`
}

func must(err error) {
	if err != nil {
		panic(err)
	}
}
func integer(s string) *big.Int {
	n, ok := new(big.Int).SetString(s, 10)
	if !ok || n.Sign() < 0 || n.BitLen() > 256 {
		panic("bad integer")
	}
	return n
}
func config(s inputCase) *params.ChainConfig {
	c := *params.OptimismTestConfig
	c.ChainID = new(big.Int).SetUint64(s.Fixture.Environment.ChainID)
	c.PragueTime = nil
	c.IsthmusTime = nil
	c.JovianTime = nil
	c.FjordTime = nil
	c.GraniteTime = nil
	c.HoloceneTime = nil
	zero := uint64(0)
	switch s.Rule {
	case "regolith":
		c.CanyonTime = nil
		c.EcotoneTime = nil
		c.ShanghaiTime = nil
		c.CancunTime = nil
	case "canyon":
		c.EcotoneTime = nil
		c.CancunTime = nil
	case "ecotone":
	case "fjord":
		c.FjordTime = &zero
	case "granite":
		c.FjordTime = &zero
		c.GraniteTime = &zero
	case "holocene":
		c.FjordTime = &zero
		c.GraniteTime = &zero
		c.HoloceneTime = &zero
	case "isthmus":
		c.GraniteTime = &zero
		c.HoloceneTime = &zero
		c.FjordTime = &zero
		c.IsthmusTime = &zero
		c.PragueTime = &zero
	case "jovian":
		c.FjordTime = &zero
		c.IsthmusTime = &zero
		c.PragueTime = &zero
		c.JovianTime = &zero
	default:
		panic("unknown rule")
	}
	return &c
}

func main() {
	var inputs []inputCase
	must(json.NewDecoder(os.Stdin).Decode(&inputs))
	if len(inputs) != 448 {
		panic("BOUND scenario coverage changed")
	}
	rows := []map[string]any{}
	for _, s := range inputs {
		e := s.Fixture.Environment
		cfg := config(s)
		db, err := state.New(types.EmptyRootHash, state.NewDatabaseForTesting())
		must(err)
		for _, a := range s.Fixture.Accounts {
			address := common.HexToAddress(a.Address)
			if !a.Exists {
				continue
			}
			db.CreateAccount(address)
			db.SetBalance(address, uint256.MustFromBig(integer(a.Balance)), tracing.BalanceChangeUnspecified)
			db.SetNonce(address, a.Nonce, tracing.NonceChangeUnspecified)
			db.SetCode(address, common.FromHex(a.Code), tracing.CodeChangeUnspecified)
			for _, v := range a.Storage {
				db.SetState(address, common.HexToHash(v.Slot), common.HexToHash(v.Value))
			}
		}
		// Finish installation before the signed transaction. SSTORE original and
		// current storage must agree; dirty writes here would corrupt refund gas.
		db.Finalise(false)
		for _, a := range s.Fixture.Accounts {
			for _, v := range a.Storage {
				address, key := common.HexToAddress(a.Address), common.HexToHash(v.Slot)
				if db.GetCommittedState(address, key) != db.GetState(address, key) {
					panic("initial committed storage differs")
				}
			}
		}
		if db.GetRefund() != 0 {
			panic("initial refund nonzero")
		}
		raw, err := hex.DecodeString(s.Wire)
		must(err)
		var tx types.Transaction
		must(tx.UnmarshalBinary(raw))
		if tx.Hash().Hex() != s.Hash {
			panic("transaction hash differs")
		}
		msg, err := core.TransactionToMessage(&tx, types.MakeSigner(cfg, new(big.Int).SetUint64(e.Number), e.Time), integer(e.BaseFee))
		must(err)
		if msg.From != common.HexToAddress(s.Caller) || msg.SkipNonceChecks || msg.SkipTransactionChecks {
			panic("signature bypass")
		}
		random := common.HexToHash(e.Random)
		ctx := vm.BlockContext{CanTransfer: core.CanTransfer, Transfer: core.Transfer, GetHash: func(uint64) common.Hash { panic("unprovided block hash") },
			L1CostFunc: types.NewL1CostFunc(cfg, db), OperatorCostFunc: types.NewOperatorCostFunc(cfg, db), Coinbase: common.HexToAddress(e.Coinbase),
			GasLimit: e.GasLimit, BlockNumber: new(big.Int).SetUint64(e.Number), Time: e.Time, Difficulty: new(big.Int), BaseFee: integer(e.BaseFee), BlobBaseFee: big.NewInt(1), Random: &random}
		// Each balance observation uses a separate bank copy. No probe can change
		// either the transaction prestate or the poststate measured below.
		balance := func(bank *state.StateDB, token, owner string) *big.Int {
			data := append(common.FromHex("70a08231"), common.LeftPadBytes(common.HexToAddress(owner).Bytes(), 32)...)
			out, _, err := vm.NewEVM(ctx, bank.Copy(), cfg, vm.Config{}).StaticCall(msg.From, common.HexToAddress(token), data, 200000)
			must(err)
			if len(out) != 32 {
				panic("balanceOf output length")
			}
			return new(big.Int).SetBytes(out)
		}
		sellBefore, buyBefore := balance(db, s.TokenIn, s.Caller), balance(db, s.TokenOut, s.Recipient)
		l1 := types.NewL1CostFunc(cfg, db)(tx.RollupCostData(), e.Time)
		if l1 == nil {
			panic("missing L1 fee")
		}
		op := types.NewOperatorCostFunc(cfg, db)
		result, err := core.ApplyMessage(vm.NewEVM(ctx, db, cfg, vm.Config{}), msg, core.NewGasPool(e.GasLimit))
		must(err)
		if result.Failed() {
			panic(fmt.Sprintf("%s original swap failed: %v", s.ID, result.Err))
		}
		spent := new(big.Int).Sub(sellBefore, balance(db, s.TokenIn, s.Caller))
		bought := new(big.Int).Sub(balance(db, s.TokenOut, s.Recipient), buyBefore)
		if spent.Sign() <= 0 || bought.Sign() <= 0 {
			panic("missing real token transfer")
		}
		accounts := map[string]any{}
		for _, a := range s.Fixture.Accounts {
			address := common.HexToAddress(a.Address)
			storage := map[string]string{}
			for _, v := range a.Storage {
				storage[v.Slot] = db.GetState(address, common.HexToHash(v.Slot)).Hex()
			}
			accounts[strings.ToLower(a.Address)] = map[string]any{"balance": db.GetBalance(address).String(), "nonce": db.GetNonce(address), "code": "0x" + hex.EncodeToString(db.GetCode(address)), "storage": storage}
		}
		rows = append(rows, map[string]any{"id": s.ID, "transaction_hash": s.Hash, "gas_used": result.UsedGas, "nonce_after": db.GetNonce(msg.From),
			"spent": spent.String(), "bought": bought.String(), "l1_fee": l1.String(), "operator_fee": op(result.UsedGas, e.Time).String(), "accounts": accounts})
	}
	must(json.NewEncoder(os.Stdout).Encode(rows))
}
