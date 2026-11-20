package evm

import (
	"errors"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/crypto"
	"xone/chain-parser-go/internal/domain"
)

// Authenticate the actual code and each reached SLOAD instead of assuming a
// universal ERC20 storage layout. This only proves this zero-caller view; token
// transfer behavior, consensus authority and production execution remain separate.
func (batch *accountProofBatch) verifyNativeV2Balance(reader *Adapter, token, owner string, expected domain.Amount) error {
	ctx := batch.ctx
	account, err := batch.read(token, nil)
	if err != nil {
		return err
	}
	if !account.Account.Exists {
		return errors.New("V2_COMPLETE_TOKEN_PROOF_ACCOUNT")
	}
	code, err := batch.readProvenViewCode(reader, token, account.Account.CodeHash)
	if err != nil {
		return err
	}
	var delegate *committedViewDelegate
	slots := map[common.Hash]common.Hash{}
	if batch.adapter.Chain.Key == "evm:10" && token == opUSDCAddress {
		delegate, err = batch.opUSDCDelegate(reader, code, slots)
		if err != nil {
			return err
		}
	}
	input := append(common.FromHex("0x70a08231"), common.LeftPadBytes(common.FromHex(owner), 32)...)
	for round := 0; round <= committedViewSlots; round++ {
		output, missing, err := executeCommittedViewWithDelegate(ctx, code, common.HexToAddress(token), input, slots, delegate)
		if missing == nil {
			if err != nil {
				return err
			}
			actual, err := domain.AmountFromBytes(output)
			if err != nil {
				return err
			}
			if actual != expected {
				return errors.New("V2_COMPLETE_TOKEN_PROOF_BALANCE")
			}
			return ctx.Err()
		}
		if len(slots) >= committedViewSlots {
			return errors.New("V2_COMPLETE_TOKEN_PROOF_SLOT_LIMIT")
		}
		// A sealed empty storage root proves zero without a separate key request.
		value, e := account.StorageValue(missing.Hex())
		if e != nil {
			proof, e := batch.read(token, []string{missing.Hex()})
			if e != nil {
				return e
			}
			if proof.Account.CodeHash != account.Account.CodeHash || proof.Account.StorageRoot != account.Account.StorageRoot {
				return errors.New("V2_COMPLETE_TOKEN_PROOF_CHANGED")
			}
			value, e = proof.StorageValue(missing.Hex())
			if e != nil {
				return e
			}
		}
		slots[*missing] = common.HexToHash(value)
	}
	return errors.New("V2_COMPLETE_TOKEN_PROOF_SLOT_LIMIT")
}

func (batch *accountProofBatch) readProvenViewCode(reader *Adapter, address, codeHash string) ([]byte, error) {
	var encoded string
	if err := reader.Client.Call(batch.ctx, "eth_getCode", []any{address, map[string]any{"blockHash": batch.block.Hash, "requireCanonical": true}}, &encoded); err != nil {
		return nil, err
	}
	if len(encoded) <= 2 || len(encoded) > 2+2*24576 {
		return nil, errors.New("V2_COMPLETE_TOKEN_PROOF_CODE")
	}
	code, err := v2CompleteDecode(encoded, (len(encoded)-2)/2)
	if err != nil {
		return nil, err
	}
	if crypto.Keccak256Hash(code).Hex() != codeHash {
		return nil, errors.New("V2_COMPLETE_TOKEN_PROOF_CODE")
	}
	return code, nil
}
