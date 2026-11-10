package service

import (
	"bytes"
	"compress/gzip"
	"context"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"github.com/ethereum/go-ethereum/crypto"
	"io"
	"math/big"
	"strconv"
	"strings"
	"xone/chain-parser-go/internal/ingest"
	"xone/chain-parser-go/internal/rpc"

	"xone/chain-parser-go/internal/config"
)

var errV3Template = errors.New("CATALOG_V3_OFFICIAL_TEMPLATE_MISMATCH")

type v3ImmutableReference struct {
	Start  int `json:"start"`
	Length int `json:"length"`
}
type v3ImmutableDeclaration struct {
	ID         int                    `json:"id"`
	Source     string                 `json:"source"`
	Contract   string                 `json:"contract"`
	Name       string                 `json:"name"`
	Type       string                 `json:"type"`
	References []v3ImmutableReference `json:"references"`
}
type v3PoolImmutables struct {
	Pool, Factory, Token0, Token1 string
	Fee                           uint32
	TickSpacing                   int32
	MaxLiquidityPerTick           string
}

func officialV3Template() ([]byte, []v3ImmutableDeclaration, error) {
	compressed, err := base64.StdEncoding.DecodeString(officialV3RuntimeGZIPBase64)
	if err != nil {
		return nil, nil, err
	}
	reader, err := gzip.NewReader(bytes.NewReader(compressed))
	if err != nil {
		return nil, nil, err
	}
	code, err := io.ReadAll(io.LimitReader(reader, officialV3RuntimeBytes+1))
	closeErr := reader.Close()
	if err != nil {
		return nil, nil, err
	}
	if closeErr != nil {
		return nil, nil, closeErr
	}
	layout, err := validateOfficialV3Layout(code, officialV3ImmutableLayout)
	return code, layout, err
}
func validateOfficialV3Layout(code []byte, raw string) ([]v3ImmutableDeclaration, error) {
	digest := sha256.Sum256(code)
	layoutHash := sha256.Sum256([]byte(raw))
	if len(code) != officialV3RuntimeBytes || hex.EncodeToString(digest[:]) != config.OfficialV3RuntimeTemplateSHA256 || hex.EncodeToString(layoutHash[:]) != officialV3LayoutSHA256 {
		return nil, errV3Template
	}
	var declarations []v3ImmutableDeclaration
	if json.Unmarshal([]byte(raw), &declarations) != nil || len(declarations) != 7 {
		return nil, errV3Template
	}
	expected := map[string]string{"original": "address", "factory": "address", "token0": "address", "token1": "address", "fee": "uint24", "tickSpacing": "int24", "maxLiquidityPerTick": "uint128"}
	occupied := make([]bool, len(code))
	references := 0
	for _, d := range declarations {
		typ, ok := expected[d.Name]
		if !ok || d.Type != typ || d.ID <= 0 || len(d.References) == 0 {
			return nil, errV3Template
		}
		delete(expected, d.Name)
		if d.Name == "original" {
			if d.Contract != "NoDelegateCall" || d.Source != "contracts/NoDelegateCall.sol" {
				return nil, errV3Template
			}
		} else if d.Contract != "UniswapV3Pool" || d.Source != "contracts/UniswapV3Pool.sol" {
			return nil, errV3Template
		}
		for _, ref := range d.References {
			if ref.Length != 32 || ref.Start < 0 || ref.Start > len(code)-32 {
				return nil, errV3Template
			}
			references++
			for i := ref.Start; i < ref.Start+ref.Length; i++ {
				if occupied[i] || code[i] != 0 {
					return nil, errV3Template
				}
				occupied[i] = true
			}
		}
	}
	if len(expected) != 0 || references != 27 {
		return nil, errV3Template
	}
	return declarations, nil
}
func v3Address(value string) bool {
	if len(value) != 42 || !strings.HasPrefix(value, "0x") || value != strings.ToLower(value) || strings.Trim(value[2:], "0") == "" {
		return false
	}
	_, err := hex.DecodeString(value[2:])
	return err == nil
}
func v3MaxLiquidity(spacing int32) (string, error) {
	if spacing <= 0 || spacing >= 16384 {
		return "", errV3Template
	}
	min, max := (-887272/spacing)*spacing, (887272/spacing)*spacing
	ticks := uint64((max-min)/spacing) + 1
	limit := new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), 128), big.NewInt(1))
	return limit.Div(limit, new(big.Int).SetUint64(ticks)).String(), nil
}
func v3BindingsFromStrings(pool, factory, token0, token1, fee, spacing, maximum string) (v3PoolImmutables, error) {
	f, err := strconv.ParseUint(fee, 10, 24)
	if err != nil {
		return v3PoolImmutables{}, errV3Template
	}
	s, err := strconv.ParseInt(spacing, 10, 24)
	if err != nil {
		return v3PoolImmutables{}, errV3Template
	}
	b := v3PoolImmutables{Pool: pool, Factory: factory, Token0: token0, Token1: token1, Fee: uint32(f), TickSpacing: int32(s), MaxLiquidityPerTick: maximum}
	return b, b.validate()
}
func (b v3PoolImmutables) validate() error {
	if !v3Address(b.Pool) || !v3Address(b.Factory) || !v3Address(b.Token0) || !v3Address(b.Token1) || b.Token0 >= b.Token1 || b.Fee == 0 || b.Fee >= 1000000 {
		return errV3Template
	}
	maximum, err := v3MaxLiquidity(b.TickSpacing)
	if err != nil || maximum != b.MaxLiquidityPerTick {
		return errV3Template
	}
	return nil
}
func (b v3PoolImmutables) word(name string) ([]byte, error) {
	value := new(big.Int)
	switch name {
	case "original", "factory", "token0", "token1":
		address := map[string]string{"original": b.Pool, "factory": b.Factory, "token0": b.Token0, "token1": b.Token1}[name]
		raw, err := hex.DecodeString(address[2:])
		if err != nil {
			return nil, err
		}
		return append(make([]byte, 12), raw...), nil
	case "fee":
		value.SetUint64(uint64(b.Fee))
	case "tickSpacing":
		value.SetInt64(int64(b.TickSpacing))
	case "maxLiquidityPerTick":
		if _, ok := value.SetString(b.MaxLiquidityPerTick, 10); !ok || value.Sign() <= 0 || value.BitLen() > 128 {
			return nil, errV3Template
		}
	default:
		return nil, errV3Template
	}
	return value.FillBytes(make([]byte, 32)), nil
}

// verifyOfficialV3Runtime compares every deployed byte, including unchanged
// metadata. Only the immutable locations emitted by the pinned compiler/AST
// are filled, and each repeated occurrence receives the same typed value.
func verifyOfficialV3Runtime(profile config.V3PoolTemplateConfig, observed string, b v3PoolImmutables) (string, error) {
	if err := profile.Validate(); err != nil {
		return "", err
	}
	if err := b.validate(); err != nil {
		return "", err
	}
	if !strings.HasPrefix(observed, "0x") || len(observed) != 2+2*officialV3RuntimeBytes {
		return "", errV3Template
	}
	actual, err := hex.DecodeString(observed[2:])
	if err != nil {
		return "", errV3Template
	}
	expected, layout, err := officialV3Template()
	if err != nil {
		return "", err
	}
	for _, d := range layout {
		word, err := b.word(d.Name)
		if err != nil {
			return "", err
		}
		for _, ref := range d.References {
			if !bytes.Equal(actual[ref.Start:ref.Start+ref.Length], word) {
				return "", errV3Template
			}
			copy(expected[ref.Start:ref.Start+ref.Length], word)
		}
	}
	if !bytes.Equal(actual, expected) {
		return "", errV3Template
	}
	digest := sha256.Sum256(actual)
	return hex.EncodeToString(digest[:]), nil
}

// readOfficialV3Runtime binds every constructor identity to current hash-pinned
// getters before producing the per-instance code pin consumed by readV3.
func (r *CatalogRuntime) readOfficialV3Runtime(ctx context.Context, client *rpc.Client, s config.FactoryDeployment, c ingest.FactoryCandidate, a ingest.CatalogAnchor) (string, error) {
	if s.PoolTemplate == nil {
		return "", errV3Template
	}
	evidence, err := s.PoolTemplate.Deployment(r.options.Chain.Key, r.options.Chain.Genesis, c.Factory, s.FactoryCodeSHA256)
	if err != nil {
		return "", err
	}
	if evidence.DeploymentHeight != 0 && c.Height < evidence.DeploymentHeight {
		return "", errV3Template
	}
	anchor := map[string]any{"blockHash": a.Hash, "requireCanonical": true}
	call := func(signature string) (string, error) {
		var word string
		selector := crypto.Keccak256([]byte(signature))[:4]
		err := client.Call(ctx, "eth_call", []any{map[string]string{"to": c.Pool, "data": "0x" + hex.EncodeToString(selector)}, anchor}, &word)
		return word, err
	}
	addresses := map[string]string{}
	for _, signature := range []string{"factory()", "token0()", "token1()"} {
		word, err := call(signature)
		if err != nil {
			return "", err
		}
		address, err := catalogAddressWord(word)
		if err != nil {
			return "", err
		}
		addresses[signature] = address
	}
	if addresses["factory()"] != c.Factory || addresses["token0()"] != c.Token0 || addresses["token1()"] != c.Token1 {
		return "", errV3Template
	}
	uintWord := func(signature string, bits int) (*big.Int, error) {
		word, err := call(signature)
		if err != nil {
			return nil, err
		}
		raw, err := catalogWord(word)
		if err != nil {
			return nil, err
		}
		n := new(big.Int).SetBytes(raw)
		if n.BitLen() > bits {
			return nil, errV3Template
		}
		return n, nil
	}
	fee, err := uintWord("fee()", 24)
	if err != nil {
		return "", err
	}
	spacing, err := uintWord("tickSpacing()", 23)
	if err != nil {
		return "", err
	}
	maximum, err := uintWord("maxLiquidityPerTick()", 128)
	if err != nil {
		return "", err
	}
	if fee.Uint64() != uint64(c.Fee) || spacing.Int64() != int64(c.TickSpacing) {
		return "", errV3Template
	}
	bindings, err := v3BindingsFromStrings(c.Pool, c.Factory, c.Token0, c.Token1, fee.String(), spacing.String(), maximum.String())
	if err != nil {
		return "", err
	}
	var observed string
	if err = client.Call(ctx, "eth_getCode", []any{c.Pool, anchor}, &observed); err != nil {
		return "", err
	}
	digest, err := verifyOfficialV3Runtime(*s.PoolTemplate, observed, bindings)
	if err != nil {
		return "", err
	}
	if s.PoolCodeSHA256 != "" && s.PoolCodeSHA256 != digest {
		return "", errV3Template
	}
	return digest, nil
}

// V3TemplateBinding is immutable admission evidence, persisted with the entry.
// It describes byte/source identity, never provider independence or consensus.
type V3TemplateBinding struct {
	Profile                string
	RuntimeTemplateSHA256  string
	ActualRuntimeSHA256    string
	CompilerSHA256         string
	CompilerInputSHA256    string
	PoolSourceSHA256       string
	NoDelegateSourceSHA256 string
	FactorySourceSHA256    string
}

func officialV3Binding(profile config.V3PoolTemplateConfig, deployment config.V3TemplateDeployment, actual string) *V3TemplateBinding {
	return &V3TemplateBinding{Profile: profile.Profile, RuntimeTemplateSHA256: config.OfficialV3RuntimeTemplateSHA256, ActualRuntimeSHA256: actual, CompilerSHA256: config.OfficialV3CompilerSHA256, CompilerInputSHA256: officialV3CompilerInputSHA256, PoolSourceSHA256: officialV3PoolSourceSHA256, NoDelegateSourceSHA256: officialV3NoDelegateSourceSHA256, FactorySourceSHA256: deployment.SourceSHA256}
}

func (r *CatalogRuntime) officialTemplatePoolCounts() (verified, total int) {
	r.mu.Lock()
	defer r.mu.Unlock()
	for _, entry := range r.online.Entries {
		total++
		binding := entry.TemplateBinding
		if binding == nil || entry.Config.V3 == nil || entry.Pool.V3 == nil || entry.Admission.Candidate.Version != ingest.UniswapV3Factory {
			continue
		}
		source, err := r.deployment(entry.Admission.Candidate.Factory)
		if err != nil || source.PoolTemplate == nil {
			continue
		}
		deployment, err := source.PoolTemplate.Deployment(r.options.Chain.Key, r.options.Chain.Genesis, source.Address, source.FactoryCodeSHA256)
		if err != nil {
			continue
		}
		expected := officialV3Binding(*source.PoolTemplate, deployment, entry.Config.V3.ExpectedCodeSHA256)
		if *binding == *expected && binding.ActualRuntimeSHA256 == entry.Pool.V3.CodeSHA256 && entry.Config.ID == entry.Admission.Candidate.Pool {
			verified++
		}
	}
	return verified, total
}
