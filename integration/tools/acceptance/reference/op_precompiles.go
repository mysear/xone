// Input/output wrapper only. Both precompiles and signed state transitions use
// the unmodified pinned original op-geth. Helpers come from op_execution.go.
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
	"github.com/ethereum/go-ethereum/core/tracing"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/core/vm"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/params"
	"github.com/holiman/uint256"
)

type precompileInput struct {
	ID         string `json:"id"`
	Fork       string `json:"fork"`
	Address    string `json:"address"`
	Data       string `json:"data"`
	Gas        uint64 `json:"gas"`
	Transition bool   `json:"transition"`
	Attributes string `json:"attributes,omitempty"`
	Timestamp  uint64 `json:"timestamp,omitempty"`
}

func registry(fork string) vm.PrecompiledContracts {
	switch fork {
	case "regolith", "canyon":
		return vm.PrecompiledContractsBerlin
	case "ecotone":
		return vm.PrecompiledContractsCancun
	case "fjord":
		return vm.PrecompiledContractsFjord
	case "granite", "holocene":
		return vm.PrecompiledContractsGranite
	case "isthmus":
		return vm.PrecompiledContractsIsthmus
	default:
		panic("unsupported historical fork")
	}
}
func signedPrecompile(p precompileInput) map[string]any {
	s := scenario{Chain: 10, Rule: p.Fork, Attributes: p.Attributes, Timestamp: p.Timestamp, BaseFee: "7"}
	cfg := config(s)
	key, err := crypto.HexToECDSA("0000000000000000000000000000000000000000000000000000000000000001")
	must(err)
	caller := crypto.PubkeyToAddress(key.PublicKey)
	to := common.HexToAddress(p.Address)
	data, err := hex.DecodeString(p.Data)
	must(err)
	gas := p.Gas + 21000
	for _, b := range data {
		if b == 0 {
			gas += 4
		} else {
			gas += 16
		}
	}
	if cfg.IsPrague(big.NewInt(100), s.Timestamp) {
		floor, err := core.FloorDataGas(data)
		must(err)
		if gas < floor {
			gas = floor
		}
	}
	tx, err := types.SignTx(types.NewTx(&types.DynamicFeeTx{ChainID: big.NewInt(10), To: &to, Gas: gas, GasFeeCap: big.NewInt(100), GasTipCap: big.NewInt(2), Data: data}), types.MakeSigner(cfg, big.NewInt(100), p.Timestamp), key)
	must(err)
	wire, err := tx.MarshalBinary()
	must(err)
	msg, err := core.TransactionToMessage(tx, types.MakeSigner(cfg, big.NewInt(100), p.Timestamp), big.NewInt(7))
	must(err)
	if msg.From != caller || msg.SkipNonceChecks || msg.SkipTransactionChecks {
		panic("signature bypass")
	}
	db := initial(s)
	beneficiary := common.HexToAddress("0x2000000000000000000000000000000000000002")
	owners := map[common.Address]bool{caller: true, to: true, beneficiary: true, params.OptimismBaseFeeRecipient: true, params.OptimismL1FeeRecipient: true, params.OptimismOperatorFeeRecipient: true}
	for a := range owners {
		db.SetBalance(a, uint256.NewInt(111), tracing.BalanceChangeUnspecified)
	}
	db.SetBalance(caller, uint256.NewInt(1_000_000_000_000_000_000), tracing.BalanceChangeUnspecified)
	db.Finalise(false)
	if db.GetRefund() != 0 {
		panic("dirty initial state")
	}
	before := map[string]string{}
	for a := range owners {
		before[a.Hex()] = db.GetBalance(a).String()
	}
	random := common.Hash{}
	ctx := vm.BlockContext{CanTransfer: core.CanTransfer, Transfer: core.Transfer, GetHash: func(uint64) common.Hash { return common.Hash{} }, L1CostFunc: types.NewL1CostFunc(cfg, db), OperatorCostFunc: types.NewOperatorCostFunc(cfg, db), Coinbase: beneficiary, GasLimit: 60_000_000, BlockNumber: big.NewInt(100), Time: p.Timestamp, Difficulty: new(big.Int), BaseFee: big.NewInt(7), BlobBaseFee: big.NewInt(1), Random: &random}
	result, err := core.ApplyMessage(vm.NewEVM(ctx, db, cfg, vm.Config{}), msg, core.NewGasPool(60_000_000))
	if err != nil {
		panic(fmt.Sprint(p.ID, err))
	}
	after := map[string]string{}
	for a := range owners {
		after[a.Hex()] = db.GetBalance(a).String()
	}
	return map[string]any{"signed_wire": hex.EncodeToString(wire), "transaction_hash": tx.Hash().Hex(), "caller": caller.Hex(), "beneficiary": beneficiary.Hex(), "before": before, "after": after, "gas_limit": gas, "gas_used": result.UsedGas, "success": !result.Failed(), "output": hex.EncodeToString(result.ReturnData), "nonce_after": db.GetNonce(caller)}
}
func main() {
	var input []precompileInput
	must(json.NewDecoder(os.Stdin).Decode(&input))
	if len(input) == 0 || len(input) > 10000 {
		panic("precompile input budget")
	}
	rows := []map[string]any{}
	for _, p := range input {
		raw, err := hex.DecodeString(p.Data)
		must(err)
		if len(raw) > 1_000_000 {
			panic("precompile byte budget")
		}
		contract, ok := registry(p.Fork)[common.HexToAddress(p.Address)]
		row := map[string]any{"input": p, "present": ok, "success": false, "output": "", "out_of_gas": false}
		if ok {
			row["required_gas"] = contract.RequiredGas(raw)
			output, remaining, err := vm.RunPrecompiledContract(nil, contract, common.HexToAddress(p.Address), raw, p.Gas, nil)
			row["success"] = err == nil
			row["output"] = hex.EncodeToString(output)
			row["remaining_gas"] = remaining
			row["out_of_gas"] = errors.Is(err, vm.ErrOutOfGas)
			if err != nil {
				row["error"] = err.Error()
			}
		}
		if p.Transition {
			row["transition"] = signedPrecompile(p)
		}
		rows = append(rows, row)
	}
	must(json.NewEncoder(os.Stdout).Encode(rows))
}
