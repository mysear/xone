package evm

import (
	"encoding/hex"
	"errors"
	"fmt"
	"math/big"
	"strings"
	"xone/chain-parser-go/internal/domain"
)

// Slot definitions are bound to the fixed published compiler input/AST and the
// exact runtime templates checked by the common Managed reader. No caller may
// supply a layout or a substitute getter for these private storage fields.
func managedV2ScheduledTokenSlot(token string) string {
	encoded := append(managedV2AddressWord(token), managedV2Word(big.NewInt(11))...)
	return "0x" + hex.EncodeToString(managedV2Hash(string(encoded)))
}
func (r *managedV2Reader) storageWord(to, slot string) domain.Amount {
	if r.err != nil {
		return "0"
	}
	var value string
	e := r.a.Client.Call(r.ctx, "eth_getStorageAt", []any{to, slot, r.anchor}, &value)
	var bytes []byte
	if e == nil {
		if len(value) != 66 || !strings.HasPrefix(value, "0x") {
			e = errors.New("MANAGED_V2_SCHEDULE_STORAGE_WIDTH")
		} else {
			bytes, e = hex.DecodeString(value[2:])
		}
	}
	if e != nil {
		r.err = fmt.Errorf("MANAGED_V2_SCHEDULE_STORAGE %s: %w", slot, e)
		return "0"
	}
	return domain.Amount(new(big.Int).SetBytes(bytes).String())
}
func managedV2ScheduledBits(word *big.Int, offset, width uint) *big.Int {
	shifted := new(big.Int).Rsh(new(big.Int).Set(word), offset)
	return shifted.And(shifted, new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), width), big.NewInt(1)))
}
func (a *Adapter) readManagedV2ScheduledState(r *managedV2Reader, pool string, state *domain.MultiAssetState, fee *big.Int) {
	m := state.ManagedV2
	raw := &domain.ManagedV2Schedule{PoolStateWord: r.storageWord(pool, "0x9"), Tokens: []domain.ManagedV2ScheduledToken{}}
	for _, asset := range state.Assets {
		raw.Tokens = append(raw.Tokens, domain.ManagedV2ScheduledToken{Token: asset.Token, StateWord: r.storageWord(pool, managedV2ScheduledTokenSlot(asset.Token))})
	}
	if r.err != nil {
		return
	}
	view, e := raw.At(m.Timestamp)
	if e != nil {
		r.require(false, "SCHEDULE_PACKED_STATE")
		return
	}
	m.Schedule = raw
	m.GradualWeightEnd = view.WeightEnd
	m.GradualFeeEnd = view.FeeEnd
	r.require(view.SwapsEnabled == m.SwapsEnabled && view.MustAllowlistLPs == m.AllowlistEnabled && view.RecoveryMode == m.RecoveryMode, "SCHEDULE_FLAGS")
	r.require(r.boolean(pool, "getJoinExitEnabled()", "") == view.JoinExitEnabled, "SCHEDULE_JOIN_FLAG")
	r.require(fee.String() == view.SwapFee.String(), "SCHEDULE_CURRENT_FEE")
	params := r.words(r.call(pool, "getGradualSwapFeeUpdateParams()", "", 4), 4)
	word := raw.PoolStateWord.Big()
	for i, span := range []struct{ offset, width uint }{{64, 32}, {96, 32}, {128, 62}, {190, 62}} {
		r.require(r.uintWord(params[i], int(span.width)).Cmp(managedV2ScheduledBits(word, span.offset, span.width)) == 0, "SCHEDULE_FEE_FIELDS")
	}
	count := len(state.Assets)
	b := r.call(pool, "getGradualWeightUpdateParams()", "", -1)
	r.require(len(b) == (6+2*count)*32, "SCHEDULE_WEIGHT_ABI_SIZE")
	if r.err != nil {
		return
	}
	r.require(r.uintWord(b[:32], 32).Uint64() == view.WeightStart && r.uintWord(b[32:64], 32).Uint64() == view.WeightEnd, "SCHEDULE_WEIGHT_TIMES")
	r.require(r.uintWord(b[64:96], 64).Uint64() == 128 && r.uintWord(b[96:128], 64).Uint64() == uint64((5+count)*32) && r.uintWord(b[128:160], 64).Uint64() == uint64(count) && r.uintWord(b[(5+count)*32:(6+count)*32], 64).Uint64() == uint64(count), "SCHEDULE_WEIGHT_ABI_LAYOUT")
	for i, asset := range state.Assets {
		tokenWord := raw.Tokens[i].StateWord.Big()
		start := r.uintWord(b[(5+i)*32:(6+i)*32], 64)
		end := r.uintWord(b[(6+count+i)*32:(7+count+i)*32], 64)
		r.require(start.Cmp(managedV2ScheduledBits(tokenWord, 0, 64)) == 0 && end.Cmp(managedV2ScheduledBits(tokenWord, 64, 64)) == 0, "SCHEDULE_WEIGHT_FIELDS")
		r.require(view.Weights[i] == state.Weights[i] && view.Scaling[i] == asset.Scaling, "SCHEDULE_CURRENT_WEIGHT_SCALE")
	}
}
