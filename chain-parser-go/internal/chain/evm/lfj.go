package evm

import (
	"context"
	"encoding/hex"
	"errors"
	"fmt"
	"math/big"
	"strings"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
)

func (a *Adapter) readLfj(ctx context.Context, p config.PoolConfig, anchor map[string]any, block domain.BlockRef) (domain.Pool, error) {
	fail := func(code string) (domain.Pool, error) { return domain.Pool{}, errors.New(code) }
	v := p.Lfj
	if v == nil || p.Protocol != domain.LfjProtocol || v.TokenPolicy != "standard" || v.BinStep == 0 || v.BinStep > 65535 || v.MaxBins == 0 || v.MaxBins > 4096 || v.MaxSteps == 0 || v.MaxSteps > 8192 {
		return fail("LFJ_CONFIG")
	}
	for _, address := range []string{p.ID, p.Token0, p.Token1, v.ExpectedImplementation, v.ExpectedFactory} {
		if domain.ValidateEVMAddress(address) != nil || address == domain.SingletonZeroAddress {
			return fail("LFJ_CONFIG_IDENTITY")
		}
	}
	if len(anchor) != 2 || anchor["blockHash"] != block.Hash || anchor["requireCanonical"] != true {
		return fail("LFJ_FIXED_BLOCK_BINDING")
	}
	runtime, e := a.verifyCode(ctx, p.ID, v.ExpectedCodeSHA256, anchor)
	if e != nil {
		return domain.Pool{}, e
	}
	// Immutable-args clone emitted by the pinned v2.1.0 factory. Every byte,
	// implementation and immutable token/step argument is bound explicitly.
	expected := "363d3d373d3d3d3d61002c806035363936013d73" + v.ExpectedImplementation[2:] + "5af43d3d93803e603357fd5bf3" + p.Token0[2:] + p.Token1[2:] + fmt.Sprintf("%04x", v.BinStep) + "002c"
	if hex.EncodeToString(runtime) != expected {
		return fail("LFJ_IMMUTABLE_CLONE")
	}
	for _, identity := range []struct{ address, digest string }{{v.ExpectedImplementation, v.ExpectedImplementationCodeSHA256}, {v.ExpectedFactory, v.ExpectedFactoryCodeSHA256}, {p.Token0, v.ExpectedToken0CodeSHA256}, {p.Token1, v.ExpectedToken1CodeSHA256}} {
		if _, e = a.verifyCode(ctx, identity.address, identity.digest, anchor); e != nil {
			return domain.Pool{}, e
		}
	}
	call := func(to, signature, args string, words int) ([]byte, error) {
		return a.stableCall(ctx, to, singletonSelector(signature)+args, anchor, words)
	}
	scalar := func(to, signature, args string, bits int) (*big.Int, error) {
		raw, e := call(to, signature, args, 1)
		if e != nil {
			return nil, e
		}
		return wordUint(raw, bits)
	}
	word := func(value uint64) string { return fmt.Sprintf("%064x", value) }
	addressWord := func(address string) string { return strings.Repeat("0", 24) + address[2:] }
	readSlot := func(index uint64) (domain.Amount, error) {
		var text string
		if e := a.Client.Call(ctx, "eth_getStorageAt", []any{p.ID, fmt.Sprintf("0x%x", index), anchor}, &text); e != nil {
			return "", e
		}
		if len(text) != 66 || !strings.HasPrefix(text, "0x") {
			return "", errors.New("LFJ_STORAGE_WORD")
		}
		raw, e := hex.DecodeString(text[2:])
		if e != nil {
			return "", e
		}
		return domain.AmountFromBytes(raw)
	}
	for _, identity := range []struct{ signature, address string }{{"getFactory()", v.ExpectedFactory}, {"getTokenX()", p.Token0}, {"getTokenY()", p.Token1}} {
		value, e := scalar(p.ID, identity.signature, "", 160)
		if e != nil {
			return domain.Pool{}, e
		}
		if fmt.Sprintf("0x%040x", value) != identity.address {
			return fail("LFJ_GETTER_IDENTITY")
		}
	}
	step, e := scalar(p.ID, "getBinStep()", "", 16)
	if e != nil {
		return domain.Pool{}, e
	}
	if step.Uint64() != uint64(v.BinStep) {
		return fail("LFJ_BIN_STEP")
	}
	guard, e := readSlot(3)
	if e != nil {
		return domain.Pool{}, e
	}
	if guard != "1" {
		return fail("LFJ_REENTRANCY_STATE")
	}
	parameters, e := readSlot(4)
	if e != nil {
		return domain.Pool{}, e
	}
	reserves, e := readSlot(5)
	if e != nil {
		return domain.Pool{}, e
	}
	fees, e := readSlot(6)
	if e != nil {
		return domain.Pool{}, e
	}
	split := func(value domain.Amount) (domain.Amount, domain.Amount) {
		a, _ := domain.ParseAmount(string(value))
		n := a.Big()
		mask := new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), 128), big.NewInt(1))
		lo := new(big.Int).And(new(big.Int).Set(n), mask)
		hi := new(big.Int).Rsh(n, 128)
		return domain.Amount(lo.String()), domain.Amount(hi.String())
	}
	raw0, raw1 := split(reserves)
	fee0, fee1 := split(fees)
	state := &domain.LfjState{CodeSHA256: v.ExpectedCodeSHA256, Implementation: v.ExpectedImplementation, ImplementationCodeSHA256: v.ExpectedImplementationCodeSHA256, Factory: v.ExpectedFactory, FactoryCodeSHA256: v.ExpectedFactoryCodeSHA256, Token0CodeSHA256: v.ExpectedToken0CodeSHA256, Token1CodeSHA256: v.ExpectedToken1CodeSHA256, BlockHash: block.Hash, BlockNumber: block.Number, Timestamp: block.TimestampMS / 1000, BinStep: v.BinStep, Parameters: parameters, RawReserve0: raw0, RawReserve1: raw1, ProtocolFees0: fee0, ProtocolFees1: fee1, Bins: []domain.LfjBin{}, OracleSamples: []domain.LfjOracle{}, MaxBins: v.MaxBins, MaxSteps: v.MaxSteps}
	active, e := scalar(p.ID, "getActiveId()", "", 24)
	if e != nil {
		return domain.Pool{}, e
	}
	if active.Uint64() != domain.LfjField(parameters, 232, 24) {
		return fail("LFJ_ACTIVE_ID")
	}
	for _, spec := range []struct {
		signature string
		fields    [][2]uint
	}{{"getStaticFeeParameters()", [][2]uint{{0, 16}, {16, 12}, {28, 12}, {40, 14}, {54, 24}, {78, 14}, {92, 20}}}, {"getVariableFeeParameters()", [][2]uint{{112, 20}, {132, 20}, {152, 24}, {176, 40}}}} {
		data, e := call(p.ID, spec.signature, "", len(spec.fields))
		if e != nil {
			return domain.Pool{}, e
		}
		for i, f := range spec.fields {
			value, e := wordUint(data[i*32:(i+1)*32], int(f[1]))
			if e != nil {
				return domain.Pool{}, e
			}
			if value.Uint64() != domain.LfjField(parameters, f[0], f[1]) {
				return fail("LFJ_PARAMETER_GETTER")
			}
		}
	}
	for _, spec := range []struct {
		signature string
		expected  [2]domain.Amount
	}{{"getReserves()", [2]domain.Amount{raw0, raw1}}, {"getProtocolFees()", [2]domain.Amount{fee0, fee1}}} {
		data, e := call(p.ID, spec.signature, "", 2)
		if e != nil {
			return domain.Pool{}, e
		}
		for i, want := range spec.expected {
			value, e := wordUint(data[i*32:(i+1)*32], 128)
			if e != nil {
				return domain.Pool{}, e
			}
			if spec.signature == "getReserves()" {
				fee := fee0
				if i == 1 {
					fee = fee1
				}
				f, _ := domain.ParseAmount(string(fee))
				value.Add(value, f.Big())
			}
			if value.String() != string(want) {
				return fail("LFJ_RESERVE_GETTER")
			}
		}
	}
	for i, token := range []string{p.Token0, p.Token1} {
		actual, e := scalar(token, "balanceOf(address)", addressWord(p.ID), 256)
		if e != nil {
			return domain.Pool{}, e
		}
		want := raw0
		if i == 1 {
			want = raw1
		}
		if actual.String() != string(want) {
			return fail("LFJ_UNSYNCED_TOKEN_BALANCE")
		}
	}
	var cursor uint64
	for {
		id, e := scalar(p.ID, "getNextNonEmptyBin(bool,uint24)", word(0)+word(cursor), 24)
		if e != nil {
			return domain.Pool{}, e
		}
		next := id.Uint64()
		if next == 0 {
			break
		}
		if next <= cursor || next >= 0xffffff || len(state.Bins) >= int(v.MaxBins) {
			return fail("LFJ_TREE_BUDGET_OR_ORDER")
		}
		reverse, e := scalar(p.ID, "getNextNonEmptyBin(bool,uint24)", word(1)+word(next), 24)
		if e != nil {
			return domain.Pool{}, e
		}
		if reverse.Uint64() != cursor && !(cursor == 0 && reverse.Uint64() == 0xffffff) {
			return fail("LFJ_TREE_REVERSE_LINK")
		}
		data, e := call(p.ID, "getBin(uint24)", word(next), 2)
		if e != nil {
			return domain.Pool{}, e
		}
		x, e := wordUint(data[:32], 128)
		if e != nil {
			return domain.Pool{}, e
		}
		y, e := wordUint(data[32:], 128)
		if e != nil {
			return domain.Pool{}, e
		}
		supply, e := scalar(p.ID, "totalSupply(uint256)", word(next), 256)
		if e != nil {
			return domain.Pool{}, e
		}
		state.Bins = append(state.Bins, domain.LfjBin{ID: uint32(next), Reserve0: domain.Amount(x.String()), Reserve1: domain.Amount(y.String()), TotalShares: domain.Amount(supply.String())})
		cursor = next
	}
	oracle := domain.LfjField(parameters, 216, 16)
	if oracle != 0 {
		sample, e := readSlot(11 + oracle - 1)
		if e != nil {
			return domain.Pool{}, e
		}
		state.OracleSamples = append(state.OracleSamples, domain.LfjOracle{Index: uint32(oracle), Data: sample})
		length := domain.LfjField(sample, 0, 16)
		if length == 0 {
			return fail("LFJ_ORACLE_LENGTH")
		}
		next := oracle%length + 1
		if next != oracle {
			sample, e := readSlot(11 + next - 1)
			if e != nil {
				return domain.Pool{}, e
			}
			state.OracleSamples = append(state.OracleSamples, domain.LfjOracle{Index: uint32(next), Data: sample})
		}
	}
	pool := domain.Pool{Lfj: state, ChainKey: a.Chain.Key, PoolID: p.ID, Protocol: domain.LfjProtocol, Token0: p.Token0, Token1: p.Token1, Reserve0: "0", Reserve1: "0", FeeNumerator: 0, FeeDenominator: 1000000000000000000, AdapterVersion: domain.LfjAdapter, TokenBehavior: "standard"}
	return pool, pool.Validate()
}
