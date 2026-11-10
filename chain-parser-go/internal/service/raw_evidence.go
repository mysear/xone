package service

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"sync"
	"xone/chain-parser-go/internal/ingest"
)

type rawRecorder struct {
	chain       string
	archive     *ingest.Archive
	mu          sync.Mutex
	recent      *ingest.RawRef
	boot        string
	sequence    uint64
	maintenance *RawMaintenance
}

func newRawRecorder(chain string, archive *ingest.Archive) *rawRecorder {
	var token [16]byte
	if _, err := rand.Read(token[:]); err != nil {
		panic(err)
	}
	return &rawRecorder{chain: chain, archive: archive, boot: hex.EncodeToString(token[:])}
}
func (r *rawRecorder) capture(ctx context.Context, provider, method string, params json.RawMessage, response []byte) error {
	if e := ctx.Err(); e != nil {
		return e
	}
	data, e := json.Marshal(struct {
		Kind     string          `json:"source_kind"`
		Method   string          `json:"method"`
		Params   json.RawMessage `json:"params"`
		Response []byte          `json:"response"`
	}{"raw-json-rpc-response", method, params, response})
	if e != nil {
		return errors.New("SOURCE_ARCHIVE_ENCODING")
	}
	hash := sha256.Sum256(data)
	group, _ := ctx.Value(rawCaptureKey{}).(*rawCapture)
	r.mu.Lock()
	maintenance := r.maintenance
	r.sequence++
	sequence := r.sequence
	r.mu.Unlock()
	if group == nil && maintenance != nil {
		return errors.New("RAW_CAPTURE_RESPONSIBILITY_REQUIRED")
	}
	owner := ingest.Owner{Chain: r.chain, ProviderID: provider, Height: 0, Hash: hex.EncodeToString(hash[:])}
	var ref ingest.RawRef
	if group == nil {
		// Legacy raw evidence has no maintenance consumer: preserve Put semantics.
		ref, e = r.archive.Put(owner, data)
	} else {
		group.mu.Lock()
		defer group.mu.Unlock()
		if group.Chain != r.chain || group.Sealed || len(group.Pins) >= group.Max {
			return errors.New("RAW_CAPTURE_CLOSED_OR_BUDGET")
		}
		pin := fmt.Sprintf("http-%s-%s-%d", r.boot, group.ID, sequence)
		// An HTTP observation identity is not a canonical block hash. Give each
		// actual acquisition its own owner while retaining the exact content digest;
		// identical responses remain capturable after a previous observation is GC'd.
		owner.Hash = rawDigest([]byte(owner.Hash + "/" + group.ID + "/" + fmt.Sprint(sequence)))
		if maintenance != nil {
			ref, e = maintenance.putEvidencePinned(ctx, r.chain, owner, data, pin)
		} else {
			ref, e = r.archive.PutPinned(owner, data, pin)
		}
		if e == nil {
			group.Pins = append(group.Pins, RawPin{Raw: ref, Reference: pin})
		}
	}
	if e != nil {
		return e
	}
	r.mu.Lock()
	r.recent = &ref
	r.mu.Unlock()
	return ctx.Err()
}
func (r *rawRecorder) last() (ingest.RawRef, bool) {
	r.mu.Lock()
	defer r.mu.Unlock()
	if r.recent == nil {
		return ingest.RawRef{}, false
	}
	return *r.recent, true
}
func (r *rawRecorder) quarantine(code, scope string) error {
	ref, ok := r.last()
	if !ok {
		return nil
	}
	return r.archive.Quarantine(ingest.DeadLetter{Raw: ref, Stage: "source-collection", Code: code, Scope: scope, Message: "Last retained response for failed scope collection; not proof of sole failing input"})
}
