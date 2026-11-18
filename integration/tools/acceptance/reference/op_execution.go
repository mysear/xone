// Local scenario construction and I/O only. Fee expectations and balance changes
// are produced by the unchanged, pinned op-geth core.ApplyMessage and StateDB.
package main

import (
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"math/big"
	"os"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core"
	"github.com/ethereum/go-ethereum/core/state"
	"github.com/ethereum/go-ethereum/core/tracing"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/core/vm"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/params"
	"github.com/holiman/uint256"
)

type scenario struct {
	Chain       uint64 `json:"chain"`
	Access      bool   `json:"access"`
	ID          string `json:"id"`
	Rule        string `json:"rule"`
	Attributes  string `json:"attributes"`
	Kind        string `json:"kind"`
	Outcome     string `json:"outcome"`
	Beneficiary string `json:"beneficiary"`
	BaseFee     string `json:"base_fee"`
	Maximum     string `json:"maximum"`
	Priority    string `json:"priority"`
	GasLimit    uint64 `json:"gas_limit"`
	Timestamp   uint64 `json:"timestamp"`
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
func config(s scenario) *params.ChainConfig {
	c := *params.OptimismTestConfig
	c.ChainID = new(big.Int).SetUint64(s.Chain)
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
func initial(s scenario) *state.StateDB {
	db, err := state.New(types.EmptyRootHash, state.NewDatabaseForTesting())
	must(err)
	a, err := hex.DecodeString(s.Attributes)
	must(err)
	put := func(slot common.Hash, data []byte) { db.SetState(types.L1BlockAddr, slot, common.BytesToHash(data)) }
	if s.Rule == "regolith" || s.Rule == "canyon" {
		if len(a) != 260 {
			panic("bad Bedrock attributes")
		}
		put(types.L1BaseFeeSlot, a[68:100])
		put(types.OverheadSlot, a[196:228])
		put(types.ScalarSlot, a[228:260])
	} else {
		if len(a) != 164 && len(a) != 176 && len(a) != 178 {
			panic("bad packed attributes")
		}
		put(types.L1BaseFeeSlot, a[36:68])
		put(types.L1BlobBaseFeeSlot, a[68:100])
		var scalars common.Hash
		copy(scalars[16:24], a[4:12])
		db.SetState(types.L1BlockAddr, types.L1FeeScalarsSlot, scalars)
		if len(a) >= 176 {
			var op common.Hash
			copy(op[20:32], a[164:176])
			db.SetState(types.L1BlockAddr, types.OperatorFeeParamsSlot, op)
		}
	}
	return db
}
func main() {
	var input []scenario
	must(json.NewDecoder(os.Stdin).Decode(&input))
	if len(input) == 0 || len(input) > 1024 {
		panic("scenario budget")
	}
	key, err := crypto.HexToECDSA("0000000000000000000000000000000000000000000000000000000000000001")
	must(err)
	payer := crypto.PubkeyToAddress(key.PublicKey)
	target := common.HexToAddress("0x1000000000000000000000000000000000000001")
	output := []map[string]any{}
	for _, s := range input {
		c := config(s)
		number := big.NewInt(100)
		signer := types.MakeSigner(c, number, s.Timestamp)
		data := []byte{0, 1, 2, 0, 3}
		var tx *types.Transaction
		var access types.AccessList
		if s.Access {
			access = types.AccessList{{Address: target, StorageKeys: []common.Hash{{}}}}
		}
		switch s.Kind {
		case "legacy":
			tx = types.NewTx(&types.LegacyTx{Nonce: 0, To: &target, Gas: s.GasLimit, GasPrice: integer(s.Maximum), Value: new(big.Int), Data: data})
		case "access":
			tx = types.NewTx(&types.AccessListTx{AccessList: access, ChainID: c.ChainID, Nonce: 0, To: &target, Gas: s.GasLimit, GasPrice: integer(s.Maximum), Value: new(big.Int), Data: data})
		case "dynamic":
			tx = types.NewTx(&types.DynamicFeeTx{AccessList: access, ChainID: c.ChainID, Nonce: 0, To: &target, Gas: s.GasLimit, GasFeeCap: integer(s.Maximum), GasTipCap: integer(s.Priority), Value: new(big.Int), Data: data})
		default:
			panic("unknown transaction kind")
		}
		tx, err = types.SignTx(tx, signer, key)
		must(err)
		raw, err := tx.MarshalBinary()
		must(err)
		msg, err := core.TransactionToMessage(tx, signer, integer(s.BaseFee))
		must(err)
		if msg.From != payer || msg.SkipNonceChecks || msg.SkipTransactionChecks {
			panic("signature bypass")
		}
		beneficiary := common.HexToAddress("0x2000000000000000000000000000000000000002")
		switch s.Beneficiary {
		case "payer":
			beneficiary = payer
		case "base":
			beneficiary = params.OptimismBaseFeeRecipient
		case "l1":
			beneficiary = params.OptimismL1FeeRecipient
		case "operator":
			beneficiary = params.OptimismOperatorFeeRecipient
		case "separate":
		default:
			panic("unknown beneficiary")
		}
		db := initial(s)
		l1 := types.NewL1CostFunc(c, db)(tx.RollupCostData(), s.Timestamp)
		if l1 == nil {
			panic("missing L1 fee")
		}
		opFn := types.NewOperatorCostFunc(c, db)
		opMax := opFn(s.GasLimit, s.Timestamp).ToBig()
		maxFunding := new(big.Int).Mul(new(big.Int).SetUint64(s.GasLimit), tx.GasFeeCap())
		maxFunding.Add(maxFunding, l1)
		maxFunding.Add(maxFunding, opMax)
		run := func(balance *big.Int) map[string]any {
			db := initial(s)
			accounts := map[common.Address]bool{payer: true, beneficiary: true, params.OptimismBaseFeeRecipient: true, params.OptimismL1FeeRecipient: true, params.OptimismOperatorFeeRecipient: true}
			for a := range accounts {
				db.SetBalance(a, uint256.NewInt(111), tracing.BalanceChangeUnspecified)
			}
			db.SetBalance(payer, uint256.MustFromBig(balance), tracing.BalanceChangeUnspecified)
			code := "00"
			switch s.Outcome {
			case "success":
			case "refund":
				code = "600060005500"
				db.SetState(target, common.Hash{}, common.BigToHash(big.NewInt(1)))
			case "revert":
				code = "600160005560006000fd"
			case "halt":
				code = "6001600055fe"
			default:
				panic("unknown outcome")
			}
			db.SetCode(target, common.FromHex(code), tracing.CodeChangeUnspecified)
			// State installation is a completed prior phase, not a dirty write
			// made inside the transaction being measured. EIP-2200 must see
			// the same original/current slot that the independent VM receives.
			db.Finalise(false)
			if db.GetCommittedState(target, common.Hash{}) != db.GetState(target, common.Hash{}) || db.GetRefund() != 0 {
				panic("uncommitted initial storage")
			}
			before := map[string]string{}
			for a := range accounts {
				before[a.Hex()] = db.GetBalance(a).String()
			}
			random := common.Hash{}
			ctx := vm.BlockContext{CanTransfer: core.CanTransfer, Transfer: core.Transfer, GetHash: func(uint64) common.Hash { return common.Hash{} }, L1CostFunc: types.NewL1CostFunc(c, db), OperatorCostFunc: types.NewOperatorCostFunc(c, db), Coinbase: beneficiary, GasLimit: 30000000, BlockNumber: number, Time: s.Timestamp, Difficulty: new(big.Int), BaseFee: integer(s.BaseFee), BlobBaseFee: big.NewInt(1), Random: &random}
			result, coreErr := core.ApplyMessage(vm.NewEVM(ctx, db, c, vm.Config{}), msg, core.NewGasPool(30000000))
			after := map[string]string{}
			for a := range accounts {
				after[a.Hex()] = db.GetBalance(a).String()
			}
			row := map[string]any{"before": before, "after": after, "nonce_after": db.GetNonce(payer), "storage_after": db.GetState(target, common.Hash{}).Hex(), "accepted": coreErr == nil}
			if coreErr != nil {
				row["insufficient_funds"] = errors.Is(coreErr, core.ErrInsufficientFunds)
				row["error"] = coreErr.Error()
			} else {
				row["gas_used"] = result.UsedGas
				row["failed"] = result.Failed()
				row["operator_fee"] = opFn(result.UsedGas, s.Timestamp).String()
				if result.Err != nil {
					row["execution_error"] = result.Err.Error()
				}
			}
			return row
		}
		rich := new(big.Int).Add(maxFunding, big.NewInt(1000000000))
		actual := run(rich)
		exact := run(maxFunding)
		if actual["accepted"] != true || exact["accepted"] != true {
			panic(fmt.Sprint("reference execution rejected ", s.ID, actual, exact))
		}
		if actual["failed"] != (s.Outcome == "revert" || s.Outcome == "halt") {
			panic("wrong VM outcome")
		}
		var below map[string]any
		if maxFunding.Sign() > 0 {
			below = run(new(big.Int).Sub(maxFunding, big.NewInt(1)))
			if below["accepted"] != false || below["insufficient_funds"] != true || below["nonce_after"] != uint64(0) {
				panic("funding boundary not enforced")
			}
		}
		output = append(output, map[string]any{"input": s, "signed_wire": hex.EncodeToString(raw), "transaction_hash": tx.Hash().Hex(), "payer": payer.Hex(), "beneficiary": beneficiary.Hex(), "l1_fee": l1.String(), "operator_maximum": opMax.String(), "maximum_funding": maxFunding.String(), "effective_gas_price": msg.GasPrice.String(), "actual": actual, "exact_funding": exact, "below_funding": below})
	}
	must(json.NewEncoder(os.Stdout).Encode(output))
}
