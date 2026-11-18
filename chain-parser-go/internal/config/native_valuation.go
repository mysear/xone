package config

import (
	"strings"

	"xone/chain-parser-go/internal/domain"
)

// Only public oracle authorities belong in configuration, never private keys.
type NativePricePolicy struct {
	SourceID      string            `json:"source_id"`
	ChainKey      string            `json:"chain_key"`
	GenesisHash   string            `json:"genesis_hash"`
	Token         string            `json:"token"`
	Signers       map[string]string `json:"signers"`
	Threshold     int               `json:"threshold"`
	MaxAgeMS      uint64            `json:"max_age_ms"`
	MaxValidityMS uint64            `json:"max_validity_ms"`
}
type NativeValuationConfig struct {
	ObservationsFile string              `json:"observations_file"`
	Policies         []NativePricePolicy `json:"policies"`
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
