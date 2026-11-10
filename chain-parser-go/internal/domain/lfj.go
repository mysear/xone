package domain

import (
	"bytes"
	"encoding/hex"
	"encoding/json"
	"errors"
	"io"
	"math/big"
	"strings"
)

const LfjProtocol = "lfj-lb-v21"
const LfjAdapter = "lfj-lb/v2.1.0-fee-oracle-1"

type LfjBin struct {
	ID          uint32 `json:"id"`
	Reserve0    Amount `json:"reserve0"`
	Reserve1    Amount `json:"reserve1"`
	TotalShares Amount `json:"total_shares"`
}
type LfjOracle struct {
	Index uint32 `json:"index"`
	Data  Amount `json:"data"`
}
type LfjState struct {
	CodeSHA256               string      `json:"code_sha256"`
	Implementation           string      `json:"implementation"`
	ImplementationCodeSHA256 string      `json:"implementation_code_sha256"`
	Factory                  string      `json:"factory"`
	FactoryCodeSHA256        string      `json:"factory_code_sha256"`
	Token0CodeSHA256         string      `json:"token0_code_sha256"`
	Token1CodeSHA256         string      `json:"token1_code_sha256"`
	BlockHash                string      `json:"block_hash"`
	BlockNumber              uint64      `json:"block_number"`
	Timestamp                uint64      `json:"timestamp"`
	BinStep                  uint32      `json:"bin_step"`
	Parameters               Amount      `json:"parameters"`
	RawReserve0              Amount      `json:"raw_reserve0"`
	RawReserve1              Amount      `json:"raw_reserve1"`
	ProtocolFees0            Amount      `json:"protocol_fees0"`
	ProtocolFees1            Amount      `json:"protocol_fees1"`
	Bins                     []LfjBin    `json:"bins"`
	OracleSamples            []LfjOracle `json:"oracle_samples"`
	MaxBins                  uint32      `json:"max_bins"`
	MaxSteps                 uint32      `json:"max_steps"`
}

func LfjField(value Amount, offset, bits uint) uint64 {
	p, e := ParseAmount(string(value))
	if e != nil {
		return 0
	}
	n := new(big.Int).Rsh(p.Big(), offset)
	mask := new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), bits), big.NewInt(1))
	return n.And(n, mask).Uint64()
}
func (s *LfjState) Validate(p Pool) error {
	fail := func(code string) error { return errors.New(code) }
	if p.Protocol != LfjProtocol || p.AdapterVersion != LfjAdapter || p.TokenBehavior != "standard" || !strings.HasPrefix(p.ChainKey, "evm:") || ValidateChain(p.ChainKey) != nil || p.FeeNumerator != 0 || p.FeeDenominator != 1000000000000000000 || p.Reserve0 != "0" || p.Reserve1 != "0" {
		return fail("LFJ_PROFILE")
	}
	seen := map[string]bool{}
	for _, a := range []string{p.PoolID, p.Token0, p.Token1, s.Implementation, s.Factory} {
		if ValidateEVMAddress(a) != nil || a == SingletonZeroAddress || seen[a] {
			return fail("LFJ_IDENTITY_ALIAS")
		}
		seen[a] = true
	}
	return s.ValidateWire()
}
func (s *LfjState) ValidateWire() error {
	if s.Bins == nil || s.OracleSamples == nil {
		return errors.New("LFJ_REQUIRED_VECTORS")
	}

	fail := func(code string) error { return errors.New(code) }
	if s.Factory == SingletonZeroAddress || s.Implementation == SingletonZeroAddress || s.Factory == s.Implementation {
		return fail("LFJ_STATE_IDENTITY")
	}
	if ValidateEVMIdentity(s.Implementation, s.ImplementationCodeSHA256) != nil || ValidateEVMIdentity(s.Factory, s.FactoryCodeSHA256) != nil {
		return fail("LFJ_STATE_IDENTITY")
	}
	digest := func(value string) bool {
		b, e := hex.DecodeString(value)
		return e == nil && len(b) == 32 && value == strings.ToLower(value)
	}
	for _, d := range []string{s.CodeSHA256, s.ImplementationCodeSHA256, s.FactoryCodeSHA256, s.Token0CodeSHA256, s.Token1CodeSHA256} {
		if !digest(d) {
			return fail("LFJ_STATE_IDENTITY")
		}
	}
	if len(s.BlockHash) != 66 || !strings.HasPrefix(s.BlockHash, "0x") || !digest(s.BlockHash[2:]) || s.Timestamp >= 1<<40 || s.BinStep == 0 || s.BinStep > 65535 || s.MaxBins == 0 || s.MaxBins > 4096 || s.MaxSteps == 0 || s.MaxSteps > 8192 || len(s.Bins) > int(s.MaxBins) {
		return fail("LFJ_STATE_IDENTITY")
	}
	if _, e := ParseAmount(string(s.Parameters)); e != nil {
		return e
	}
	g := func(o, b uint) uint64 { return LfjField(s.Parameters, o, b) }
	if g(16, 12) > g(28, 12) || g(40, 14) > 10000 || g(78, 14) > 2500 || g(176, 40) > s.Timestamp {
		return fail("LFJ_FEE_PARAMETERS")
	}
	product := new(big.Int).SetUint64(g(92, 20) * uint64(s.BinStep))
	variable := new(big.Int).Mul(product, product)
	variable.Mul(variable, new(big.Int).SetUint64(g(54, 24)))
	variable.Add(variable, big.NewInt(99))
	variable.Div(variable, big.NewInt(100))
	base := new(big.Int).SetUint64(g(0, 16) * uint64(s.BinStep))
	base.Mul(base, big.NewInt(10000000000))
	base.Add(base, variable)
	if base.Cmp(big.NewInt(100000000000000000)) > 0 {
		return fail("LFJ_MAX_FEE")
	}
	width := func(value Amount, bits int) (*big.Int, error) {
		a, e := ParseAmount(string(value))
		if e != nil {
			return nil, e
		}
		if a.Big().BitLen() > bits {
			return nil, fail("LFJ_RESERVE_WIDTH")
		}
		return a.Big(), nil
	}
	sum0 := new(big.Int)
	sum1 := new(big.Int)
	var previous uint32
	for i, b := range s.Bins {
		if b.ID == 0 || b.ID >= 0xffffff || i > 0 && b.ID <= previous {
			return fail("LFJ_BIN_SET")
		}
		previous = b.ID
		r0, e := width(b.Reserve0, 128)
		if e != nil {
			return e
		}
		r1, e := width(b.Reserve1, 128)
		if e != nil {
			return e
		}
		shares, e := width(b.TotalShares, 256)
		if e != nil || shares.Sign() == 0 {
			return fail("LFJ_BIN_SET")
		}
		sum0.Add(sum0, r0)
		sum1.Add(sum1, r1)
	}
	r0, e := width(s.RawReserve0, 128)
	if e != nil {
		return e
	}
	r1, e := width(s.RawReserve1, 128)
	if e != nil {
		return e
	}
	f0, e := width(s.ProtocolFees0, 128)
	if e != nil {
		return e
	}
	f1, e := width(s.ProtocolFees1, 128)
	if e != nil {
		return e
	}
	sum0.Add(sum0, f0)
	sum1.Add(sum1, f1)
	if sum0.Cmp(r0) != 0 || sum1.Cmp(r1) != 0 {
		return fail("LFJ_RESERVE_ACCOUNTING")
	}
	if len(s.OracleSamples) > 2 {
		return fail("LFJ_ORACLE_SET")
	}
	samples := map[uint32]Amount{}
	for _, sample := range s.OracleSamples {
		if sample.Index == 0 || sample.Index > 65535 || samples[sample.Index] != "" {
			return fail("LFJ_ORACLE_SET")
		}
		if _, e := width(sample.Data, 256); e != nil {
			return e
		}
		samples[sample.Index] = sample.Data
	}
	oracleID := uint32(g(216, 16))
	if oracleID == 0 {
		if len(samples) != 0 {
			return fail("LFJ_DISABLED_ORACLE")
		}
	} else {
		sample, ok := samples[oracleID]
		if !ok {
			return fail("LFJ_ORACLE_MISSING")
		}
		length := LfjField(sample, 0, 16)
		created := LfjField(sample, 216, 40)
		lifetime := LfjField(sample, 208, 8)
		if length == 0 || uint64(oracleID) > length || created+lifetime > s.Timestamp {
			return fail("LFJ_ORACLE_CLOCK")
		}
		if s.Timestamp-created > 120 && samples[uint32(uint64(oracleID)%length+1)] == "" {
			return fail("LFJ_ORACLE_NEXT_MISSING")
		}
	}
	return nil
}
func (s LfjState) Clone() LfjState {
	s.Bins = append([]LfjBin{}, s.Bins...)
	s.OracleSamples = append([]LfjOracle{}, s.OracleSamples...)
	return s
}

// Keep required observed zero values distinct from absent/null JSON members.
func lfjStrictJSON(raw []byte, out any, required []string) error {
	d := json.NewDecoder(bytes.NewReader(raw))
	token, e := d.Token()
	if e != nil || token != json.Delim('{') {
		return errors.New("LFJ_JSON_OBJECT_REQUIRED")
	}
	keys := map[string]bool{}
	allowed := map[string]bool{}
	for _, key := range required {
		allowed[key] = true
	}
	for d.More() {
		token, e = d.Token()
		if e != nil {
			return e
		}
		key, ok := token.(string)
		if !ok || keys[key] || !allowed[key] {
			return errors.New("LFJ_JSON_DUPLICATE_OR_UNKNOWN")
		}
		keys[key] = true
		var value json.RawMessage
		if e = d.Decode(&value); e != nil {
			return e
		}
		if bytes.Equal(bytes.TrimSpace(value), []byte("null")) {
			return errors.New("LFJ_JSON_NULL_REQUIRED")
		}
	}
	if _, e = d.Token(); e != nil {
		return e
	}
	if _, e = d.Token(); e != io.EOF {
		return errors.New("LFJ_JSON_TRAILING")
	}
	if len(keys) != len(allowed) {
		return errors.New("LFJ_JSON_MISSING_REQUIRED")
	}
	decoder := json.NewDecoder(bytes.NewReader(raw))
	decoder.DisallowUnknownFields()
	return decoder.Decode(out)
}
func (s *LfjState) UnmarshalJSON(raw []byte) error {
	type alias LfjState
	var out alias
	if e := lfjStrictJSON(raw, &out, []string{"code_sha256", "implementation", "implementation_code_sha256", "factory", "factory_code_sha256", "token0_code_sha256", "token1_code_sha256", "block_hash", "block_number", "timestamp", "bin_step", "parameters", "raw_reserve0", "raw_reserve1", "protocol_fees0", "protocol_fees1", "bins", "oracle_samples", "max_bins", "max_steps"}); e != nil {
		return e
	}
	*s = LfjState(out)
	return nil
}
func (s *LfjBin) UnmarshalJSON(raw []byte) error {
	type alias LfjBin
	var out alias
	if e := lfjStrictJSON(raw, &out, []string{"id", "reserve0", "reserve1", "total_shares"}); e != nil {
		return e
	}
	*s = LfjBin(out)
	return nil
}
func (s *LfjOracle) UnmarshalJSON(raw []byte) error {
	type alias LfjOracle
	var out alias
	if e := lfjStrictJSON(raw, &out, []string{"index", "data"}); e != nil {
		return e
	}
	*s = LfjOracle(out)
	return nil
}
