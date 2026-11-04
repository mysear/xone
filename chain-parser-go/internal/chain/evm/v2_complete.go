package evm

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"strings"

	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
)

// v2CompleteDecode accepts only an exact JSON-RPC DATA value. Checking its
// encoded size first also bounds allocation before any ABI decoding.
func v2CompleteDecode(value string, size int) ([]byte, error) {
	if !strings.HasPrefix(value, "0x") || len(value) != 2+size*2 {
		return nil, errors.New("V2_COMPLETE_ABI_SIZE")
	}
	raw, e := hex.DecodeString(value[2:])
	if e != nil {
		return nil, errors.New("V2_COMPLETE_ABI_HEX")
	}
	return raw, nil
}
func v2CompleteHash(hash string) bool {
	if len(hash) != 66 || hash != strings.ToLower(hash) {
		return false
	}
	_, e := v2CompleteDecode(hash, 32)
	return e == nil && hash != "0x"+strings.Repeat("0", 64)
}

// Only genesis may have a zero parent. The block's own hash still uses the
// strict nonzero identity check above.
func v2CompleteParentHash(number uint64, hash string) bool {
	return v2CompleteHash(hash) || (number == 0 && hash == "0x"+strings.Repeat("0", 64))
}

// Complete-state banks must be anchored by canonical header encodings before
// the shared reader normalizes receipt identities for legacy protocols.
func validateV2CompleteHeader(b RawBlock) error {
	number, e := canonicalIndex(b.Number)
	if e != nil {
		return e
	}
	if !v2CompleteHash(b.Hash) || !v2CompleteParentHash(number, b.ParentHash) {
		return errors.New("V2_COMPLETE_HEADER_HASH")
	}
	if _, e := canonicalIndex(b.Timestamp); e != nil {
		return e
	}
	return nil
}

func (a *Adapter) readV2Complete(ctx context.Context, p config.PoolConfig, anchor map[string]any, ref domain.BlockRef) (domain.Pool, error) {
	fail := func(code string) (domain.Pool, error) { return domain.Pool{}, errors.New(code) }
	profile, ok := domain.Protocol(p.Protocol)
	if !ok || p.Protocol != domain.V2CompleteProtocol || profile.Family != "v2" || profile.AdapterVersion != domain.V2CompleteAdapterVersion {
		return fail("V2_COMPLETE_PROFILE")
	}
	if p.V2 == nil {
		return fail("V2_IDENTITY_REQUIRED")
	}
	if p.V2.ExpectedCodeSHA256 != domain.V2CompletePairCodeSHA256 {
		return fail("V2_COMPLETE_PAIR_CODE")
	}
	if p.FeeNumerator != 3 || p.FeeDenominator != 1000 {
		return fail("PROTOCOL_FEE_MISMATCH")
	}
	if ref.ChainKey != a.Chain.Key || !strings.HasPrefix(ref.ChainKey, "evm:") || domain.ValidateChain(ref.ChainKey) != nil || !v2CompleteHash(ref.Hash) || !v2CompleteParentHash(ref.Number, ref.ParentHash) || ref.TimestampMS%1000 != 0 || len(anchor) != 2 || anchor["blockHash"] != ref.Hash || anchor["requireCanonical"] != true {
		return fail("V2_COMPLETE_ANCHOR")
	}
	zero := "0x" + strings.Repeat("0", 40)
	for _, address := range []string{p.ID, p.Token0, p.Token1, p.V2.ExpectedFactory} {
		if domain.ValidateEVMAddress(address) != nil || address == zero {
			return fail("V2_COMPLETE_ADDRESS")
		}
	}
	if p.Token0 >= p.Token1 || p.ID == p.Token0 || p.ID == p.Token1 || p.ID == p.V2.ExpectedFactory {
		return fail("V2_COMPLETE_TOKEN_ORDER")
	}
	// The base verifies actual Pair code, factory(), token0/1(), and all three
	// getReserves words without validating the still-incomplete pool.
	pool, last, e := a.readV2Base(ctx, p, anchor)
	if e != nil {
		return domain.Pool{}, e
	}
	var code string
	if e = a.Client.Call(ctx, "eth_getCode", []any{p.V2.ExpectedFactory, anchor}, &code); e != nil {
		return domain.Pool{}, e
	}
	// EIP-170 bounds the admitted original runtime, not an operator-selected
	// layout. The pinned hash must match before private slot 12 is interpreted.
	if !strings.HasPrefix(code, "0x") || len(code) <= 2 || len(code) > 2+2*24576 || len(code)%2 != 0 {
		return fail("V2_COMPLETE_FACTORY_CODE")
	}
	raw, e := hex.DecodeString(code[2:])
	if e != nil {
		return fail("V2_COMPLETE_FACTORY_CODE")
	}
	digest := sha256.Sum256(raw)
	if hex.EncodeToString(digest[:]) != domain.V2CompleteFactoryCodeSHA256 {
		return fail("V2_COMPLETE_FACTORY_CODE")
	}
	call := func(to, data string) ([]byte, error) {
		var result string
		if e := a.Client.Call(ctx, "eth_call", []any{map[string]string{"to": to, "data": data}, anchor}, &result); e != nil {
			return nil, e
		}
		return v2CompleteDecode(result, 32)
	}
	addressWord := func(address string) string { return strings.Repeat("0", 24) + address[2:] }
	for _, tokens := range [][2]string{{p.Token0, p.Token1}, {p.Token1, p.Token0}} {
		raw, e := call(p.V2.ExpectedFactory, "0xe6a43905"+addressWord(tokens[0])+addressWord(tokens[1]))
		if e != nil {
			return domain.Pool{}, e
		}
		if _, e = wordUint(raw, 160); e != nil || "0x"+hex.EncodeToString(raw[12:]) != p.ID {
			return fail("V2_COMPLETE_FACTORY_PAIR")
		}
	}
	swap := &domain.V2SwapState{BlockTimestampLast: last, Unlocked: true, FactoryCodeSHA256: domain.V2CompleteFactoryCodeSHA256, BankBlockNumber: ref.Number, BankBlockHash: ref.Hash, BankUnixTimestamp: ref.TimestampMS / 1000}
	for _, entry := range []struct {
		to, data string
		dest     *domain.Amount
	}{
		{p.Token0, "0x70a08231" + addressWord(p.ID), &swap.RawBalance0},
		{p.Token1, "0x70a08231" + addressWord(p.ID), &swap.RawBalance1},
		{p.ID, "0x5909c0d5", &swap.Price0CumulativeLast},
		{p.ID, "0x5a3d5493", &swap.Price1CumulativeLast},
	} {
		raw, e := call(entry.to, entry.data)
		if e != nil {
			return domain.Pool{}, e
		}
		*entry.dest, e = domain.AmountFromBytes(raw)
		if e != nil {
			return domain.Pool{}, e
		}
	}
	var lock string
	if e = a.Client.Call(ctx, "eth_getStorageAt", []any{p.ID, "0xc", anchor}, &lock); e != nil {
		return domain.Pool{}, e
	}
	lockWord, e := v2CompleteDecode(lock, 32)
	if e != nil {
		return domain.Pool{}, e
	}
	unlocked, e := wordUint(lockWord, 256)
	if e != nil || !unlocked.IsUint64() || unlocked.Uint64() != 1 {
		return fail("V2_COMPLETE_LOCKED_OR_UNKNOWN")
	}
	pool.V2.Swap = swap
	return pool, pool.Validate()
}
