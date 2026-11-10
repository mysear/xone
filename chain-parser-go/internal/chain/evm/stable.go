package evm

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"math/big"
	"strings"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
)

func (a *Adapter) stableCall(ctx context.Context, to, data string, anchor map[string]any, words int) ([]byte, error) {
	var result string
	if e := a.Client.Call(ctx, "eth_call", []any{map[string]string{"to": to, "data": data}, anchor}, &result); e != nil {
		return nil, e
	}
	b, e := hex.DecodeString(strings.TrimPrefix(result, "0x"))
	if e != nil || len(b)%32 != 0 || words >= 0 && len(b) != words*32 {
		return nil, errors.New("STABLE_ABI_SIZE")
	}
	return b, nil
}
func (a *Adapter) verifyCode(ctx context.Context, address, expected string, anchor map[string]any) ([]byte, error) {
	var result string
	if e := a.Client.Call(ctx, "eth_getCode", []any{address, anchor}, &result); e != nil {
		return nil, e
	}
	b, e := hex.DecodeString(strings.TrimPrefix(result, "0x"))
	if e != nil || len(b) == 0 {
		return nil, errors.New("STABLE_CODE_IDENTITY")
	}
	hash := sha256.Sum256(b)
	if hex.EncodeToString(hash[:]) != expected {
		return nil, errors.New("STABLE_CODE_HASH_MISMATCH")
	}
	return b, nil
}
func (a *Adapter) readStable(ctx context.Context, p config.PoolConfig, anchor map[string]any) (domain.Pool, error) {
	profile, ok := domain.Protocol(p.Protocol)
	v := p.Stable
	if !ok || v == nil {
		return domain.Pool{}, errors.New("STABLE_CONFIG_REQUIRED")
	}
	code, e := a.verifyCode(ctx, p.ID, v.ExpectedCodeSHA256, anchor)
	if e != nil {
		return domain.Pool{}, e
	}
	if e = a.verifyClone(ctx, code, v.ExpectedImplementation, v.ExpectedImplementationCodeSHA256, anchor); e != nil {
		return domain.Pool{}, e
	}
	state := &domain.StableState{CodeSHA256: v.ExpectedCodeSHA256, Implementation: v.ExpectedImplementation, ImplementationCodeSHA256: v.ExpectedImplementationCodeSHA256, Factory: v.ExpectedFactory, FactoryCodeSHA256: v.ExpectedFactoryCodeSHA256, Vault: v.ExpectedVault, VaultCodeSHA256: v.ExpectedVaultCodeSHA256, PoolID: v.PoolID, Rate0: "0", Rate1: "0", Amplification: "0", MaxIterations: 255}
	call := func(to, selector string, words int) ([]byte, error) {
		return a.stableCall(ctx, to, selector, anchor, words)
	}
	addr := func(word []byte) (string, error) {
		if _, e := wordUint(word, 160); e != nil {
			return "", e
		}
		return "0x" + hex.EncodeToString(word[12:]), nil
	}
	tokenCheck := func(target, selector, expected string) error {
		b, e := call(target, selector, 1)
		if e != nil {
			return e
		}
		got, e := addr(b)
		if e != nil || got != expected {
			return errors.New("STABLE_TOKEN_IDENTITY")
		}
		return nil
	}
	uintCall := func(target, selector string) (*big.Int, error) {
		b, e := call(target, selector, 1)
		if e != nil {
			return nil, e
		}
		return wordUint(b, 256)
	}
	boolCall := func(target, selector string) (bool, error) {
		b, e := call(target, selector, 1)
		if e != nil {
			return false, e
		}
		n, e := wordUint(b, 1)
		return e == nil && n.Sign() != 0, e
	}
	var r0, r1, fee *big.Int
	switch profile.Family {
	case "aero":
		if dependency, err := a.verifyCode(ctx, v.ExpectedFactory, v.ExpectedFactoryCodeSHA256, anchor); err != nil || isStandardClone(dependency) {
			if err == nil {
				err = errors.New("STABLE_DEPENDENCY_PROXY_UNSUPPORTED")
			}
			return domain.Pool{}, err
		}

		if e = tokenCheck(p.ID, "0xc45a0155", v.ExpectedFactory); e != nil {
			return domain.Pool{}, e
		}
		paused, e := boolCall(v.ExpectedFactory, selectorIsPaused)
		if e != nil {
			return domain.Pool{}, e
		}
		if paused {
			return domain.Pool{}, errors.New("AERO_FACTORY_PAUSED")
		}
		meta, e := call(p.ID, selectorMetadata, 7)
		if e != nil {
			return domain.Pool{}, e
		}
		stableFlag, e := wordUint(meta[128:160], 1)
		if e != nil {
			return domain.Pool{}, e
		}
		if (stableFlag.Sign() != 0) != (p.Protocol == "aerodrome-v2-stable" || p.Protocol == "velodrome-v2-stable") {
			return domain.Pool{}, errors.New("AERO_MODEL_MISMATCH")
		}
		for i, expected := range []string{p.Token0, p.Token1} {
			got, e := addr(meta[(5+i)*32 : (6+i)*32])
			if e != nil || got != expected {
				return domain.Pool{}, errors.New("STABLE_TOKEN_IDENTITY")
			}
		}
		state.Rate0 = domain.Amount(new(big.Int).SetBytes(meta[:32]).String())
		state.Rate1 = domain.Amount(new(big.Int).SetBytes(meta[32:64]).String())
		r0 = new(big.Int).SetBytes(meta[64:96])
		r1 = new(big.Int).SetBytes(meta[96:128])
		fee, e = uintCall(v.ExpectedFactory, selectorGetFee+strings.Repeat("0", 24)+p.ID[2:]+fmt.Sprintf("%064x", stableFlag))
		if e != nil {
			return domain.Pool{}, e
		}
	case "curve":
		for i, expected := range []string{p.Token0, p.Token1} {
			if e = tokenCheck(p.ID, selectorCoins+fmt.Sprintf("%064x", i), expected); e != nil {
				return domain.Pool{}, e
			}
		}
		r0, e = uintCall(p.ID, selectorBalances+fmt.Sprintf("%064x", 0))
		if e != nil {
			return domain.Pool{}, e
		}
		r1, e = uintCall(p.ID, selectorBalances+fmt.Sprintf("%064x", 1))
		if e != nil {
			return domain.Pool{}, e
		}
		fee, e = uintCall(p.ID, selectorFee)
		if e != nil {
			return domain.Pool{}, e
		}
		amp, e := uintCall(p.ID, selectorAPrecise)
		if e != nil {
			return domain.Pool{}, e
		}
		state.Amplification = domain.Amount(amp.String())
		admin, e := uintCall(p.ID, selectorAdminFee)
		if e != nil || !admin.IsUint64() {
			return domain.Pool{}, errors.New("CURVE_ADMIN_FEE")
		}
		state.AdminFee = admin.Uint64()
		// Plain2Basic v5 uses private immutable-rate storage slots11/12, pinned
		// by exact runtime/implementation code and Vyper0.3.1 layout evidence.
		for i := 0; i < 2; i++ {
			var value string
			if e = a.Client.Call(ctx, "eth_getStorageAt", []any{p.ID, fmt.Sprintf("0x%x", 11+i), anchor}, &value); e != nil {
				return domain.Pool{}, e
			}
			raw, e := hex.DecodeString(strings.TrimPrefix(value, "0x"))
			if e != nil || len(raw) != 32 {
				return domain.Pool{}, errors.New("CURVE_RATE_STORAGE_WIDTH")
			}
			rate := domain.Amount(new(big.Int).SetBytes(raw).String())
			if i == 0 {
				state.Rate0 = rate
			} else {
				state.Rate1 = rate
			}
		}
	case "balancer":
		if dependency, err := a.verifyCode(ctx, v.ExpectedVault, v.ExpectedVaultCodeSHA256, anchor); err != nil || isStandardClone(dependency) {
			if err == nil {
				err = errors.New("STABLE_DEPENDENCY_PROXY_UNSUPPORTED")
			}
			return domain.Pool{}, err
		}

		if e = tokenCheck(p.ID, selectorGetVault, v.ExpectedVault); e != nil {
			return domain.Pool{}, e
		}
		id, e := call(p.ID, selectorGetPoolID, 1)
		if e != nil || "0x"+hex.EncodeToString(id) != v.PoolID {
			return domain.Pool{}, errors.New("BALANCER_POOL_ID_MISMATCH")
		}
		registered, e := call(v.ExpectedVault, selectorGetPool+v.PoolID[2:], 2)
		if e != nil {
			return domain.Pool{}, e
		}
		poolAddress, e := addr(registered[:32])
		if e != nil || poolAddress != p.ID {
			return domain.Pool{}, errors.New("BALANCER_VAULT_REGISTRATION")
		}
		specialization, e := wordUint(registered[32:], 8)
		if e != nil || specialization.Uint64() > 2 {
			return domain.Pool{}, errors.New("BALANCER_SPECIALIZATION")
		}
		vaultPause, e := call(v.ExpectedVault, selectorPausedState, 3)
		if e != nil {
			return domain.Pool{}, e
		}
		vaultFlag, e := wordUint(vaultPause[:32], 1)
		if e != nil || vaultFlag.Sign() != 0 {
			return domain.Pool{}, errors.New("BALANCER_VAULT_PAUSED")
		}
		pause, e := call(p.ID, selectorPausedState, 3)
		if e != nil {
			return domain.Pool{}, e
		}
		flag, e := wordUint(pause[:32], 1)
		if e != nil || flag.Sign() != 0 {
			return domain.Pool{}, errors.New("BALANCER_POOL_PAUSED")
		}
		amp, e := call(p.ID, selectorAmplification, 3)
		if e != nil {
			return domain.Pool{}, e
		}
		if _, e = wordUint(amp[32:64], 1); e != nil {
			return domain.Pool{}, e
		}
		if new(big.Int).SetBytes(amp[64:96]).Uint64() != 1000 || new(big.Int).SetBytes(amp[64:96]).BitLen() > 64 {
			return domain.Pool{}, errors.New("BALANCER_AMP_PRECISION")
		}
		state.Amplification = domain.Amount(new(big.Int).SetBytes(amp[:32]).String())
		factors, e := call(p.ID, selectorScaling, -1)
		if e != nil {
			return domain.Pool{}, e
		}
		if len(factors) != 128 || new(big.Int).SetBytes(factors[:32]).Cmp(big.NewInt(32)) != 0 || new(big.Int).SetBytes(factors[32:64]).Cmp(big.NewInt(2)) != 0 {
			return domain.Pool{}, errors.New("BALANCER_SCALING_ARRAY")
		}
		state.Rate0 = domain.Amount(new(big.Int).SetBytes(factors[64:96]).String())
		state.Rate1 = domain.Amount(new(big.Int).SetBytes(factors[96:128]).String())
		tokens, e := call(v.ExpectedVault, selectorPoolTokens+v.PoolID[2:], -1)
		if e != nil {
			return domain.Pool{}, e
		}
		// Canonical ABI shape for exactly two assets: offsets96/192, two arrays.
		if len(tokens) != 288 || new(big.Int).SetBytes(tokens[:32]).Cmp(big.NewInt(96)) != 0 || new(big.Int).SetBytes(tokens[32:64]).Cmp(big.NewInt(192)) != 0 || new(big.Int).SetBytes(tokens[96:128]).Cmp(big.NewInt(2)) != 0 || new(big.Int).SetBytes(tokens[192:224]).Cmp(big.NewInt(2)) != 0 {
			return domain.Pool{}, errors.New("BALANCER_TWO_TOKEN_ARRAY")
		}
		for i, expected := range []string{p.Token0, p.Token1} {
			got, e := addr(tokens[128+i*32 : 160+i*32])
			if e != nil || got != expected || got == p.ID {
				return domain.Pool{}, errors.New("BALANCER_TOKEN_IDENTITY")
			}
		}
		r0, e = wordUint(tokens[224:256], 112)
		if e != nil {
			return domain.Pool{}, e
		}
		r1, e = wordUint(tokens[256:288], 112)
		if e != nil {
			return domain.Pool{}, e
		}
		fee, e = uintCall(p.ID, selectorSwapFee)
		if e != nil {
			return domain.Pool{}, e
		}
	default:
		return domain.Pool{}, errors.New("STABLE_PROTOCOL_UNSUPPORTED")
	}
	if !fee.IsUint64() || fee.Uint64() >= profile.FeeDenominator {
		return domain.Pool{}, errors.New("STABLE_FEE_RANGE")
	}
	pool := domain.Pool{Stable: state, ChainKey: a.Chain.Key, PoolID: p.ID, Protocol: p.Protocol, Token0: p.Token0, Token1: p.Token1, Reserve0: domain.Amount(r0.String()), Reserve1: domain.Amount(r1.String()), FeeNumerator: fee.Uint64(), FeeDenominator: profile.FeeDenominator, AdapterVersion: profile.AdapterVersion, TokenBehavior: "standard"}
	return pool, pool.Validate()
}

func isStandardClone(code []byte) bool {
	return len(code) == 45 && hex.EncodeToString(code[:10]) == "363d3d373d3d3d363d73" && hex.EncodeToString(code[30:]) == "5af43d82803e903d91602b57fd5bf3"
}
func (a *Adapter) verifyClone(ctx context.Context, code []byte, implementation, digest string, anchor map[string]any) error {
	clone := isStandardClone(code)
	if clone && implementation == "" {
		return errors.New("CLONE_IMPLEMENTATION_REQUIRED")
	}
	if implementation == "" {
		return nil
	}
	if domain.ValidateEVMIdentity(implementation, digest) != nil || !clone || "0x"+hex.EncodeToString(code[10:30]) != implementation {
		return errors.New("STABLE_PROXY_LAYOUT_UNSUPPORTED")
	}
	target, e := a.verifyCode(ctx, implementation, digest, anchor)
	if e != nil {
		return e
	}
	if isStandardClone(target) {
		return errors.New("NESTED_CLONE_UNSUPPORTED")
	}
	return nil
}
