package evm

import (
	"context"
	"encoding/hex"
	"errors"
	"math/big"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
)

const selectorWeights = "0xf89f27ed"   // getNormalizedWeights()
const selectorTokenInfo = "0xb05f8e48" // getPoolTokenInfo(bytes32,address)
const selectorDecimalsWeighted = "0x313ce567"

// readWeighted reads all dependencies at one EIP-1898 blockHash. Its caller
// retains receipts-root validation and the after-read canonical anchor check.
func (a *Adapter) readWeighted(ctx context.Context, p config.PoolConfig, anchor map[string]any, blockNumber uint64) (domain.Pool, error) {
	v := p.Weighted
	profile, known := domain.Protocol(p.Protocol)
	if !known || profile.Family != "weighted" || v == nil || v.TokenPolicy != "standard-no-bpt" {
		return domain.Pool{}, errors.New("WEIGHTED_CONFIG_REQUIRED")
	}
	fail := func() (domain.Pool, error) { return domain.Pool{}, errors.New("WEIGHTED_RPC_STATE") }
	// Constructor-initialized immutable pools do not admit ordinary EIP-1167 clones.
	for _, id := range [][2]string{{p.ID, v.ExpectedCodeSHA256}, {v.ExpectedVault, v.ExpectedVaultCodeSHA256}, {p.Token0, v.ExpectedToken0CodeSHA256}, {p.Token1, v.ExpectedToken1CodeSHA256}} {
		code, e := a.verifyCode(ctx, id[0], id[1], anchor)
		if e != nil {
			return domain.Pool{}, e
		}
		if isStandardClone(code) {
			return domain.Pool{}, errors.New("WEIGHTED_PROXY_UNSUPPORTED")
		}
	}
	call := func(to, data string, words int) ([]byte, error) { return a.stableCall(ctx, to, data, anchor, words) }
	address := func(b []byte) (string, error) {
		if _, e := wordUint(b, 160); e != nil {
			return "", e
		}
		return "0x" + hex.EncodeToString(b[12:]), nil
	}
	b, e := call(p.ID, selectorGetVault, 1)
	if e != nil {
		return domain.Pool{}, e
	}
	got, e := address(b)
	if e != nil || got != v.ExpectedVault {
		return fail()
	}
	b, e = call(p.ID, selectorGetPoolID, 1)
	if e != nil {
		return domain.Pool{}, e
	}
	if "0x"+hex.EncodeToString(b) != v.PoolID {
		return fail()
	}
	b, e = call(v.ExpectedVault, selectorGetPool+v.PoolID[2:], 2)
	if e != nil {
		return domain.Pool{}, e
	}
	got, e = address(b[:32])
	if e != nil || got != p.ID || new(big.Int).SetBytes(b[32:]).Cmp(big.NewInt(2)) != 0 {
		return fail()
	}
	for _, target := range []string{p.ID, v.ExpectedVault} {
		b, e = call(target, selectorPausedState, 3)
		if e != nil {
			return domain.Pool{}, e
		}
		if new(big.Int).SetBytes(b[:32]).Sign() != 0 {
			return fail()
		}
	}
	pair := func(selector string) ([2]domain.Amount, error) {
		var out [2]domain.Amount
		b, e := call(p.ID, selector, -1)
		if e != nil {
			return out, e
		}
		if len(b) != 128 || new(big.Int).SetBytes(b[:32]).Cmp(big.NewInt(32)) != 0 || new(big.Int).SetBytes(b[32:64]).Cmp(big.NewInt(2)) != 0 {
			return out, errors.New("WEIGHTED_ARRAY_ABI")
		}
		out[0] = domain.Amount(new(big.Int).SetBytes(b[64:96]).String())
		out[1] = domain.Amount(new(big.Int).SetBytes(b[96:128]).String())
		return out, nil
	}
	scales, e := pair(selectorScaling)
	if e != nil {
		return domain.Pool{}, e
	}
	weights, e := pair(selectorWeights)
	if e != nil {
		return domain.Pool{}, e
	}
	for i, token := range []string{p.Token0, p.Token1} {
		b, e = call(token, selectorDecimalsWeighted, 1)
		if e != nil {
			return domain.Pool{}, e
		}
		d, e := wordUint(b, 8)
		if e != nil || d.Uint64() > 18 || scales[i].Big().Cmp(new(big.Int).Exp(big.NewInt(10), big.NewInt(36-int64(d.Uint64())), nil)) != 0 {
			return fail()
		}
	}
	b, e = call(v.ExpectedVault, selectorPoolTokens+v.PoolID[2:], -1)
	if e != nil {
		return domain.Pool{}, e
	}
	if len(b) != 288 || new(big.Int).SetBytes(b[:32]).Cmp(big.NewInt(96)) != 0 || new(big.Int).SetBytes(b[32:64]).Cmp(big.NewInt(192)) != 0 || new(big.Int).SetBytes(b[96:128]).Cmp(big.NewInt(2)) != 0 || new(big.Int).SetBytes(b[192:224]).Cmp(big.NewInt(2)) != 0 {
		return fail()
	}
	last, e := wordUint(b[64:96], 64)
	if e != nil || last.Uint64() > blockNumber {
		return fail()
	}
	reserves := [2]domain.Amount{}
	for i, token := range []string{p.Token0, p.Token1} {
		got, e = address(b[128+i*32 : 160+i*32])
		if e != nil || got != token {
			return fail()
		}
		balance, e := wordUint(b[224+i*32:256+i*32], 112)
		if e != nil {
			return fail()
		}
		reserves[i] = domain.Amount(balance.String())
		info, e := call(v.ExpectedVault, selectorTokenInfo+v.PoolID[2:]+"000000000000000000000000"+token[2:], 4)
		if e != nil {
			return domain.Pool{}, e
		}
		cash, e := wordUint(info[:32], 112)
		if e != nil || cash.Cmp(balance) != 0 || new(big.Int).SetBytes(info[32:64]).Sign() != 0 || new(big.Int).SetBytes(info[64:96]).Cmp(last) > 0 || new(big.Int).SetBytes(info[96:128]).Sign() != 0 {
			return fail()
		}
	}
	b, e = call(p.ID, selectorSwapFee, 1)
	if e != nil {
		return domain.Pool{}, e
	}
	fee, e := wordUint(b, 64)
	if e != nil {
		return fail()
	}
	state := &domain.WeightedState{CodeSHA256: v.ExpectedCodeSHA256, Vault: v.ExpectedVault, VaultCodeSHA256: v.ExpectedVaultCodeSHA256, PoolID: v.PoolID, Scaling0: scales[0], Scaling1: scales[1], Weight0: weights[0], Weight1: weights[1], Token0CodeSHA256: v.ExpectedToken0CodeSHA256, Token1CodeSHA256: v.ExpectedToken1CodeSHA256}
	pool := domain.Pool{Weighted: state, ChainKey: a.Chain.Key, PoolID: p.ID, Protocol: p.Protocol, Token0: p.Token0, Token1: p.Token1, Reserve0: reserves[0], Reserve1: reserves[1], FeeNumerator: fee.Uint64(), FeeDenominator: 1000000000000000000, AdapterVersion: profile.AdapterVersion, TokenBehavior: "standard"}
	return pool, pool.Validate()
}
