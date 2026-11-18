package evm

import (
	"context"
	"errors"
	"math/big"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/state"
	"github.com/ethereum/go-ethereum/core/tracing"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/core/vm"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/params"
	"github.com/holiman/uint256"
)

const committedViewGas = uint64(100000)
const committedViewSlots = 8

type committedViewStop struct{ err error }

// Only stable, single-account computation is admitted. External calls, account
// inspection, block/fee input, GAS and all writes are rejected. Caller/origin
// and value are fixed at zero, matching the parser's declared eth_call shape.
// The fixed gas allowance is a resource cap, never an on-chain fee estimate.
func committedViewOpcode(op byte) bool {
	return op <= 0x0b || op >= 0x10 && op <= 0x1d || op == 0x20 ||
		op >= 0x30 && op <= 0x39 && op != 0x31 || op >= 0x50 && op <= 0x59 && op != 0x55 ||
		op == 0x5b || op >= 0x60 && op <= 0x9f || op == 0xf3 || op == 0xfd
}

// Missing SLOAD is stopped before it executes; its default zero value can never
// affect branching or be accepted. The caller proves the requested slot and
// restarts with a fresh overlay. Only our private sentinel is recovered.
func executeCommittedView(ctx context.Context, code []byte, address common.Address, input []byte, slots map[common.Hash]common.Hash) (output []byte, missing *common.Hash, err error) {
	if e := ctx.Err(); e != nil {
		return nil, nil, e
	}
	if len(code) == 0 || len(code) > 24576 || len(input) != 36 || len(slots) > committedViewSlots {
		return nil, nil, errors.New("STATE_VIEW_LIMIT")
	}
	defer func() {
		if value := recover(); value != nil {
			stop, ok := value.(committedViewStop)
			if !ok {
				panic(value)
			}
			err = stop.err
			output = nil
		}
	}()
	db := state.NewDatabaseForTesting()
	defer db.TrieDB().Disk().Close()
	defer db.TrieDB().Close()
	overlay, e := state.New(types.EmptyRootHash, db)
	if e != nil {
		return nil, nil, errors.New("STATE_VIEW_OVERLAY")
	}
	for key, value := range slots {
		overlay.SetState(address, key, value)
	}
	hooks := &tracing.Hooks{OnOpcode: func(pc uint64, op byte, gas, cost uint64, scope tracing.OpContext, data []byte, depth int, fault error) {
		if e := ctx.Err(); e != nil {
			panic(committedViewStop{e})
		}
		if !committedViewOpcode(op) {
			panic(committedViewStop{errors.New("STATE_VIEW_OPCODE_UNSUPPORTED")})
		}
		if op == byte(vm.SLOAD) {
			stack := scope.StackData()
			key := common.Hash(stack[len(stack)-1].Bytes32())
			if _, ok := slots[key]; !ok {
				missing = &key
				panic(committedViewStop{errors.New("STATE_VIEW_STORAGE_REQUIRED")})
			}
		}
	}}
	// Berlin's stable arithmetic/memory/SLOAD semantics are sufficient for this
	// subset. PUSH0 and all environment-dependent instructions are excluded.
	evm := vm.NewEVM(vm.BlockContext{BlockNumber: big.NewInt(12244000), Time: 1618481223, Difficulty: new(big.Int)}, overlay, params.MainnetChainConfig, vm.Config{Tracer: hooks})
	evm.SetTxContext(vm.TxContext{GasPrice: new(big.Int)})
	contract := vm.NewContract(common.Address{}, address, new(uint256.Int), committedViewGas, nil)
	contract.SetCallCode(crypto.Keccak256Hash(code), code)
	output, err = evm.Run(contract, input, true)
	if e := ctx.Err(); e != nil {
		return nil, nil, e
	}
	if err != nil {
		return nil, nil, errors.New("STATE_VIEW_EXECUTION_FAILED")
	}
	if len(output) != 32 {
		return nil, nil, errors.New("STATE_VIEW_RETURN_SIZE")
	}
	return output, nil, nil
}
