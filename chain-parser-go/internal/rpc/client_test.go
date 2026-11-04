package rpc

import (
	"context"
	"fmt"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

func TestWrongChainQuarantinedAndResponseBounded(t *testing.T) {
	bad := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { fmt.Fprint(w, `{"jsonrpc":"2.0","id":1,"result":"0x1"}`) }))
	defer bad.Close()
	c := New([]string{bad.URL}, time.Second, 1024)
	if e := c.VerifyEVM(context.Background(), "evm:31337", "0x00"); e == nil {
		t.Fatal("wrong chain accepted")
	}
	huge := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		for range 2048 {
			fmt.Fprint(w, "x")
		}
	}))
	defer huge.Close()
	c = New([]string{huge.URL}, time.Second, 1024)
	var result any
	if c.Call(context.Background(), "method", []any{}, &result) == nil {
		t.Fatal("unbounded payload")
	}
}

func TestForkConfirmationRequiresDistinctAgreeingProviders(t *testing.T) {
	server := func(hash string) *httptest.Server {
		return httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			fmt.Fprintf(w, `{"jsonrpc":"2.0","id":1,"result":{"number":"0x64","hash":%q,"parentHash":"parent"}}`, hash)
		}))
	}
	a, b, bad := server("fork"), server("fork"), server("other")
	defer a.Close()
	defer b.Close()
	defer bad.Close()
	for _, urls := range [][]string{{a.URL}, {a.URL, a.URL}, {a.URL, bad.URL}} {
		c := New(urls, time.Second, 1024)
		if c.ConfirmEVMBlock(context.Background(), 100, "fork", "parent") == nil {
			t.Fatal("unconfirmed fork accepted")
		}
	}
	if e := New([]string{a.URL, b.URL}, time.Second, 1024).ConfirmEVMBlock(context.Background(), 100, "fork", "parent"); e != nil {
		t.Fatal(e)
	}
}
