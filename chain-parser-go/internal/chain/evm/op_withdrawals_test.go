package evm

import (
	"compress/gzip"
	"context"
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"os"
	"testing"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
	"xone/chain-parser-go/internal/domain"
)

func TestOPMainnetWithdrawalForks(t *testing.T) {
	other := common.HexToHash("0x1234")
	for _, tc := range []struct {
		ts             uint64
		root, requests *common.Hash
		body           string
		valid          bool
	}{
		{1704992400, nil, nil, "null", true}, {1704992400, nil, nil, "[]", false},
		{1704992401, &types.EmptyWithdrawalsHash, nil, "[ \n ]", true}, {1704992401, &other, nil, "[]", false},
		{opIsthmusTime - 1, &types.EmptyWithdrawalsHash, &types.EmptyRequestsHash, "[]", false},
		{opIsthmusTime, &other, &types.EmptyRequestsHash, "[]", true}, {opIsthmusTime, &other, &other, "[]", false},
		{opIsthmusTime, nil, &types.EmptyRequestsHash, "[]", false}, {opIsthmusTime, &other, nil, "[]", false},
		{opIsthmusTime, &other, &types.EmptyRequestsHash, "null", false}, {opIsthmusTime, &other, &types.EmptyRequestsHash, "[{}]", false},
	} {
		err := verifyOPMainnetWithdrawalFields(&types.Header{Time: tc.ts, WithdrawalsHash: tc.root, RequestsHash: tc.requests}, json.RawMessage(tc.body))
		if (err == nil) != tc.valid {
			t.Fatalf("slot time %d body %q: %v", tc.ts, tc.body, err)
		}
	}
}

func TestOPMainnetBlock159769967(t *testing.T) {
	f, err := os.Open("../../../../integration/test/source/op-mainnet-withdrawals/block-159769967.json.gz")
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	z, err := gzip.NewReader(f)
	if err != nil {
		t.Fatal(err)
	}
	defer z.Close()
	var data struct {
		Block, Pair json.RawMessage
		Receipts    []Receipt
		Code        string
	}
	if err = json.NewDecoder(z).Decode(&data); err != nil {
		t.Fatal(err)
	}
	ctx := context.Background()
	body, err := VerifyChainBlockJSON(ctx, "evm:10", data.Block, 64<<20)
	if err != nil {
		t.Fatal(err)
	}
	if body.Block.Hash != "0x82be41f91788d9eba982c7059d12e42475158b6bbbe3daa55490e04cfacc2e5d" || len(body.RawTransactions) != 66 {
		t.Fatal(body.Block.Hash, len(body.RawTransactions))
	}
	if err = VerifyChainReceiptRoot(ctx, "evm:10", body.Block, data.Receipts); err != nil {
		t.Fatal(err)
	}
	anchor := ExecutionStateAnchor{Height: 159769967, SettlingHeight: 159769967, BlockHash: body.Block.Hash, SettlingHash: body.Block.Hash, StateRoot: body.HeaderCommitment.StateRoot, AccountEncodingID: "ethereum-account-rlp/v1", Authority: "native-post-execution-header-state-root"}
	pair, err := VerifyExecutionAccountProof(ctx, anchor, data.Pair, 1<<20)
	if err != nil {
		t.Fatal(err)
	}
	code := common.FromHex(data.Code)
	if !pair.Exists || pair.Address != "0x4c43646304492a925e335f2b6d840c1489f17815" || len(pair.StorageValues) != 7 || pair.CodeHash != crypto.Keccak256Hash(code).Hex() || fmt.Sprintf("%x", sha256.Sum256(code)) != domain.V2CompletePairCodeSHA256 {
		t.Fatal("unexpected Pair commitment")
	}
	var block map[string]json.RawMessage
	if err = json.Unmarshal(data.Block, &block); err != nil {
		t.Fatal(err)
	}
	block["withdrawals"] = json.RawMessage(`[{}]`)
	raw, err := json.Marshal(block)
	if err != nil {
		t.Fatal(err)
	}
	if _, err = VerifyChainBlockJSON(ctx, "evm:10", raw, 64<<20); err == nil || err.Error() != "OP_WITHDRAWALS_EMPTY_LIST_REQUIRED" {
		t.Fatal(err)
	}
}
