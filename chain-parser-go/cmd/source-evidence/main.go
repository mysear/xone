// source-evidence captures or verifies a finite immutable EVM source bundle.
package main

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"time"
	"xone/chain-parser-go/internal/provenance"
)

func main() {
	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
func writeNew(path string, data []byte) error {
	dir := filepath.Dir(path)
	file, err := os.CreateTemp(dir, ".source-evidence-")
	if err != nil {
		return errors.New("EVIDENCE_OUTPUT")
	}
	name := file.Name()
	defer os.Remove(name)
	if _, err = file.Write(data); err == nil {
		err = file.Sync()
	}
	closeErr := file.Close()
	if err != nil || closeErr != nil {
		return errors.New("EVIDENCE_OUTPUT")
	}
	// Link is exclusive and atomic: another capture can never replace this path.
	if err = os.Link(name, path); err != nil {
		return errors.New("EVIDENCE_OUTPUT_EXISTS_OR_UNAVAILABLE")
	}
	directory, err := os.Open(dir)
	if err != nil {
		return errors.New("EVIDENCE_DIRECTORY")
	}
	defer directory.Close()
	return directory.Sync()
}
func run() error {
	primary := flag.String("rpc", "", "primary read-only EVM RPC endpoint")
	witness := flag.String("witness-rpc", "", "separately selected witness RPC endpoint")
	chain := flag.String("chain-id", "1", "decimal chain ID")
	block := flag.String("block", "finalized", "finalized, safe or explicit canonical hex block number")
	addresses := flag.String("addresses", "", "comma-separated deployment addresses")
	storageSlots := flag.String("storage-slots", "", "comma-separated canonical 32-byte hex storage keys for every deployment")
	output := flag.String("out", "", "new output file for captured evidence")
	verify := flag.String("verify", "", "offline evidence file")
	flag.Parse()
	if *verify != "" {
		f, err := os.Open(*verify)
		if err != nil {
			return errors.New("EVIDENCE_INPUT")
		}
		defer f.Close()
		data, err := io.ReadAll(io.LimitReader(f, provenance.MaxEvidenceBytes+1))
		if err != nil || len(data) > provenance.MaxEvidenceBytes {
			return errors.New("EVIDENCE_INPUT_LIMIT")
		}
		var evidence provenance.Evidence
		decoder := json.NewDecoder(bytes.NewReader(data))
		decoder.DisallowUnknownFields()
		if decoder.Decode(&evidence) != nil {
			return errors.New("EVIDENCE_JSON")
		}
		report, err := provenance.Verify(evidence)
		if err != nil {
			return err
		}
		return json.NewEncoder(os.Stdout).Encode(report)
	}
	if *primary == "" || *witness == "" || *addresses == "" || *output == "" {
		return errors.New("EVIDENCE_CAPTURE_ARGUMENTS_REQUIRED")
	}
	for _, source := range []string{*primary, *witness} {
		u, err := url.Parse(source)
		if err != nil || u.Scheme != "https" || u.Hostname() == "" || u.User != nil || u.Fragment != "" {
			return errors.New("EVIDENCE_HTTPS_RPC_REQUIRED")
		}
	}
	httpClient := &http.Client{Timeout: 20 * time.Second, CheckRedirect: func(*http.Request, []*http.Request) error { return errors.New("EVIDENCE_REDIRECT") }}
	ctx, cancel := context.WithTimeout(context.Background(), 180*time.Second)
	defer cancel()
	observations := []map[string]any{}
	caller := func(ctx context.Context, source, method string, params, out any) error {
		request, _ := json.Marshal(map[string]any{"jsonrpc": "2.0", "id": 1, "method": method, "params": params})
		req, err := http.NewRequestWithContext(ctx, http.MethodPost, source, bytes.NewReader(request))
		if err != nil {
			return errors.New("EVIDENCE_RPC_REQUEST")
		}
		req.Header.Set("Content-Type", "application/json")
		response, err := httpClient.Do(req)
		if err != nil {
			return errors.New("EVIDENCE_RPC_NETWORK")
		}
		defer response.Body.Close()
		raw, err := io.ReadAll(io.LimitReader(response.Body, provenance.MaxEvidenceBytes+1))
		if err != nil || len(raw) > provenance.MaxEvidenceBytes {
			return errors.New("EVIDENCE_RPC_LIMIT")
		}
		if response.StatusCode != 200 {
			return fmt.Errorf("EVIDENCE_RPC_HTTP_%d", response.StatusCode)
		}
		sourceHash := sha256.Sum256([]byte(source))
		sum := sha256.Sum256(raw)
		observations = append(observations, map[string]any{"source_id": fmt.Sprintf("%x", sourceHash), "method": method, "raw_sha256": fmt.Sprintf("%x", sum), "raw_response": json.RawMessage(raw)})
		var envelope struct {
			JSONRPC string          `json:"jsonrpc"`
			ID      int             `json:"id"`
			Result  json.RawMessage `json:"result"`
			Error   *struct {
				Code int `json:"code"`
			} `json:"error"`
		}
		if json.Unmarshal(raw, &envelope) != nil || envelope.JSONRPC != "2.0" || envelope.ID != 1 {
			return errors.New("EVIDENCE_RPC_ENVELOPE")
		}
		if envelope.Error != nil {
			return fmt.Errorf("EVIDENCE_RPC_ERROR_%d", envelope.Error.Code)
		}
		if len(envelope.Result) == 0 || string(envelope.Result) == "null" || json.Unmarshal(envelope.Result, out) != nil {
			return errors.New("EVIDENCE_RPC_RESULT")
		}
		return nil
	}
	keys := map[string][]string{}
	if *storageSlots != "" {
		for _, address := range strings.Split(*addresses, ",") {
			keys[address] = strings.Split(*storageSlots, ",")
		}
	}
	evidence, report, err := provenance.Capture(ctx, provenance.CaptureRequest{Primary: *primary, Witness: *witness, ChainID: *chain, BlockTag: *block, Addresses: strings.Split(*addresses, ","), StorageKeys: keys}, caller)
	if err != nil {
		return err
	}
	raw, err := json.MarshalIndent(evidence, "", "  ")
	if err != nil {
		return err
	}
	if err = writeNew(*output, append(raw, '\n')); err != nil {
		return err
	}
	sidecar, _ := json.MarshalIndent(map[string]any{"schema_version": 1, "report": report, "captured_at_unix_ms": time.Now().UnixMilli(), "observations": observations, "provider_failure_domain_verified": false, "production_ready": false}, "", "  ")
	if err = writeNew(*output+".observations.json", append(sidecar, '\n')); err != nil {
		return err
	}
	return json.NewEncoder(os.Stdout).Encode(report)
}
