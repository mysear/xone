package evm

import (
	"crypto/sha256"
	"encoding/hex"
	"errors"

	"github.com/ethereum/go-ethereum/common"
)

const opUSDCAddress = "0x0b2c639c533813f4aa9d7837caf62653d097ff85"

// The proxy and implementation pins describe the deployment read at block
// 159770518. Prove the implementation pointer again at every selected bank.
func (batch *accountProofBatch) opUSDCDelegate(reader *Adapter, code []byte, slots map[common.Hash]common.Hash) (*committedViewDelegate, error) {
	const implementationSlot = "0x7050c9e0f4ca769c69bd3a8ef740bc37934f8e2c036e5a723fd8ee048ed3f8c3"
	const implementationAddress = "0xded3b9a8dbedc2f9cb725b55d0e686a81e6d06dc"
	digest := sha256.Sum256(code)
	if hex.EncodeToString(digest[:]) != "82b9f5bf79f2088c128f6881cad81e6d0996ddf79289b2bde7186fe2e55113aa" {
		return nil, errors.New("OP_USDC_PROXY_CODE")
	}
	proxy, err := batch.read(opUSDCAddress, []string{implementationSlot})
	if err != nil {
		return nil, err
	}
	value, err := proxy.StorageValue(implementationSlot)
	if err != nil {
		return nil, err
	}
	target := common.HexToAddress(implementationAddress)
	word := common.HexToHash(value)
	if word != common.BytesToHash(target.Bytes()) {
		return nil, errors.New("OP_USDC_IMPLEMENTATION_SLOT")
	}
	implementation, err := batch.read(implementationAddress, nil)
	if err != nil {
		return nil, err
	}
	if !implementation.Account.Exists || implementation.Account.CodeHash != "0x24c6e8b8e0aaea31861e014c15e9da1564402d3512eb474d46def8e8bb2b2259" {
		return nil, errors.New("OP_USDC_IMPLEMENTATION_CODE")
	}
	runtime, err := batch.readProvenViewCode(reader, implementationAddress, implementation.Account.CodeHash)
	if err != nil {
		return nil, err
	}
	slots[common.HexToHash(implementationSlot)] = word
	return &committedViewDelegate{address: target, code: runtime}, nil
}
