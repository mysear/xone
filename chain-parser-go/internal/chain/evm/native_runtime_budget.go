package evm

import (
	"context"
	"errors"
	"io"
	"net/http"
)

// Native pool reads retain the original RPC client's gate, retry/failover and
// live recorder. Only this scoped HTTP transport is replaced; request/response
// bodies of every attempt consume the same native session as the commitments.
func (a *Adapter) nativeStateReader(ctx context.Context, block RawBlock) (*Adapter, error) {
	if a.Chain.BlockIntegrity != ChainNativeCommitmentsV2 {
		return a, nil
	}
	session := block.nativeSession
	if session == nil || session.ctx != ctx {
		return nil, errors.New("NATIVE_NORMAL_BODY_PROOF_REQUIRED")
	}
	session.mu.Lock()
	limits, err := session.available()
	session.mu.Unlock()
	if err != nil {
		return nil, err
	}
	base := a.Client.HTTP
	transport := base.Transport
	if transport == nil {
		transport = http.DefaultTransport
	}
	httpClient := &http.Client{Transport: nativeStateBudgetTransport{base: transport, session: session}, Timeout: base.Timeout, CheckRedirect: base.CheckRedirect, Jar: base.Jar}
	copy := a.CopyWithChain(a.Chain)
	copy.Client = a.Client.CopyWithHTTP(httpClient)
	copy.Client.MaxBytes = min(a.Client.MaxBytes, limits.MaxBytes)
	return copy, nil
}
func (s *nativeReadSession) consumeStateBytes(n int64) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if n < 0 || n > int64(s.remaining) {
		s.remaining = 0
		return errors.New("NATIVE_RUNTIME_PROOF_BUDGET")
	}
	return s.consume(int(n))
}
func (s *nativeReadSession) retainPool(encoded []byte) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.consume(len(encoded) + 256)
}

type nativeStateBudgetTransport struct {
	base    http.RoundTripper
	session *nativeReadSession
}

func (t nativeStateBudgetTransport) RoundTrip(request *http.Request) (*http.Response, error) {
	if err := t.session.consumeStateBytes(request.ContentLength); err != nil {
		return nil, err
	}
	response, err := t.base.RoundTrip(request)
	if err != nil {
		return nil, err
	}
	response.Body = &nativeStateBudgetBody{ReadCloser: response.Body, session: t.session}
	return response, nil
}

type nativeStateBudgetBody struct {
	io.ReadCloser
	session *nativeReadSession
}

func (b *nativeStateBudgetBody) Read(buffer []byte) (int, error) {
	if len(buffer) == 0 {
		return 0, nil
	}
	b.session.mu.Lock()
	defer b.session.mu.Unlock()
	if b.session.remaining <= 0 {
		return 0, errors.New("NATIVE_RUNTIME_PROOF_BUDGET")
	}
	n, err := b.ReadCloser.Read(buffer[:min(len(buffer), b.session.remaining+1)])
	if budgetErr := b.session.consume(n); budgetErr != nil {
		b.session.remaining = 0
		return n, budgetErr
	}
	return n, err
}

func (s *nativeReadSession) stateReadError(err error) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.remaining <= 0 {
		return errors.Join(errors.New("NATIVE_RUNTIME_PROOF_BUDGET"), err)
	}
	return err
}
