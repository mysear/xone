package domain

import (
	"encoding/json"
	"strings"
	"testing"
)

func TestAmountContract(t *testing.T) {
	max := strings.Repeat("f", 64)
	a, err := AmountFromBytes(mustHex(t, max))
	if err != nil {
		t.Fatal(err)
	}
	if a.String() != "115792089237316195423570985008687907853269984665640564039457584007913129639935" {
		t.Fatal(a)
	}
	for _, raw := range []string{`123`, `"-1"`, `"1.0"`, `"01"`, `"115792089237316195423570985008687907853269984665640564039457584007913129639936"`} {
		var n Amount
		if json.Unmarshal([]byte(raw), &n) == nil {
			t.Fatalf("accepted %s", raw)
		}
	}
	if _, err := AmountFromBytes([]byte{1}); err == nil {
		t.Fatal("accepted short uint256")
	}
}
func TestCanonicalHash(t *testing.T) {
	a, _ := Hash(map[string]any{"z": "1", "a": "2"})
	b, _ := Hash(map[string]any{"a": "2", "z": "1"})
	if a != b {
		t.Fatal("unstable hash")
	}
}
