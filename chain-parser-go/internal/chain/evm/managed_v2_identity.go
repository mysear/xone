package evm

import (
	"bytes"
	"compress/gzip"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"math/big"
)

type managedV2Template struct {
	Methods    map[string]string `json:"methods"`
	Hex        string            `json:"hex"`
	SHA256     string            `json:"sha256"`
	Library    bool              `json:"library"`
	Links      map[string][]int  `json:"links"`
	Immutables map[string][]int  `json:"immutables"`
}

var managedV2Templates = func() map[string]managedV2Template {
	raw, e := base64.StdEncoding.DecodeString(managedV2TemplateBank)
	if e != nil {
		panic(e)
	}
	z, e := gzip.NewReader(bytes.NewReader(raw))
	if e != nil {
		panic(e)
	}
	raw, e = io.ReadAll(z)
	if e != nil {
		panic(e)
	}
	if e = z.Close(); e != nil {
		panic(e)
	}
	var bank map[string]managedV2Template
	if e = json.Unmarshal(raw, &bank); e != nil {
		panic(e)
	}
	return bank
}()
var managedV2Roles = map[string]string{
	"circuit-breaker-lib": "CircuitBreakerLib", "managed-pool-add-remove-token-lib": "ManagedPoolAddRemoveTokenLib", "managed-pool-amm-lib": "ManagedPoolAmmLib", "external-weighted-math": "ExternalWeightedMath", "recovery-mode-helper": "RecoveryModeHelper",
}

func managedV2Word(v *big.Int) []byte { return v.FillBytes(make([]byte, 32)) }
func managedV2AddressWord(s string) []byte {
	b, e := hex.DecodeString(safeManagedHex(s))
	if e != nil || len(b) != 20 {
		return nil
	}
	return append(make([]byte, 12), b...)
}
func safeManagedHex(s string) string {
	if len(s) >= 2 && s[:2] == "0x" {
		return s[2:]
	}
	return ""
}

// Rebuild every runtime byte from the pinned original template. There is no
// wildcard mask: every compiler relocation requires an exact semantic binding.
func managedV2Match(name, address string, code []byte, bindings map[string][]byte, links map[string]string) error {
	t, ok := managedV2Templates[name]
	if !ok {
		return errors.New("MANAGED_V2_TEMPLATE_UNKNOWN")
	}
	expected, e := hex.DecodeString(t.Hex)
	if e != nil || len(code) != len(expected) || len(code) > 24576 {
		return errors.New("MANAGED_V2_RUNTIME_SIZE")
	}
	if t.Library {
		self := managedV2AddressWord(address)
		if len(self) != 32 || expected[0] != 0x73 {
			return errors.New("MANAGED_V2_LIBRARY_SELF")
		}
		copy(expected[1:21], self[12:])
	}
	for lib, offsets := range t.Links {
		word := managedV2AddressWord(links[lib])
		if len(word) != 32 {
			return fmt.Errorf("MANAGED_V2_LINK_MISSING: %s", lib)
		}
		for _, at := range offsets {
			copy(expected[at:at+20], word[12:])
		}
	}
	for key, offsets := range t.Immutables {
		value := bindings[key]
		if len(value) != 32 {
			return fmt.Errorf("MANAGED_V2_IMMUTABLE_UNBOUND: %s/%s", name, key)
		}
		for _, at := range offsets {
			copy(expected[at:at+32], value)
		}
	}
	if !bytes.Equal(code, expected) {
		return fmt.Errorf("MANAGED_V2_RUNTIME_TEMPLATE: %s", name)
	}
	return nil
}

// Extract a compiler-declared immutable only to bind it to a getter or check
// its documented structure. Repeated occurrences must already agree.
func managedV2Immutable(name, key string, code []byte) ([]byte, error) {
	t, ok := managedV2Templates[name]
	if !ok || len(code) != len(t.Hex)/2 {
		return nil, errors.New("MANAGED_V2_RUNTIME_SIZE")
	}
	offsets := t.Immutables[key]
	if len(offsets) == 0 {
		return nil, errors.New("MANAGED_V2_IMMUTABLE_UNKNOWN")
	}
	value := code[offsets[0] : offsets[0]+32]
	for _, at := range offsets[1:] {
		if !bytes.Equal(value, code[at:at+32]) {
			return nil, errors.New("MANAGED_V2_IMMUTABLE_REPEAT")
		}
	}
	return bytes.Clone(value), nil
}
