package rpc

import (
	"context"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

func TestProviderClientsAreDistinctHealthyAndEndpointBound(t *testing.T) {
	parent := New([]string{"http://a", "http://a", "http://b", "http://c"}, time.Second, 4096)
	parent.quarantined["http://b"] = true
	configured, clients := parent.ProviderClients()
	if configured != 3 || len(clients) != 2 {
		t.Fatal(configured, len(clients))
	}
	for i, url := range []string{"http://a", "http://c"} {
		child := clients[i]
		if len(child.URLs) != 1 || child.URLs[0] != url || child.HTTP != parent.HTTP || child.MaxBytes != parent.MaxBytes {
			t.Fatal("provider escaped its transport/limits", child)
		}
		child.quarantined[url] = true
		if parent.quarantined[url] {
			t.Fatal("child changed parent quarantine")
		}
	}
	if configured, clients := New(nil, time.Second, 4096).ProviderClients(); configured != 0 || len(clients) != 0 {
		t.Fatal(configured, clients)
	}
}

func TestForkConfirmationUsesByteEquivalentBlockHashCase(t *testing.T) {
	hash := "0x" + strings.Repeat("ab", 32)
	parent := "0x" + strings.Repeat("cd", 32)
	server := func(uppercase bool) *httptest.Server {
		return httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			h, p := hash, parent
			if uppercase {
				h = "0x" + strings.ToUpper(hash[2:])
				p = "0x" + strings.ToUpper(parent[2:])
			}
			fmt.Fprintf(w, `{"jsonrpc":"2.0","id":1,"result":{"number":"0x64","hash":%q,"parentHash":%q}}`, h, p)
		}))
	}
	a, b := server(false), server(true)
	defer a.Close()
	defer b.Close()
	if err := New([]string{a.URL, b.URL}, time.Second, 4096).ConfirmEVMBlock(context.Background(), 100, hash, parent); err != nil {
		t.Fatal(err)
	}
}
