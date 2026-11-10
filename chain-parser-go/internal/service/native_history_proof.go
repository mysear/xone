package service

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"strconv"
	"strings"

	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/ingest"
)

type nativeHistoryArchive struct {
	Kind     string                    `json:"source_kind"`
	Evidence evm.NativeHistoryEvidence `json:"evidence"`
}

// Install at source construction; scope copies retain the same responsibility
// sink. The artifact has its own kind, so no derived proof is presented as an
// actual JSON-RPC response.
func (h *historyPublicSource) installNativeHistoryEvidence() {
	h.adapter.OnHistoryEvidence = h.captureNativeHistoryEvidence
}
func (h *historyPublicSource) captureNativeHistoryEvidence(ctx context.Context, evidence evm.NativeHistoryEvidence) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	group, _ := ctx.Value(rawCaptureKey{}).(*rawCapture)
	if group == nil || h.recorder == nil || h.recorder.archive == nil {
		return errors.New("HISTORY_NATIVE_CAPTURE_RESPONSIBILITY_REQUIRED")
	}
	raw, err := json.Marshal(nativeHistoryArchive{Kind: "native-history-proof/v2", Evidence: evidence})
	if err != nil {
		return err
	}
	if len(raw) > h.remaining {
		return errors.New("HISTORY_SOURCE_BUFFER_BUDGET")
	}
	height, err := strconv.ParseUint(strings.TrimPrefix(evidence.Block.Number, "0x"), 16, 64)
	if err != nil || evidence.Block.Number != fmt.Sprintf("0x%x", height) {
		return errors.New("HISTORY_NATIVE_BLOCK_HEIGHT")
	}
	recorder := h.recorder
	recorder.mu.Lock()
	recorder.sequence++
	sequence := recorder.sequence
	maintenance := recorder.maintenance
	recorder.mu.Unlock()
	group.mu.Lock()
	defer group.mu.Unlock()
	if group.Chain != recorder.chain || group.Sealed || len(group.Pins) >= group.Max {
		return errors.New("RAW_CAPTURE_CLOSED_OR_BUDGET")
	}
	pin := fmt.Sprintf("native-history-%s-%s-%d", recorder.boot, group.ID, sequence)
	// The owner binds the exact artifact digest and acquisition. Its Block inside
	// the envelope retains the canonical target identity and actual semantics ID.
	owner := ingest.Owner{Chain: recorder.chain, Height: height, Hash: rawDigest(append([]byte(group.ID+"/"+fmt.Sprint(sequence)+"/"), raw...)), ProviderID: "native-history-proof/v2"}
	var ref ingest.RawRef
	if maintenance != nil {
		ref, err = maintenance.putEvidencePinned(ctx, recorder.chain, owner, raw, pin)
	} else {
		ref, err = recorder.archive.PutPinned(owner, raw, pin)
	}
	if err != nil {
		return err
	}
	h.remaining -= len(raw)
	group.Pins = append(group.Pins, RawPin{Raw: ref, Reference: pin})
	recorder.mu.Lock()
	recorder.recent = &ref
	recorder.mu.Unlock()
	return ctx.Err()
}

// ReplayNativeHistoryEvidence rechecks the complete archived input under the
// current frozen pool/factory/decoder scope. Archive ownership and the actual
// fork-specific dataset are independent requirements.
func ReplayNativeHistoryEvidence(ctx context.Context, archive *ingest.Archive, ref ingest.RawRef, chain config.Chain) (domain.FactBatch, []ingest.FactoryCandidate, error) {
	var empty domain.FactBatch
	if err := ctx.Err(); err != nil {
		return empty, nil, err
	}
	if archive == nil || ref.Owner.Chain != chain.Key || ref.Owner.ProviderID != "native-history-proof/v2" {
		return empty, nil, errors.New("HISTORY_NATIVE_ARCHIVE_IDENTITY")
	}
	limits, err := evm.NativeRuntimeLimits(chain)
	if err != nil {
		return empty, nil, err
	}
	if ref.Bytes > int64(limits.MaxBytes) {
		return empty, nil, ingest.ErrBudget
	}
	raw, err := archive.Read(ref)
	if err != nil {
		return empty, nil, err
	}
	if len(raw) > limits.MaxBytes {
		return empty, nil, ingest.ErrBudget
	}
	var envelope nativeHistoryArchive
	if err = json.Unmarshal(raw, &envelope); err != nil {
		return empty, nil, err
	}
	if envelope.Kind != "native-history-proof/v2" || envelope.Evidence.Block.Number != fmt.Sprintf("0x%x", ref.Owner.Height) {
		return empty, nil, errors.New("HISTORY_NATIVE_ARCHIVE_IDENTITY")
	}
	return evm.VerifyNativeHistoryEvidence(ctx, chain, envelope.Evidence)
}
