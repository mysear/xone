package rpc

import (
	"context"
	"encoding/json"
	"errors"
	"sync"
)

var ErrRecordingFailed = errors.New("RPC_RECORDING_FAILED")

const RecordingFailed ErrorClass = "recording_failed"

// ResponseRecorder runs synchronously before HTTP status, envelope or result
// interpretation. Only complete bodies within MaxBytes are delivered; oversized
// or interrupted reads are rejected rather than recorded as a complete response.
// Copies of params/body belong to the recorder. It must honor ctx and durably
// complete its own write before returning nil. ProviderID never contains a URL.
type ResponseRecorder func(ctx context.Context, providerID, method string, params json.RawMessage, response []byte) error
type recorderState struct {
	mu     sync.RWMutex
	record ResponseRecorder
}

func (c *Client) recorderState() *recorderState {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.recorder == nil {
		c.recorder = &recorderState{}
	}
	return c.recorder
}

// SetRecorder shares a live recorder with both existing and later endpoint-bound
// ProviderClients. nil disables it. Callback failures are local critical errors,
// not provider failures eligible for retry, capability downgrade or failover.
func (c *Client) SetRecorder(record ResponseRecorder) {
	state := c.recorderState()
	state.mu.Lock()
	state.record = record
	state.mu.Unlock()
}
func (c *Client) recordResponse(ctx context.Context, providerID, method string, request, response []byte) error {
	state := c.recorderState()
	state.mu.RLock()
	record := state.record
	state.mu.RUnlock()
	if record == nil {
		return nil
	}
	var envelope struct {
		Params json.RawMessage `json:"params"`
	}
	if err := json.Unmarshal(request, &envelope); err != nil {
		return &CallError{ProviderID: providerID, Class: RecordingFailed, Code: ErrRecordingFailed.Error(), Cause: errors.Join(ErrRecordingFailed, err)}
	}
	err := record(ctx, providerID, method, append(json.RawMessage(nil), envelope.Params...), append([]byte(nil), response...))
	if err != nil {
		return &CallError{ProviderID: providerID, Class: RecordingFailed, Code: ErrRecordingFailed.Error(), Cause: errors.Join(ErrRecordingFailed, err)}
	}
	return nil
}
