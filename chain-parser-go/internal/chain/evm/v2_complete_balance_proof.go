package evm

import (
	"context"
	"errors"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/crypto"
	"xone/chain-parser-go/internal/domain"
)

// Authenticate the actual code and each reached SLOAD instead of assuming a
// universal ERC20 storage layout. This only proves this zero-caller view; token
// transfer behavior, consensus authority and production execution remain separate.
func (a *Adapter) verifyNativeV2Balance(ctx context.Context, reader *Adapter, block RawBlock, token, owner string, expected domain.Amount) error {
	account, err := a.ReadNativeContractAccountProof(ctx, block, token, nil)
	if err != nil {
		return err
	}
	if !account.Account.Exists {
		return errors.New("V2_COMPLETE_TOKEN_PROOF_ACCOUNT")
	}
	var encoded string
	if err = reader.Client.Call(ctx, "eth_getCode", []any{token, map[string]any{"blockHash": block.Hash, "requireCanonical": true}}, &encoded); err != nil {
		return err
	}
	if len(encoded) <= 2 || len(encoded) > 2+2*24576 {
		return errors.New("V2_COMPLETE_TOKEN_PROOF_CODE")
	}
	code, err := v2CompleteDecode(encoded, (len(encoded)-2)/2)
	if err != nil {
		return err
	}
	if crypto.Keccak256Hash(code).Hex() != account.Account.CodeHash {
		return errors.New("V2_COMPLETE_TOKEN_PROOF_CODE")
	}
	input := append(common.FromHex("0x70a08231"), common.LeftPadBytes(common.FromHex(owner), 32)...)
	slots := map[common.Hash]common.Hash{}
	for round := 0; round <= committedViewSlots; round++ {
		output, missing, err := executeCommittedView(ctx, code, common.HexToAddress(token), input, slots)
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
		if round == committedViewSlots {
			return errors.New("V2_COMPLETE_TOKEN_PROOF_SLOT_LIMIT")
		}
		// A sealed empty storage root proves zero without a separate key request.
		value, e := account.StorageValue(missing.Hex())
		if e != nil {
			proof, e := a.ReadNativeContractAccountProof(ctx, block, token, []string{missing.Hex()})
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
