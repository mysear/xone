package rpc

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"strings"
)

const MaxEthCallBatch = 16
const batchArchiveMethod = "jsonrpc_batch"

type ethCallBatch [][]any

// BatchEthCall returns results in request order. Each request carries its own
// block reference and admission costs one logical call per element.
func (c *Client) BatchEthCall(ctx context.Context, params [][]any) ([]string, error) {
	if len(params) == 0 || len(params) > MaxEthCallBatch {
		return nil, errors.New("RPC_BATCH_SIZE")
	}
	var result []string
	err := c.Call(ctx, "eth_call", ethCallBatch(params), &result)
	return result, err
}

func encodeRequest(method string, params any) ([]byte, error) {
	if batch, ok := params.(ethCallBatch); ok {
		requests := make([]map[string]any, len(batch))
		for i, params := range batch {
			requests[i] = map[string]any{"jsonrpc": "2.0", "id": i + 1, "method": method, "params": params}
		}
		return json.Marshal(requests)
	}
	return json.Marshal(map[string]any{"jsonrpc": "2.0", "id": 1, "method": method, "params": params})
}

// Decode at most count objects. Reject ambiguous IDs and fields before any
// caller can observe a prefix from a failed or reordered batch.
func decodeEthCallBatch(raw []byte, count int) ([]string, error) {
	invalid := errors.New("RPC_INVALID_BATCH_ENVELOPE")
	dec := json.NewDecoder(bytes.NewReader(raw))
	if tok, err := dec.Token(); err != nil || tok != json.Delim('[') {
		return nil, invalid
	}
	results := make([]string, count)
	seen := make([]bool, count)
	for i := 0; i < count; i++ {
		if tok, err := dec.Token(); err != nil || tok != json.Delim('{') {
			return nil, invalid
		}
		var member rpcResponse
		fields := make(map[string]bool)
		for dec.More() {
			tok, err := dec.Token()
			key, ok := tok.(string)
			if err != nil || !ok || fields[strings.ToLower(key)] {
				return nil, invalid
			}
			fields[strings.ToLower(key)] = true
			switch key {
			case "jsonrpc":
				err = dec.Decode(&member.JSONRPC)
			case "id":
				err = dec.Decode(&member.ID)
			case "result":
				err = dec.Decode(&member.Result)
			case "error":
				err = dec.Decode(&member.Error)
			default:
				var ignored json.RawMessage
				err = dec.Decode(&ignored)
			}
			if err != nil {
				return nil, invalid
			}
		}
		if tok, err := dec.Token(); err != nil || tok != json.Delim('}') {
			return nil, invalid
		}
		if member.JSONRPC != "2.0" || member.ID < 1 || member.ID > count || seen[member.ID-1] || fields["result"] == fields["error"] {
			return nil, invalid
		}
		if fields["error"] {
			if member.Error == nil {
				return nil, invalid
			}
			return nil, fmt.Errorf("RPC_ERROR_%d", member.Error.Code)
		}
		if len(member.Result) == 0 || member.Result[0] != '"' || json.Unmarshal(member.Result, &results[member.ID-1]) != nil {
			return nil, invalid
		}
		seen[member.ID-1] = true
	}
	if tok, err := dec.Token(); err != nil || tok != json.Delim(']') {
		return nil, invalid
	}
	if _, err := dec.Token(); err != io.EOF {
		return nil, invalid
	}
	return results, nil
}
