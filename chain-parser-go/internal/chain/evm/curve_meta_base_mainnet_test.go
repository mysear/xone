package evm

import (
	"context"
	"fmt"
	"os"
	"reflect"
	"testing"
	"time"

	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/rpc"
)

func TestCurveMetaBaseMainnetHistoricalState(t *testing.T) {
	endpoint := os.Getenv("XONE_CURVE_META_ARCHIVE_RPC")
	if endpoint == "" {
		t.Skip("set XONE_CURVE_META_ARCHIVE_RPC for the Ethereum mainnet historical check")
	}
	const blockHash = "0xd5dd0566e58aaea31aa1f496064dbd2db6901327806c76eb71c9678c360f46cc"
	ctx := context.Background()
	a := Adapter{Client: rpc.New([]string{endpoint}, time.Minute, 16<<20)}
	head, err := a.Block(ctx, fmt.Sprintf("0x%x", 21057112))
	if err != nil || head.Hash != blockHash {
		t.Fatalf("3pool archive block: hash %s, error %v", head.Hash, err)
	}
	anchor := map[string]any{"blockHash": blockHash, "requireCanonical": true}
	for address, sum := range map[string]string{
		domain.CurveMetaBasePool: domain.CurveMetaBaseRuntimeSHA256,
		domain.CurveMetaBaseLP:   domain.CurveMetaBaseLPRuntimeSHA256,
	} {
		if _, err := a.verifyCode(ctx, address, sum, anchor); err != nil {
			t.Fatal(err)
		}
	}
	amount := func(to, data string) (domain.Amount, error) {
		word, err := a.stableCall(ctx, to, data, anchor, 1)
		if err != nil {
			return "", err
		}
		return domain.AmountFromBytes(word)
	}
	got, err := a.readCurveMetaBasePool(ctx, anchor, amount)
	if err != nil {
		t.Fatal(err)
	}
	want := &domain.CurveMetaBasePoolState{
		Balances:      []domain.Amount{"27874370702970927066796009", "28456740289417", "103652003557574"},
		AdminBalances: []domain.Amount{"5702158527745295494588", "7179531568", "6760503189"},
		Rates:         []domain.Amount{"1000000000000000000", "1000000000000000000000000000000", "1000000000000000000000000000000"},
		Amplification: "200000",
		Fee:           1000000,
		AdminFee:      5000000000,
		TotalSupply:   "154429461875698069046282809",
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("unexpected 3pool state: %+v", got)
	}
	vp, err := amount(domain.CurveMetaBasePool, "0xbb7b8b80") // get_virtual_price()
	if err != nil || vp != "1035816725132420036" {
		t.Fatalf("unexpected 3pool virtual price: %v, %v", vp, err)
	}
}
