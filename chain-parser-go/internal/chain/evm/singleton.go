package evm

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"math/big"
	"strings"

	"golang.org/x/crypto/sha3"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
)

// Storage/getter layouts are pinned to Uniswap/v4-core
// bea7f6d1c11cd5dc4f0d5151ffa37a1c239feee6 (StateLibrary.sol, Pool.sol), and
// pancakeswap/infinity-core 3c5ef1be1f71442d8bc60cc6fd11f9c677b714df
// (CLPoolManager.sol, Tick.sol). StateLibrary functions are internal: V4 reads
// use the manager's actual extsload(bytes32) entrypoint, never a library ABI.
func singletonKeccak(data []byte) []byte {
	h := sha3.NewLegacyKeccak256()
	_, _ = h.Write(data)
	return h.Sum(nil)
}
func singletonSelector(signature string) string {
	return "0x" + hex.EncodeToString(singletonKeccak([]byte(signature))[:4])
}
func singletonHex(s string, size int) bool {
	if len(s) != 2+size*2 || !strings.HasPrefix(s, "0x") || s != strings.ToLower(s) {
		return false
	}
	_, err := hex.DecodeString(s[2:])
	return err == nil
}
func singletonSlotOffset(slot []byte, offset int64) []byte {
	n := new(big.Int).Add(new(big.Int).SetBytes(slot), big.NewInt(offset))
	n.Mod(n, new(big.Int).Lsh(big.NewInt(1), 256))
	return n.FillBytes(make([]byte, 32))
}
func singletonMappingSlot(key, slot []byte) []byte {
	encoded := make([]byte, 0, 64)
	encoded = append(encoded, key...)
	encoded = append(encoded, slot...)
	return singletonKeccak(encoded)
}

func (a *Adapter) readSingletonCL(ctx context.Context, p config.PoolConfig, anchor map[string]any) (domain.Pool, error) {
	return a.readSingletonCLSnapshot(ctx, p, anchor, nil)
}

// SingletonAdvancedReadConfig is an explicit opt-in, independent of legacy config admission.
type SingletonAdvancedReadConfig struct {
	State               domain.SingletonAdvancedState
	TickWordRadius      uint32
	MaxInitializedTicks uint32
	MaxSteps            uint32
}

func (a *Adapter) ReadSingletonAdvanced(ctx context.Context, request SingletonAdvancedReadConfig) (domain.SingletonAdvancedState, error) {
	state := request.State
	if err := state.ValidateIdentity(); err != nil {
		return domain.SingletonAdvancedState{}, err
	}
	s := state.Snapshot
	p := config.PoolConfig{ID: s.Manager + "/" + s.PoolID, Protocol: state.Protocol, Token0: state.Currency0, Token1: state.Currency1,
		FeeNumerator: uint64(state.KeyFee), FeeDenominator: 1000000,
		SingletonCL: &config.SingletonCLPoolConfig{Manager: s.Manager, ExpectedManagerCodeSHA256: s.ManagerCodeSHA256,
			PoolID: s.PoolID, LPFee: state.KeyFee, TickSpacing: s.Core.TickSpacing, Parameters: s.Parameters,
			TickWordRadius: request.TickWordRadius, MaxInitializedTicks: request.MaxInitializedTicks, MaxSteps: request.MaxSteps}}
	anchor := map[string]any{"blockHash": state.SnapshotBlockHash, "requireCanonical": true}
	pool, err := a.readSingletonCLSnapshot(ctx, p, anchor, &state)
	if err != nil {
		return domain.SingletonAdvancedState{}, err
	}
	state.Snapshot = *pool.SingletonCL
	if err := state.Validate(); err != nil {
		return domain.SingletonAdvancedState{}, err
	}
	return state, nil
}
func (a *Adapter) readSingletonCLSnapshot(ctx context.Context, p config.PoolConfig, anchor map[string]any, advanced *domain.SingletonAdvancedState) (domain.Pool, error) {
	fail := func(code string) (domain.Pool, error) { return domain.Pool{}, errors.New(code) }
	profile, known := domain.Protocol(p.Protocol)
	if !known || profile.Family != "singleton" || (p.Protocol != "uniswap-v4" && p.Protocol != "pancake-infinity-cl") {
		return fail("SINGLETON_PROTOCOL_UNSUPPORTED")
	}
	v := p.SingletonCL
	if v == nil || v.TickWordRadius > 16 || v.MaxInitializedTicks == 0 || v.MaxInitializedTicks > 4096 || v.MaxSteps == 0 || v.MaxSteps > 8192 {
		return fail("SINGLETON_CONFIG_REQUIRED")
	}
	if domain.ValidateEVMIdentity(v.Manager, v.ExpectedManagerCodeSHA256) != nil || v.Manager == domain.SingletonZeroAddress {
		return fail("SINGLETON_MANAGER_IDENTITY")
	}
	var rawID string
	var err error
	hooks := domain.SingletonZeroAddress
	if advanced == nil {
		rawID, err = domain.SingletonPoolID(p.Protocol, v.Manager, p.Token0, p.Token1, v.LPFee, v.TickSpacing, v.Parameters)
	} else {
		rawID, err = advanced.KeyID()
		hooks = advanced.Snapshot.Hooks
	}
	if err != nil {
		return domain.Pool{}, fmt.Errorf("SINGLETON_KEY: %w", err)
	}
	if rawID != v.PoolID || p.ID != v.Manager+"/"+rawID {
		return fail("SINGLETON_POOL_IDENTITY")
	}
	if p.FeeNumerator != uint64(v.LPFee) || p.FeeDenominator != 1000000 {
		return fail("SINGLETON_FEE_CONFIG")
	}
	blockHash, ok := anchor["blockHash"].(string)
	if !ok || !singletonHex(blockHash, 32) || anchor["requireCanonical"] != true {
		return fail("SINGLETON_BLOCK_HASH_REQUIRED")
	}
	if _, exists := anchor["blockNumber"]; exists {
		return fail("SINGLETON_BLOCK_HASH_REQUIRED")
	}

	var code string
	if err = a.Client.Call(ctx, "eth_getCode", []any{v.Manager, anchor}, &code); err != nil {
		return domain.Pool{}, fmt.Errorf("SINGLETON_MANAGER_CODE: %w", err)
	}
	if !strings.HasPrefix(code, "0x") {
		return fail("SINGLETON_CODE_ENCODING")
	}
	runtime, err := hex.DecodeString(code[2:])
	if err != nil || len(runtime) == 0 {
		return fail("SINGLETON_CODE_ENCODING")
	}
	digest := sha256.Sum256(runtime)
	if hex.EncodeToString(digest[:]) != v.ExpectedManagerCodeSHA256 {
		return fail("SINGLETON_CODE_HASH_MISMATCH")
	}
	if isStandardClone(runtime) {
		return fail("SINGLETON_MANAGER_PROXY_UNSUPPORTED")
	}

	call := func(data string, words int) ([]byte, error) {
		var result string
		if err := a.Client.Call(ctx, "eth_call", []any{map[string]string{"to": v.Manager, "data": data}, anchor}, &result); err != nil {
			return nil, fmt.Errorf("SINGLETON_RPC_CALL: %w", err)
		}
		if !strings.HasPrefix(result, "0x") {
			return nil, errors.New("SINGLETON_ABI_ENCODING")
		}
		raw, err := hex.DecodeString(result[2:])
		if err != nil || len(raw) != words*32 {
			return nil, errors.New("SINGLETON_ABI_SIZE")
		}
		return raw, nil
	}
	rawBytes, _ := hex.DecodeString(rawID[2:])
	base := singletonMappingSlot(rawBytes, singletonSlotOffset(make([]byte, 32), 6))
	extsload := func(slot []byte) ([]byte, error) {
		return call(singletonSelector("extsload(bytes32)")+hex.EncodeToString(slot), 1)
	}
	var price, tick, protocolFee, lpFee *big.Int
	var liquidityRaw []byte
	if p.Protocol == "uniswap-v4" {
		slot, e := extsload(base)
		if e != nil {
			return domain.Pool{}, e
		}
		// The high 24 reserved bits of Slot0 must remain zero for this version.
		if slot[0] != 0 || slot[1] != 0 || slot[2] != 0 {
			return fail("SINGLETON_SLOT0_RESERVED")
		}
		price = new(big.Int).SetBytes(slot[12:])
		tick = new(big.Int).SetBytes(slot[9:12])
		if slot[9]&128 != 0 {
			tick.Sub(tick, big.NewInt(1<<24))
		}
		protocolFee = new(big.Int).SetBytes(slot[6:9])
		lpFee = new(big.Int).SetBytes(slot[3:6])
		liquidityRaw, err = extsload(singletonSlotOffset(base, 3))
	} else {
		paused, e := call(singletonSelector("paused()"), 1)
		if e != nil {
			return domain.Pool{}, e
		}
		flag, e := wordUint(paused, 1)
		if e != nil || flag.Sign() != 0 {
			return fail("SINGLETON_MANAGER_PAUSED")
		}
		key, e := call(singletonSelector("poolIdToPoolKey(bytes32)")+rawID[2:], 6)
		if e != nil {
			return domain.Pool{}, e
		}
		if advanced != nil && hooks != domain.SingletonZeroAddress {
			var result string
			if err := a.Client.Call(ctx, "eth_call", []any{map[string]string{"to": hooks, "data": singletonSelector("getHooksRegistrationBitmap()")}, anchor}, &result); err != nil {
				return domain.Pool{}, fmt.Errorf("SINGLETON_HOOK_BITMAP: %w", err)
			}
			if !singletonHex(result, 32) {
				return fail("SINGLETON_HOOK_BITMAP")
			}
			b, _ := hex.DecodeString(result[2:])
			bitmap, e := wordUint(b, 16)
			params, _ := hex.DecodeString(v.Parameters[2:])
			if e != nil || bitmap.Uint64() != new(big.Int).SetBytes(params[30:]).Uint64() {
				return fail("SINGLETON_HOOK_BITMAP")
			}
		}
		// Public PoolKey getter order: currency0,currency1,hooks,poolManager,fee,parameters.
		for i, expected := range []string{p.Token0, p.Token1, hooks, v.Manager} {
			word := key[i*32 : (i+1)*32]
			if new(big.Int).SetBytes(word[:12]).Sign() != 0 || "0x"+hex.EncodeToString(word[12:]) != expected {
				return fail("SINGLETON_RETURNED_KEY_IDENTITY")
			}
		}
		fee, e := wordUint(key[128:160], 24)
		if e != nil || fee.Uint64() != uint64(v.LPFee) || "0x"+hex.EncodeToString(key[160:192]) != v.Parameters {
			return fail("SINGLETON_RETURNED_KEY_PARAMETERS")
		}
		slot, e := call(singletonSelector("getSlot0(bytes32)")+rawID[2:], 4)
		if e != nil {
			return domain.Pool{}, e
		}
		price, e = wordUint(slot[:32], 160)
		if e != nil {
			return fail("SINGLETON_PRICE_WIDTH")
		}
		tick, e = wordInt(slot[32:64], 24)
		if e != nil {
			return fail("SINGLETON_TICK_WIDTH")
		}
		protocolFee, e = wordUint(slot[64:96], 24)
		if e != nil {
			return fail("SINGLETON_PROTOCOL_FEE_WIDTH")
		}
		lpFee, e = wordUint(slot[96:128], 24)
		if e != nil {
			return fail("SINGLETON_LP_FEE_WIDTH")
		}
		liquidityRaw, err = call(singletonSelector("getLiquidity(bytes32)")+rawID[2:], 1)
	}
	if err != nil {
		return domain.Pool{}, err
	}
	liquidity, err := wordUint(liquidityRaw, 128)
	if err != nil {
		return fail("SINGLETON_LIQUIDITY_WIDTH")
	}
	if tick.Int64() < int64(domain.MinV3Tick) || tick.Int64() >= int64(domain.MaxV3Tick) {
		return fail("SINGLETON_TICK_RANGE")
	}
	if (advanced == nil && (lpFee.Uint64() != uint64(v.LPFee) || lpFee.Uint64() >= 1000000)) ||
		(advanced != nil && (lpFee.Uint64() > 1000000 || v.LPFee != domain.SingletonDynamicFeeFlag && lpFee.Uint64() != uint64(v.LPFee))) {
		return fail("SINGLETON_LP_FEE_MISMATCH")
	}
	fee0, fee1 := uint32(protocolFee.Uint64())&0xfff, uint32(protocolFee.Uint64())>>12
	maxProtocolFee := uint32(1000)
	if p.Protocol == "pancake-infinity-cl" {
		maxProtocolFee = 4000
	}
	if fee0 > maxProtocolFee || fee1 > maxProtocolFee {
		return fail("SINGLETON_PROTOCOL_FEE_RANGE")
	}
	core := domain.ConcentratedState{SqrtPriceX96: domain.Amount(price.String()), Tick: int32(tick.Int64()), Liquidity: domain.Amount(liquidity.String()), TickSpacing: v.TickSpacing, MaxSteps: v.MaxSteps, TickWords: []domain.V3BitmapWord{}, Ticks: []domain.V3Tick{}}
	// Validate price/tick before deriving the bounded bitmap window.
	low, _ := domain.SqrtRatioAtTick(core.Tick)
	high, _ := domain.SqrtRatioAtTick(core.Tick + 1)
	min, _ := domain.SqrtRatioAtTick(domain.MinV3Tick)
	max, _ := domain.SqrtRatioAtTick(domain.MaxV3Tick)
	if price.Sign() == 0 || price.Cmp(low) < 0 || price.Cmp(high) > 0 || price.Cmp(min) < 0 || price.Cmp(max) >= 0 {
		return fail("SINGLETON_PRICE_TICK_MISMATCH")
	}
	center := floorDiv(floorDiv(core.Tick, v.TickSpacing), 256)
	lower, upper := center-int32(v.TickWordRadius), center+int32(v.TickWordRadius)
	minWord := floorDiv(floorDiv(domain.MinV3Tick, v.TickSpacing), 256)
	maxWord := floorDiv(floorDiv(domain.MaxV3Tick, v.TickSpacing), 256)
	if lower < minWord {
		lower = minWord
	}
	if upper > maxWord {
		upper = maxWord
	}
	initialized := []int32{}
	for pos := lower; pos <= upper; pos++ {
		var b []byte
		if p.Protocol == "uniswap-v4" {
			key, _ := hex.DecodeString(signedArg(pos))
			b, err = extsload(singletonMappingSlot(key, singletonSlotOffset(base, 5)))
		} else {
			b, err = call(singletonSelector("getPoolBitmapInfo(bytes32,int16)")+rawID[2:]+signedArg(pos), 1)
		}
		if err != nil {
			return domain.Pool{}, err
		}
		bitmap := new(big.Int).SetBytes(b)
		core.TickWords = append(core.TickWords, domain.V3BitmapWord{WordPos: pos, Bitmap: domain.Amount(bitmap.String())})
		for bit := 0; bit < 256; bit++ {
			if bitmap.Bit(bit) == 0 {
				continue
			}
			index := (int64(pos)*256 + int64(bit)) * int64(v.TickSpacing)
			if index < int64(domain.MinV3Tick) || index > int64(domain.MaxV3Tick) {
				return fail("SINGLETON_BITMAP_TICK_RANGE")
			}
			initialized = append(initialized, int32(index))
			if len(initialized) > int(v.MaxInitializedTicks) {
				return fail("SINGLETON_INITIALIZED_TICK_BUDGET")
			}
		}
	}
	// Both pinned cores floor MIN_TICK/tickSpacing (unlike V3's truncation).
	numTicks := domain.MaxV3Tick/v.TickSpacing - floorDiv(domain.MinV3Tick, v.TickSpacing) + 1
	maxGross := new(big.Int).Quo(new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), 128), big.NewInt(1)), big.NewInt(int64(numTicks)))
	for _, index := range initialized {
		var gross, net *big.Int
		if p.Protocol == "uniswap-v4" {
			key, _ := hex.DecodeString(signedArg(index))
			b, e := extsload(singletonMappingSlot(key, singletonSlotOffset(base, 4)))
			if e != nil {
				return domain.Pool{}, e
			}
			gross = new(big.Int).SetBytes(b[16:])
			net = new(big.Int).SetBytes(b[:16])
			if b[0]&128 != 0 {
				net.Sub(net, new(big.Int).Lsh(big.NewInt(1), 128))
			}
		} else {
			b, e := call(singletonSelector("getPoolTickInfo(bytes32,int24)")+rawID[2:]+signedArg(index), 4)
			if e != nil {
				return domain.Pool{}, e
			}
			gross, e = wordUint(b[:32], 128)
			if e != nil {
				return fail("SINGLETON_TICK_GROSS_WIDTH")
			}
			net, e = wordInt(b[32:64], 128)
			if e != nil {
				return fail("SINGLETON_TICK_NET_WIDTH")
			}
			// Fee-growth words are uint256 and carry no additional width restrictions.
		}
		if gross.Sign() == 0 || gross.Cmp(maxGross) > 0 {
			return fail("SINGLETON_TICK_GROSS")
		}
		if new(big.Int).Abs(net).Cmp(gross) > 0 {
			return fail("SINGLETON_TICK_NET")
		}
		core.Ticks = append(core.Ticks, domain.V3Tick{Tick: index, LiquidityGross: domain.Amount(gross.String()), LiquidityNet: net.String()})
	}
	state := &domain.SingletonCLState{Manager: v.Manager, ManagerCodeSHA256: v.ExpectedManagerCodeSHA256, PoolID: rawID, Hooks: hooks, Parameters: v.Parameters, LPFee: uint32(lpFee.Uint64()), ProtocolFee0: fee0, ProtocolFee1: fee1, Core: core}
	pool := domain.Pool{ChainKey: a.Chain.Key, PoolID: p.ID, Protocol: p.Protocol, Token0: p.Token0, Token1: p.Token1, Reserve0: "0", Reserve1: "0", FeeNumerator: uint64(v.LPFee), FeeDenominator: 1000000, AdapterVersion: profile.AdapterVersion, TokenBehavior: "standard", SingletonCL: state}
	if advanced == nil {
		if err := pool.Validate(); err != nil {
			return domain.Pool{}, fmt.Errorf("SINGLETON_STATE: %w", err)
		}
	}
	return pool, nil
}
