package checkpoint

import (
	"bytes"
	"context"
	"errors"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"time"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/journal"
)

// Remote uses a trusted, configured store implementing immutable conditional PUT.
// Successful publication includes a read-back of exact bytes, not a durability or WORM claim.
type Remote struct {
	base, token string
	client      *http.Client
	maxBytes    int
	timeout     time.Duration
}
type RemotePublication struct {
	ManifestURL    string            `json:"manifest_url"`
	ManifestSHA256 string            `json:"manifest_sha256"`
	Reference      BootstrapManifest `json:"manifest"`
}

func NewRemote(store config.CheckpointStore, token string, maxBytes int, timeout time.Duration) (*Remote, error) {
	if store.Validate() != nil || maxBytes < 1 || maxBytes > MaxBootstrapBytes || timeout <= 0 || timeout > time.Minute {
		return nil, errors.New("CHECKPOINT_REMOTE_CONFIG")
	}
	if len(token) > 8192 {
		return nil, errors.New("CHECKPOINT_REMOTE_TOKEN")
	}
	for _, v := range token {
		if v < 0x21 || v > 0x7e {
			return nil, errors.New("CHECKPOINT_REMOTE_TOKEN")
		}
	}
	transport := http.DefaultTransport.(*http.Transport).Clone()
	transport.Proxy = nil
	transport.DisableCompression = true
	return &Remote{base: store.BaseURL, token: token, maxBytes: maxBytes, timeout: timeout, client: &http.Client{Transport: transport, Timeout: timeout, CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }}}, nil
}
func (r *Remote) Close() { r.client.CloseIdleConnections() }
func (r *Remote) request(ctx context.Context, method, url string, raw []byte) (*http.Response, error) {
	req, e := http.NewRequestWithContext(ctx, method, url, bytes.NewReader(raw))
	if e != nil {
		return nil, errors.New("CHECKPOINT_REMOTE_REQUEST")
	}
	if r.token != "" {
		req.Header.Set("Authorization", "Bearer "+r.token)
	}
	if method == http.MethodPut {
		req.Header.Set("If-None-Match", "*")
		req.Header.Set("Content-Type", "application/octet-stream")
	}
	response, e := r.client.Do(req)
	if e != nil {
		if ctx.Err() != nil {
			return nil, ctx.Err()
		}
		return nil, errors.New("CHECKPOINT_REMOTE_NETWORK")
	}
	return response, nil
}
func (r *Remote) get(ctx context.Context, url string, limit int64) ([]byte, bool, error) {
	response, e := r.request(ctx, http.MethodGet, url, nil)
	if e != nil {
		return nil, false, e
	}
	defer response.Body.Close()
	if response.StatusCode == http.StatusNotFound {
		return nil, false, nil
	}
	if response.StatusCode != http.StatusOK || response.Header.Get("Content-Encoding") != "" || response.ContentLength > limit {
		return nil, false, errors.New("CHECKPOINT_REMOTE_STATUS_OR_LIMIT")
	}
	raw, e := io.ReadAll(io.LimitReader(response.Body, limit+1))
	if e != nil || int64(len(raw)) > limit {
		return nil, false, errors.New("CHECKPOINT_REMOTE_READ_LIMIT")
	}
	return raw, true, nil
}
func (r *Remote) put(ctx context.Context, url string, raw []byte) error {
	existing, ok, e := r.get(ctx, url, int64(len(raw)))
	if e != nil {
		return e
	}
	if ok {
		if !bytes.Equal(existing, raw) {
			return errors.New("CHECKPOINT_REMOTE_CONFLICT")
		}
		return nil
	}
	response, e := r.request(ctx, http.MethodPut, url, raw)
	if e != nil {
		return e
	}
	// Bodies are neither trusted nor retained; close without an unbounded drain.
	response.Body.Close()
	if response.StatusCode != http.StatusCreated && response.StatusCode != http.StatusOK && response.StatusCode != http.StatusNoContent && response.StatusCode != http.StatusPreconditionFailed && response.StatusCode != http.StatusConflict {
		return errors.New("CHECKPOINT_REMOTE_PUT_STATUS")
	}
	existing, ok, e = r.get(ctx, url, int64(len(raw)))
	if e != nil {
		return e
	}
	if !ok || !bytes.Equal(existing, raw) {
		return errors.New("CHECKPOINT_REMOTE_READBACK")
	}
	return nil
}
func (r *Remote) objects(payloadSHA string) (string, string, error) {
	if !digest(payloadSHA) {
		return "", "", errors.New("CHECKPOINT_REMOTE_OBJECT_ID")
	}
	directory := r.base + "/public-v2/" + payloadSHA + "/"
	return directory + "manifest.json", directory + "bootstrap.pb", nil
}
func (r *Remote) Publish(ctx context.Context, p Publication, guard func() error) (RemotePublication, error) {
	ctx, cancel := context.WithTimeout(ctx, r.timeout)
	defer cancel()
	fail := func(e error) (RemotePublication, error) { return RemotePublication{}, e }
	root, e := os.OpenRoot(filepath.Dir(p.ManifestPath))
	if e != nil {
		return fail(e)
	}
	defer root.Close()
	manifest, e := readRegular(root, "manifest.json", maxManifestBytes)
	if e != nil {
		return fail(e)
	}
	m, e := ParseBootstrapManifest(manifest, p.ManifestSHA256, r.maxBytes)
	if e != nil {
		return fail(e)
	}
	payload, e := readRegular(root, "bootstrap.pb", int64(m.Size))
	if e != nil {
		return fail(e)
	}
	if _, e = DecodeBootstrap(manifest, payload, p.ManifestSHA256, r.maxBytes); e != nil {
		return fail(e)
	}
	manifestURL, payloadURL, e := r.objects(m.SHA256)
	if e != nil {
		return fail(e)
	}
	check := func() error {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		if guard != nil {
			return guard()
		}
		return nil
	}
	if e = check(); e != nil {
		return fail(e)
	}
	if e = r.put(ctx, payloadURL, payload); e != nil {
		return fail(e)
	}
	if e = check(); e != nil {
		return fail(e)
	}
	if e = r.put(ctx, manifestURL, manifest); e != nil {
		return fail(e)
	}
	if e = check(); e != nil {
		return fail(e)
	}
	publication := r.publication(p, m)
	if e = r.recordCompletion(filepath.Dir(p.ManifestPath), publication); e != nil {
		return fail(e)
	}
	if e = check(); e != nil {
		return fail(e)
	}
	if e = r.recordCandidate(filepath.Dir(p.ManifestPath), publication); e != nil {
		return fail(e)
	}
	if e = check(); e != nil {
		return fail(e)
	}
	return publication, nil
}

// Fetch accepts a content ID and a pin obtained independently by the caller.
// It derives both URLs from configured base; a manifest can never redirect credentials.
func (r *Remote) Fetch(ctx context.Context, payloadSHA, manifestSHA string) (journal.Bootstrap, error) {
	ctx, cancel := context.WithTimeout(ctx, r.timeout)
	defer cancel()
	manifestURL, payloadURL, e := r.objects(payloadSHA)
	if e != nil {
		return journal.Bootstrap{}, e
	}
	if !digest(manifestSHA) {
		return journal.Bootstrap{}, errors.New("CHECKPOINT_SIZE_OR_PIN")
	}
	manifest, ok, e := r.get(ctx, manifestURL, maxManifestBytes)
	if e != nil {
		return journal.Bootstrap{}, e
	}
	if !ok {
		return journal.Bootstrap{}, errors.New("CHECKPOINT_REMOTE_MISSING")
	}
	m, e := ParseBootstrapManifest(manifest, manifestSHA, r.maxBytes)
	if e != nil {
		return journal.Bootstrap{}, e
	}
	if m.SHA256 != payloadSHA {
		return journal.Bootstrap{}, errors.New("CHECKPOINT_REMOTE_OBJECT_ID")
	}
	payload, ok, e := r.get(ctx, payloadURL, int64(m.Size))
	if e != nil {
		return journal.Bootstrap{}, e
	}
	if !ok {
		return journal.Bootstrap{}, errors.New("CHECKPOINT_REMOTE_MISSING")
	}
	return DecodeBootstrap(manifest, payload, manifestSHA, r.maxBytes)
}
