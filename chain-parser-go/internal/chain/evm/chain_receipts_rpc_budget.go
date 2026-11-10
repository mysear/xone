package evm

import (
	"errors"
	"io"
	"net/http"
)

// Count each attempt's request before sending, and every response payload,
// including empty errors and RPC-client retries. Logical call overhead is
// charged separately; charging a request only once would let retries evade
// the aggregate native-proof budget.
type nativeBudgetTransport struct {
	base   http.RoundTripper
	budget *receiptProofBudget
}

func (t nativeBudgetTransport) RoundTrip(request *http.Request) (*http.Response, error) {
	if t.budget.remaining <= 0 {
		return nil, errors.New("RECEIPT_PROOF_BUDGET")
	}
	// RPC requests use a marshaled byte reader with an exact ContentLength.
	// Refuse unknown or unaffordable bodies before any network transmission.
	if request.ContentLength < 0 || request.ContentLength > int64(t.budget.remaining) {
		t.budget.remaining = 0
		return nil, errors.New("RECEIPT_PROOF_BUDGET")
	}
	if err := t.budget.charge(int(request.ContentLength)); err != nil {
		return nil, err
	}
	response, err := t.base.RoundTrip(request)
	if err != nil {
		return nil, err
	}
	response.Body = &nativeBudgetBody{ReadCloser: response.Body, budget: t.budget}
	return response, nil
}

type nativeBudgetBody struct {
	io.ReadCloser
	budget *receiptProofBudget
}

func (b *nativeBudgetBody) Read(buffer []byte) (int, error) {
	if len(buffer) == 0 {
		return 0, nil
	}
	if b.budget.remaining <= 0 {
		return 0, errors.New("RECEIPT_PROOF_BUDGET")
	}
	width := min(len(buffer), b.budget.remaining+1)
	n, err := b.ReadCloser.Read(buffer[:width])
	if charged := b.budget.charge(n); charged != nil {
		b.budget.remaining = 0
		return n, charged
	}
	return n, err
}
