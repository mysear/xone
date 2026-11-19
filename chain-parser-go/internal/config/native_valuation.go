package config

import (
	"math/big"
	"strings"

	"xone/chain-parser-go/internal/domain"
)

// Only public oracle authorities belong in configuration, never private keys.
type NativePricePolicy struct {
	SourceID      string                  `json:"source_id"`
	ChainKey      string                  `json:"chain_key"`
	GenesisHash   string                  `json:"genesis_hash"`
	Token         string                  `json:"token"`
	Signers       map[string]string       `json:"signers"`
	Threshold     int                     `json:"threshold"`
	MaxAgeMS      uint64                  `json:"max_age_ms"`
	MaxValidityMS uint64                  `json:"max_validity_ms"`
	MarketFeed    *ChainlinkUSDPairPolicy `json:"market_feed,omitempty"`
}
type ChainlinkFeedPolicy struct {
	Proxy                string        `json:"proxy"`
	ProxyCodeSHA256      string        `json:"proxy_code_sha256"`
	Aggregator           string        `json:"aggregator"`
	AggregatorCodeSHA256 string        `json:"aggregator_code_sha256"`
	Description          string        `json:"description"`
	Decimals             uint8         `json:"decimals"`
	MinAnswer            domain.Amount `json:"min_answer"`
	MaxAnswer            domain.Amount `json:"max_answer"`
	MaxAgeMS             uint64        `json:"max_age_ms"`
}
type ChainlinkUSDPairPolicy struct {
	SchemaVersion  uint32              `json:"schema_version"`
	NativeDecimals uint8               `json:"native_decimals"`
	TokenDecimals  uint8               `json:"token_decimals"`
	NativeUSD      ChainlinkFeedPolicy `json:"native_usd"`
	TokenUSD       ChainlinkFeedPolicy `json:"token_usd"`
}

func (v ChainlinkUSDPairPolicy) valid(chain, genesis string) bool {
	if v.SchemaVersion != 1 || chain != "evm:1" || genesis != "0xd4e56740f876aef8c010b86a40d5f56745a118d0906a34e69aec8c0db1cb8fa3" || v.NativeDecimals != 18 || v.TokenDecimals > 36 || v.NativeUSD.Description != "ETH / USD" || v.NativeUSD.Proxy == v.TokenUSD.Proxy && v.NativeUSD != v.TokenUSD {
		return false
	}
	for _, f := range []ChainlinkFeedPolicy{v.NativeUSD, v.TokenUSD} {
		lo, lok := new(big.Int).SetString(string(f.MinAnswer), 10)
		hi, hik := new(big.Int).SetString(string(f.MaxAnswer), 10)
		if domain.ValidateEVMAddress(f.Proxy) != nil || domain.ValidateEVMAddress(f.Aggregator) != nil || f.Proxy == "0x"+strings.Repeat("0", 40) || f.Aggregator == "0x"+strings.Repeat("0", 40) || f.Decimals > 18 || !strings.HasSuffix(f.Description, " / USD") || len(f.Description) < 7 || len(f.Description) > 128 || f.MaxAgeMS == 0 || f.MaxAgeMS > 86400000 || !lok || !hik || lo.Sign() <= 0 || lo.Cmp(hi) > 0 || hi.BitLen() > 255 || string(f.MinAnswer) != lo.String() || string(f.MaxAnswer) != hi.String() {
			return false
		}
		for _, b := range []byte(f.Description) {
			if b < 32 || b > 126 {
				return false
			}
		}
		for _, h := range []string{f.ProxyCodeSHA256, f.AggregatorCodeSHA256} {
			if !canonicalPriceHex(h, 32) || h == strings.Repeat("0", 64) {
				return false
			}
		}
	}
	return true
}

type NativeValuationConfig struct {
	ObservationsFile   string              `json:"observations_file"`
	PublicationJournal *string             `json:"publication_journal,omitempty"`
	Policies           []NativePricePolicy `json:"policies"`
}

func canonicalPriceHex(s string, n int) bool {
	if len(s) != n*2 {
		return false
	}
	for _, b := range []byte(s) {
		if !(b >= '0' && b <= '9' || b >= 'a' && b <= 'f') {
			return false
		}
	}
	return true
}
func priceName(s string) bool {
	if len(s) == 0 || len(s) > 128 {
		return false
	}
	for _, b := range []byte(s) {
		if !(b >= '0' && b <= '9' || b >= 'a' && b <= 'z' || b >= 'A' && b <= 'Z' || strings.ContainsRune("-_/.:", rune(b))) {
			return false
		}
	}
	return true
}
func (v NativeValuationConfig) valid(chains []Chain) bool {
	if path := v.PublicationJournal; path != nil && (*path == "" || len(*path) > 4096 || strings.ContainsRune(*path, '\x00') || *path == v.ObservationsFile) {
		return false
	}
	if v.ObservationsFile == "" || len(v.ObservationsFile) > 4096 || strings.ContainsRune(v.ObservationsFile, '\x00') || len(v.Policies) == 0 || len(v.Policies) > 128 {
		return false
	}
	pairs := map[string]bool{}
	for _, p := range v.Policies {
		pair := p.ChainKey + "/" + p.GenesisHash + "/" + p.Token
		if pairs[pair] || domain.ValidateChain(p.ChainKey) != nil || !strings.HasPrefix(p.ChainKey, "evm:") || len(p.GenesisHash) != 66 || !strings.HasPrefix(p.GenesisHash, "0x") || !canonicalPriceHex(p.GenesisHash[2:], 32) || p.GenesisHash == "0x"+strings.Repeat("0", 64) || domain.ValidateEVMAddress(p.Token) != nil || p.Token == "0x"+strings.Repeat("0", 40) || !priceName(p.SourceID) || len(p.Signers) < 2 || len(p.Signers) > 16 || p.Threshold < 2 || p.Threshold > len(p.Signers) || p.MaxAgeMS == 0 || p.MaxAgeMS > 3600000 || p.MaxValidityMS == 0 || p.MaxValidityMS > 3600000 {
			return false
		}
		found := false
		for _, chain := range chains {
			found = found || chain.Key == p.ChainKey && chain.Genesis == p.GenesisHash
		}
		if !found {
			return false
		}
		if p.MarketFeed != nil && !p.MarketFeed.valid(p.ChainKey, p.GenesisHash) {
			return false
		}
		keys := map[string]bool{}
		for id, key := range p.Signers {
			if !priceName(id) || !canonicalPriceHex(key, 32) || key == strings.Repeat("0", 64) || keys[key] {
				return false
			}
			keys[key] = true
		}
		pairs[pair] = true
	}
	return true
}
