// Local fixture wallet only. Signs the normal HTTP build response with fixed
// public test key 1 through the pinned original client. Never broadcasts.
package main

import (
	"encoding/hex"
	"encoding/json"
	"math/big"
	"os"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
)

func must(err error) {
	if err != nil {
		panic(err)
	}
}
func main() {
	var rpc map[string]json.RawMessage
	must(json.NewDecoder(os.Stdin).Decode(&rpc))
	text := func(key string) string { var s string; must(json.Unmarshal(rpc[key], &s)); return s }
	integer := func(key string) *big.Int {
		s := text(key)
		if len(s) < 3 || s[:2] != "0x" {
			panic("hex quantity required")
		}
		n, ok := new(big.Int).SetString(s[2:], 16)
		if !ok || n.Sign() < 0 || n.BitLen() > 256 {
			panic("invalid quantity")
		}
		return n
	}
	word := func(key string) uint64 {
		n := integer(key)
		if !n.IsUint64() {
			panic("uint64 overflow")
		}
		return n.Uint64()
	}
	to := common.HexToAddress(text("to"))
	data, err := hex.DecodeString(text("data")[2:])
	must(err)
	chain := integer("chainId")
	var tx *types.Transaction
	switch text("type") {
	case "0x0":
		tx = types.NewTx(&types.LegacyTx{Nonce: word("nonce"), GasPrice: integer("gasPrice"), Gas: word("gas"), To: &to, Value: integer("value"), Data: data})
	case "0x2":
		tx = types.NewTx(&types.DynamicFeeTx{ChainID: chain, Nonce: word("nonce"), GasTipCap: integer("maxPriorityFeePerGas"), GasFeeCap: integer("maxFeePerGas"), Gas: word("gas"), To: &to, Value: integer("value"), Data: data})
	default:
		panic("unsupported test wallet transaction")
	}
	key, err := crypto.HexToECDSA("0000000000000000000000000000000000000000000000000000000000000001")
	must(err)
	if crypto.PubkeyToAddress(key.PublicKey) != common.HexToAddress(text("from")) {
		panic("public fixture caller differs")
	}
	signed, err := types.SignTx(tx, types.LatestSignerForChainID(chain), key)
	must(err)
	wire, err := signed.MarshalBinary()
	must(err)
	must(json.NewEncoder(os.Stdout).Encode(map[string]any{"transactionHash": signed.Hash().Hex(), "signedWire": "0x" + hex.EncodeToString(wire)}))
}
