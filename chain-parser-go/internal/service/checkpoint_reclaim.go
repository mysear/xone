package service

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"path/filepath"
	"strings"
	"time"
	"xone/chain-parser-go/internal/checkpoint"
)

func reclaimResponse(w http.ResponseWriter, status int, code string, result *checkpoint.ReclaimResult) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	if code != "" {
		json.NewEncoder(w).Encode(map[string]any{"error": code, "reclamation": result})
		return
	}
	json.NewEncoder(w).Encode(result)
}
func reclaimBody(r *http.Request, fields ...string) (map[string]json.RawMessage, error) {
	raw, e := io.ReadAll(io.LimitReader(r.Body, 1025))
	if e != nil || len(raw) > 1024 {
		return nil, errors.New("CHECKPOINT_RECLAIM_REQUEST")
	}
	decoder := json.NewDecoder(bytes.NewReader(raw))
	token, e := decoder.Token()
	if e != nil || token != json.Delim('{') {
		return nil, errors.New("CHECKPOINT_RECLAIM_REQUEST")
	}
	allowed := map[string]bool{}
	for _, field := range fields {
		allowed[field] = true
	}
	values := map[string]json.RawMessage{}
	for decoder.More() {
		token, e := decoder.Token()
		if e != nil {
			return nil, errors.New("CHECKPOINT_RECLAIM_REQUEST")
		}
		key, ok := token.(string)
		if !ok || !allowed[key] || values[key] != nil {
			return nil, errors.New("CHECKPOINT_RECLAIM_REQUEST")
		}
		var value json.RawMessage
		if decoder.Decode(&value) != nil || bytes.Equal(bytes.TrimSpace(value), []byte("null")) {
			return nil, errors.New("CHECKPOINT_RECLAIM_REQUEST")
		}
		values[key] = value
	}
	if token, e = decoder.Token(); e != nil || token != json.Delim('}') || len(values) != len(fields) {
		return nil, errors.New("CHECKPOINT_RECLAIM_REQUEST")
	}
	var extra any
	if decoder.Decode(&extra) != io.EOF {
		return nil, errors.New("CHECKPOINT_RECLAIM_REQUEST")
	}
	return values, nil
}
func validReclaimRequestID(value string) bool {
	if len(value) != 64 || strings.Trim(value, "0") == "" {
		return false
	}
	for _, c := range value {
		if !(c >= '0' && c <= '9' || c >= 'a' && c <= 'f') {
			return false
		}
	}
	return true
}
func (s *Service) mountCheckpointReclaim(mux *http.ServeMux) {
	mux.HandleFunc("POST /internal/chains/{chain}/checkpoint-retention/apply", func(w http.ResponseWriter, r *http.Request) { s.checkpointReclaim(w, r, false) })
	mux.HandleFunc("POST /internal/chains/{chain}/checkpoint-retention/resume", func(w http.ResponseWriter, r *http.Request) { s.checkpointReclaim(w, r, true) })
	mux.HandleFunc("GET /internal/chains/{chain}/checkpoint-retention/status", func(w http.ResponseWriter, r *http.Request) {
		c := s.cfg.Config.Journal.CheckpointStore
		if s.cfg.Token == "" {
			reclaimResponse(w, 401, "CHECKPOINT_RECLAIM_AUTH_REQUIRED", nil)
			return
		}
		if c == nil || c.LocalRetention == nil || s.stores[r.PathValue("chain")] == nil {
			reclaimResponse(w, 503, "CHECKPOINT_RETENTION_UNAVAILABLE", nil)
			return
		}
		remote, e := checkpoint.NewRemote(*c, s.cfg.Config.Journal.SourceAuthTokens[c.BaseURL], s.cfg.Config.Journal.MaxBytes, time.Duration(s.cfg.Config.Journal.TimeoutMS)*time.Millisecond)
		if e != nil {
			reclaimResponse(w, 503, "CHECKPOINT_REMOTE_CONFIG", nil)
			return
		}
		defer remote.Close()
		dir := filepath.Join(s.cfg.Resolve(s.cfg.Config.State.Path), "checkpoints", strings.ReplaceAll(r.PathValue("chain"), ":", "-"))
		result, e := remote.ReclaimStatus(dir)
		if e != nil {
			reclaimResponse(w, 503, reclaimCode(e), nil)
			return
		}
		reclaimResponse(w, 200, "", &result)
	})
}
func reclaimCode(e error) string {
	code := e.Error()
	if strings.HasPrefix(code, "CHECKPOINT_") && !strings.ContainsAny(code, " /\\\r\n\t") {
		return code
	}
	if errors.Is(e, context.Canceled) || errors.Is(e, context.DeadlineExceeded) {
		return "CHECKPOINT_RECLAIM_CANCELLED"
	}
	return "CHECKPOINT_RECLAIM_FAILED"
}
func (s *Service) checkpointReclaim(w http.ResponseWriter, r *http.Request, resume bool) {
	c := s.cfg.Config.Journal.CheckpointStore
	store := s.stores[r.PathValue("chain")]
	if s.cfg.Token == "" {
		reclaimResponse(w, 401, "CHECKPOINT_RECLAIM_AUTH_REQUIRED", nil)
		return
	}
	if s.closed.Load() || c == nil || c.LocalRetention == nil || !c.LocalRetention.ReclaimEnabled || store == nil {
		reclaimResponse(w, 503, "CHECKPOINT_RECLAIM_DISABLED", nil)
		return
	}
	var apply checkpoint.RetentionApplyRequest
	var again checkpoint.RetentionResumeRequest
	if resume {
		values, e := reclaimBody(r, "operation_id")
		if e != nil || json.Unmarshal(values["operation_id"], &again.OperationID) != nil {
			reclaimResponse(w, 400, "CHECKPOINT_RECLAIM_REQUEST", nil)
			return
		}
	} else {
		values, e := reclaimBody(r, "plan_id", "observed_at_ms")
		if e != nil || json.Unmarshal(values["plan_id"], &apply.PlanID) != nil || json.Unmarshal(values["observed_at_ms"], &apply.ObservedAtMS) != nil {
			reclaimResponse(w, 400, "CHECKPOINT_RECLAIM_REQUEST", nil)
			return
		}
	}
	if resume && !validReclaimRequestID(again.OperationID) || !resume && (!validReclaimRequestID(apply.PlanID) || apply.ObservedAtMS < 0) {
		reclaimResponse(w, 400, "CHECKPOINT_RECLAIM_REQUEST", nil)
		return
	}
	ctx, cancel := context.WithTimeout(r.Context(), time.Duration(s.cfg.Config.Journal.TimeoutMS)*time.Millisecond)
	defer cancel()
	stop := context.AfterFunc(s.checkpointContext, cancel)
	defer stop()
	select {
	case <-ctx.Done():
		reclaimResponse(w, 503, "CHECKPOINT_RECLAIM_CANCELLED", nil)
		return
	case s.checkpointGate <- struct{}{}:
	}
	defer func() { <-s.checkpointGate }()
	boot, e := store.Bootstrap()
	if e != nil {
		reclaimResponse(w, 503, "CHECKPOINT_RETENTION_NOT_READY", nil)
		return
	}
	guard := func() error {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		if s.closed.Load() {
			return errors.New("CHECKPOINT_OWNER_CLOSED")
		}
		reference, cursor, e := store.BootstrapHead()
		if e != nil {
			return e
		}
		if reference != boot.Snapshot.Reference || cursor != boot.Cursor {
			return errors.New("CHECKPOINT_RETENTION_SOURCE_CHANGED")
		}
		return nil
	}
	remote, e := checkpoint.NewRemote(*c, s.cfg.Config.Journal.SourceAuthTokens[c.BaseURL], s.cfg.Config.Journal.MaxBytes, time.Duration(s.cfg.Config.Journal.TimeoutMS)*time.Millisecond)
	if e != nil {
		reclaimResponse(w, 503, "CHECKPOINT_REMOTE_CONFIG", nil)
		return
	}
	defer remote.Close()
	dir := filepath.Join(s.cfg.Resolve(s.cfg.Config.State.Path), "checkpoints", strings.ReplaceAll(r.PathValue("chain"), ":", "-"))
	var result checkpoint.ReclaimResult
	if resume {
		result, e = remote.ResumeRetention(ctx, dir, boot, *c.LocalRetention, again, time.Now(), guard)
	} else {
		result, e = remote.ApplyRetention(ctx, dir, boot, *c.LocalRetention, apply, time.Now(), guard)
	}
	if e != nil {
		reclaimResponse(w, 503, reclaimCode(e), &result)
		return
	}
	if result.Applied && !result.Idempotent {
		s.log.Event("info", "checkpoint.reclaimed", map[string]any{"chain_key": r.PathValue("chain"), "operation_id": result.OperationID, "completed_objects": result.CompletedObjects, "completed_bytes": result.CompletedBytes, "resumed": resume})
	}
	reclaimResponse(w, 200, "", &result)
}
