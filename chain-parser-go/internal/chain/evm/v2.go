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

func (a *Adapter) readV2(ctx context.Context, p config.PoolConfig, anchor map[string]any) (domain.Pool, error) {
	pool, _, e := a.readV2Base(ctx, p, anchor)
	if e != nil {
		return domain.Pool{}, e
	}
	return pool, pool.Validate()
}

// readV2Base leaves final validation to its caller so a complete profile can
// attach its mandatory swap state before domain validation.
func (a *Adapter) readV2Base(ctx context.Context, p config.PoolConfig, anchor map[string]any) (domain.Pool, uint32, error) {
	profile, ok := domain.Protocol(p.Protocol)
	if !ok || profile.Family != "v2" {
		return domain.Pool{}, 0, errors.New("PROTOCOL_UNSUPPORTED")
	}
	if profile.RequireIdentity && p.V2 == nil {
		return domain.Pool{}, 0, errors.New("V2_IDENTITY_REQUIRED")
	}
	if profile.FeeDenominator != 0 && (p.FeeNumerator != profile.FeeNumerator || p.FeeDenominator != profile.FeeDenominator) {
		return domain.Pool{}, 0, errors.New("PROTOCOL_FEE_MISMATCH")
	}
	call := func(selector string, words int) ([]byte, error) {
		var out string
		if e := a.Client.Call(ctx, "eth_call", []any{map[string]string{"to": p.ID, "data": selector}, anchor}, &out); e != nil {
			return nil, e
		}
		if p.Protocol == domain.V2CompleteProtocol {
			return v2CompleteDecode(out, words*32)
		}
		raw, e := hex.DecodeString(strings.TrimPrefix(out, "0x"))
		if e != nil || len(raw) != words*32 {
			return nil, errors.New("V2_ABI_SIZE")
		}
		return raw, nil
	}
	var identity *domain.V2State
	if p.V2 != nil {
		if e := domain.ValidateEVMIdentity(p.V2.ExpectedFactory, p.V2.ExpectedCodeSHA256); e != nil {
			return domain.Pool{}, 0, e
		}
		var code string
		if e := a.Client.Call(ctx, "eth_getCode", []any{p.ID, anchor}, &code); e != nil {
			return domain.Pool{}, 0, e
		}
		if p.Protocol == domain.V2CompleteProtocol && (!strings.HasPrefix(code, "0x") || len(code) > 2+2*24576) {
			return domain.Pool{}, 0, errors.New("V2_CODE_IDENTITY")
		}
		raw, e := hex.DecodeString(strings.TrimPrefix(code, "0x"))
		if e != nil || len(raw) == 0 {
			return domain.Pool{}, 0, errors.New("V2_CODE_IDENTITY")
		}
		h := sha256.Sum256(raw)
		if hex.EncodeToString(h[:]) != p.V2.ExpectedCodeSHA256 {
			return domain.Pool{}, 0, errors.New("V2_CODE_HASH_MISMATCH")
		}
		b, e := call("0xc45a0155", 1)
		if e != nil {
			return domain.Pool{}, 0, e
		}
		if strings.Trim(hex.EncodeToString(b[:12]), "0") != "" || "0x"+hex.EncodeToString(b[12:]) != p.V2.ExpectedFactory {
			return domain.Pool{}, 0, errors.New("V2_FACTORY_MISMATCH")
		}
		identity = &domain.V2State{Factory: p.V2.ExpectedFactory, CodeSHA256: p.V2.ExpectedCodeSHA256}
	}
	for _, entry := range []struct{ selector, expected string }{{"0x0dfe1681", p.Token0}, {"0xd21220a7", p.Token1}} {
		b, e := call(entry.selector, 1)
		if e != nil {
			return domain.Pool{}, 0, e
		}
		if strings.Trim(hex.EncodeToString(b[:12]), "0") != "" || !strings.EqualFold("0x"+hex.EncodeToString(b[12:]), entry.expected) {
			return domain.Pool{}, 0, errors.New("POOL_TOKEN_IDENTITY")
		}
	}
	b, e := call("0x0902f1ac", 3)
	if e != nil {
		return domain.Pool{}, 0, e
	}
	r0, e := wordUint(b[:32], 112)
	if e != nil {
		return domain.Pool{}, 0, errors.New("V2_RESERVE_WIDTH")
	}
	r1, e := wordUint(b[32:64], 112)
	if e != nil {
		return domain.Pool{}, 0, errors.New("V2_RESERVE_WIDTH")
	}
	last, e := wordUint(b[64:], 32)
	if e != nil {
		return domain.Pool{}, 0, errors.New("V2_TIMESTAMP_WIDTH")
	}
	pool := domain.Pool{ChainKey: a.Chain.Key, PoolID: p.ID, Protocol: p.Protocol, Token0: p.Token0, Token1: p.Token1, Reserve0: domain.Amount(r0.String()), Reserve1: domain.Amount(r1.String()), FeeNumerator: p.FeeNumerator, FeeDenominator: p.FeeDenominator, AdapterVersion: profile.AdapterVersion, TokenBehavior: "standard", V2: identity}
	return pool, uint32(last.Uint64()), nil
}
