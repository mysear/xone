package provenance

import (
	"encoding/json"
	"math/big"
	"testing"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/params"
	"github.com/ethereum/go-ethereum/trie"
)

func TestMainnetWithdrawalForkAndRequiredFields(t *testing.T) {
	boundary := *params.MainnetChainConfig.ShanghaiTime
	empty := types.EmptyWithdrawalsHash
	zero := types.Withdrawals{&types.Withdrawal{}}
	single := types.DeriveSha(zero, trie.NewStackTrie(nil))
	encoded, err := json.Marshal(zero)
	if err != nil {
		t.Fatal(err)
	}
	for _, tc := range []struct {
		name string
		time uint64
		root *common.Hash
		list string
		want string
	}{
		{"before", boundary - 1, nil, "", ""}, {"before-null", boundary - 1, nil, "null", ""},
		{"early-body", boundary - 1, nil, "[]", "WITHDRAWALS_BEFORE_SHANGHAI"}, {"early-root", boundary - 1, &empty, "", "WITHDRAWALS_BEFORE_SHANGHAI"},
		{"empty", boundary, &empty, "[]", ""}, {"missing-root", boundary, nil, "[]", "WITHDRAWALS_MISSING"},
		{"missing-list", boundary, &empty, "", "WITHDRAWALS_MISSING"}, {"null-list", boundary, &empty, "null", "WITHDRAWALS_MISSING"},
		{"zero-fields", boundary, &single, string(encoded), ""}, {"omitted-zero-fields", boundary, &single, "[{}]", "WITHDRAWALS_ENCODING"},
		{"different-list", boundary, &single, "[]", "WITHDRAWALS_ROOT"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			h := types.Header{Number: big.NewInt(17034870), Time: tc.time, WithdrawalsHash: tc.root}
			err := VerifyMainnetWithdrawals(&h, json.RawMessage(tc.list))
			if tc.want == "" && err != nil || tc.want != "" && (err == nil || err.Error() != tc.want) {
				t.Fatalf("got %v, want %q", err, tc.want)
			}
		})
	}
	var list types.Withdrawals
	for range 17 {
		list = append(list, &types.Withdrawal{})
	}
	encoded, err = json.Marshal(list)
	if err != nil {
		t.Fatal(err)
	}
	h := types.Header{Number: big.NewInt(17034870), Time: boundary, WithdrawalsHash: &empty}
	if err := VerifyMainnetWithdrawals(&h, encoded); err == nil || err.Error() != "WITHDRAWALS_LIMIT" {
		t.Fatal(err)
	}
}
