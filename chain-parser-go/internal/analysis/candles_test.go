package analysis

import (
	"context"
	"encoding/json"
	"strings"
	"testing"
	"xone/chain-parser-go/internal/domain"
)

func TestIdempotentReorgRecomputesOHLC(t *testing.T) {
	a := New(60000)
	trades := []Trade{{ID: "low", Chain: "evm:1", Pool: "p", Block: "b1", TimestampMS: 1, TxIndex: 0, EventIndex: 0, Base: domain.Amount("10"), Quote: domain.Amount("10")}, {ID: "high", Chain: "evm:1", Pool: "p", Block: "b2", TimestampMS: 2, TxIndex: 0, EventIndex: 0, Base: "10", Quote: "100"}, {ID: "last", Chain: "evm:1", Pool: "p", Block: "b3", TimestampMS: 3, TxIndex: 0, EventIndex: 0, Base: "10", Quote: "20"}}
	for _, tr := range trades {
		if e := a.Apply(tr); e != nil {
			t.Fatal(e)
		}
	}
	a.Apply(trades[0])
	c := a.Candles()[0]
	if c.Count != 3 || c.High != "10" {
		t.Fatal(c)
	}
	a.Revert("evm:1", "b2")
	c = a.Candles()[0]
	if c.High != "2" || c.Low != "1" || c.Count != 2 || c.Revision < 2 {
		t.Fatal(c)
	}
}
func TestReplayIsDeterministic(t *testing.T) {
	raw := `{"schema_version":1,"plugin_version":"candles/1","interval_ms":60000,"events":[{"kind":"SWAP","trade":{"id":"one","chain_key":"evm:1","pool_id":"p","block_hash":"b","timestamp_ms":1,"tx_index":0,"event_index":0,"base_amount":"3","quote_amount":"10"}}]}`
	a, e := Replay(context.Background(), strings.NewReader(raw), 4096)
	if e != nil {
		t.Fatal(e)
	}
	b, e := Replay(context.Background(), strings.NewReader(raw), 4096)
	if e != nil {
		t.Fatal(e)
	}
	aa, _ := json.Marshal(a)
	bb, _ := json.Marshal(b)
	if string(aa) != string(bb) {
		t.Fatal("nondeterministic")
	}
}
