package evm

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"math/big"
	"strings"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/crypto"
	"xone/chain-parser-go/internal/domain"
)

// A verified native header/receipt root does not authenticate eth_call output.
// Bind this pinned Pair/Factory layout to the same execution-state commitment.
// Token balanceOf still requires its own code/layout or execution proof; this
// check does not grant complete-bank or consensus/production VERIFIED authority.
func (a *Adapter) verifyNativeV2CompleteState(ctx context.Context, reader *Adapter, block RawBlock, pool domain.Pool) (err error) {
	if a.Chain.BlockIntegrity != ChainNativeCommitmentsV2 {
		return nil
	}
	if reader == nil || reader.Client == nil || block.nativeSession == nil || block.nativeSession.ctx != ctx || pool.Protocol != domain.V2CompleteProtocol || pool.V2 == nil || pool.V2.Swap == nil {
		return errors.New("V2_COMPLETE_STATE_PROOF_CONTEXT")
	}
	defer func() {
		if err != nil {
			block.nativeSession.mu.Lock()
			block.nativeSession.remaining = 0
			block.nativeSession.mu.Unlock()
		}
	}()
	if err = pool.Validate(); err != nil {
		return err
	}
	swap := pool.V2.Swap
	number, parseErr := canonicalIndex(block.Number)
	if parseErr != nil {
		return parseErr
	}
	timestamp, parseErr := canonicalIndex(block.Timestamp)
	if parseErr != nil {
		return parseErr
	}
	if swap.BankUnixTimestamp != timestamp || swap.BankBlockNumber != number || swap.BankBlockHash != block.Hash || pool.ChainKey != a.Chain.Key {
		return errors.New("V2_COMPLETE_STATE_PROOF_ANCHOR")
	}
	packed := new(big.Int).Set(pool.Reserve1.Big())
	packed.Lsh(packed, 112).Or(packed, pool.Reserve0.Big())
	packed.Or(packed, new(big.Int).Lsh(new(big.Int).SetUint64(uint64(swap.BlockTimestampLast)), 224))
	expected := map[string]*big.Int{
		"0x5": new(big.Int).SetBytes(common.FromHex(pool.V2.Factory)),
		"0x6": new(big.Int).SetBytes(common.FromHex(pool.Token0)),
		"0x7": new(big.Int).SetBytes(common.FromHex(pool.Token1)),
		"0x8": packed, "0x9": swap.Price0CumulativeLast.Big(), "0xa": swap.Price1CumulativeLast.Big(), "0xc": big.NewInt(1),
	}
	if err = a.verifyV2CompleteAccount(ctx, reader, block, pool.PoolID, domain.V2CompletePairCodeSHA256, []string{"0x5", "0x6", "0x7", "0x8", "0x9", "0xa", "0xc"}, expected); err != nil {
		return err
	}
	keys := []string{v2CompletePairMapping(pool.Token0, pool.Token1), v2CompletePairMapping(pool.Token1, pool.Token0)}
	pair := new(big.Int).SetBytes(common.FromHex(pool.PoolID))
	return a.verifyV2CompleteAccount(ctx, reader, block, pool.V2.Factory, domain.V2CompleteFactoryCodeSHA256, keys, map[string]*big.Int{keys[0]: pair, keys[1]: pair})
}

func v2CompletePairMapping(first, second string) string {
	slot := common.BigToHash(big.NewInt(2))
	outer := crypto.Keccak256(common.LeftPadBytes(common.FromHex(first), 32), slot.Bytes())
	return crypto.Keccak256Hash(common.LeftPadBytes(common.FromHex(second), 32), outer).Hex()
}

func (a *Adapter) verifyV2CompleteAccount(ctx context.Context, reader *Adapter, block RawBlock, address, codeSHA string, keys []string, expected map[string]*big.Int) error {
	proof, err := a.ReadNativeContractAccountProof(ctx, block, address, keys)
	if err != nil {
		return err
	}
	if !proof.Account.Exists || !strings.EqualFold(proof.Account.Address, address) {
		return errors.New("V2_COMPLETE_STATE_PROOF_ACCOUNT")
	}
	// SHA-256 pins the audited source runtime; Keccak binds those same bytes to
	// the MPT account. A matching configured hash alone supplies no state proof.
	var code string
	if err = reader.Client.Call(ctx, "eth_getCode", []any{address, map[string]any{"blockHash": block.Hash, "requireCanonical": true}}, &code); err != nil {
		return err
	}
	if len(code) <= 2 || len(code) > 2+2*24576 {
		return errors.New("V2_COMPLETE_STATE_PROOF_CODE")
	}
	raw, err := v2CompleteDecode(code, (len(code)-2)/2)
	if err != nil {
		return err
	}
	digest := sha256.Sum256(raw)
	if hex.EncodeToString(digest[:]) != codeSHA || crypto.Keccak256Hash(raw).Hex() != proof.Account.CodeHash {
		return errors.New("V2_COMPLETE_STATE_PROOF_CODE")
	}
	for _, key := range keys {
		logical := common.HexToHash(key).Hex()
		value, err := proof.StorageValue(logical)
		if err != nil {
			return err
		}
		actual, ok := new(big.Int).SetString(value[2:], 16)
		if !ok || expected[key] == nil || actual.Cmp(expected[key]) != 0 {
			return fmt.Errorf("V2_COMPLETE_STATE_PROOF_VALUE:%s", logical)
		}
	}
	return ctx.Err()
}
