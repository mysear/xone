package service

import (
	"context"
	"crypto/subtle"
	"encoding/json"
	"io"
	"net/http"
	"strconv"
	"strings"
	"time"
)

// Handler authenticates independently; mounting it cannot bypass auth by missing
// the enclosing Service middleware. Reads are paged; Replay accepts identities
// only, with no decoder scripts, executable paths, state or receipt payloads.
func (m *RawMaintenance) Handler() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("GET /internal/chains/{chain}/raw/status", func(w http.ResponseWriter, r *http.Request) { json.NewEncoder(w).Encode(m.Status()) })
	mux.HandleFunc("GET /internal/chains/{chain}/raw/dlq", func(w http.ResponseWriter, r *http.Request) {
		state := m.chains[r.PathValue("chain")]
		if state == nil {
			http.Error(w, "RAW_CHAIN", 404)
			return
		}
		out, e := state.recorder.archive.DeadLetters()
		if e != nil {
			http.Error(w, e.Error(), 503)
			return
		}
		offset, e := strconv.Atoi(r.URL.Query().Get("offset"))
		if r.URL.Query().Get("offset") == "" {
			offset = 0
			e = nil
		}
		if e != nil || offset < 0 || offset > len(out) {
			http.Error(w, "RAW_PAGE", 400)
			return
		}
		writeRawMaintenancePage(w, out, offset, m.cfg.MaxPageRecords, m.cfg.MaxDecodedBytes)
	})
	for _, kind := range []string{"incidents", "audit"} {
		kind := kind
		mux.HandleFunc("GET /internal/chains/{chain}/raw/"+kind+"/{id}", func(w http.ResponseWriter, r *http.Request) {
			state := m.chains[r.PathValue("chain")]
			if state == nil {
				http.Error(w, "RAW_CHAIN", 404)
				return
			}
			offset := 0
			if r.URL.Query().Get("offset") != "" {
				v, e := strconv.Atoi(r.URL.Query().Get("offset"))
				if e != nil || v < 0 {
					http.Error(w, "RAW_PAGE", 400)
					return
				}
				offset = v
			}
			if kind == "incidents" {
				records, e := state.recorder.archive.FailureIncidents(r.PathValue("id"))
				if e != nil {
					http.Error(w, e.Error(), 409)
					return
				}
				if offset > len(records) {
					http.Error(w, "RAW_PAGE", 400)
					return
				}
				writeRawMaintenancePage(w, records, offset, m.cfg.MaxPageRecords, m.cfg.MaxDecodedBytes)
			} else {
				records, e := state.recorder.archive.ReplayAudit(r.PathValue("id"))
				if e != nil {
					http.Error(w, e.Error(), 409)
					return
				}
				if offset > len(records) {
					http.Error(w, "RAW_PAGE", 400)
					return
				}
				for i := range records {
					records[i].Decoded = nil
				}
				writeRawMaintenancePage(w, records, offset, m.cfg.MaxPageRecords, m.cfg.MaxDecodedBytes)
			}
		})
	}
	mux.HandleFunc("POST /internal/chains/{chain}/raw/replay", func(w http.ResponseWriter, r *http.Request) {
		ctx, cancel := context.WithTimeout(r.Context(), time.Duration(m.cfg.ReplayTimeoutMS)*time.Millisecond)
		defer cancel()
		r = r.WithContext(ctx)
		controller := http.NewResponseController(w)
		_ = controller.SetReadDeadline(time.Now().Add(time.Duration(m.cfg.TimeoutMS) * time.Millisecond))
		defer controller.SetReadDeadline(time.Time{})
		var q RawReplayRequest
		d := json.NewDecoder(http.MaxBytesReader(w, r.Body, m.cfg.MaxRequestBytes))
		d.DisallowUnknownFields()
		if d.Decode(&q) != nil || d.Decode(new(any)) != io.EOF {
			http.Error(w, "RAW_REPLAY_REQUEST", 400)
			return
		}
		result, e := m.Replay(r.Context(), r.PathValue("chain"), q)
		status := 200
		if e != nil {
			status = 409
		}
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(status)
		message := ""
		if e != nil {
			message = e.Error()
		}
		result.Decoded = nil
		json.NewEncoder(w).Encode(map[string]any{"result": result, "error": message})
	})
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		token := strings.TrimPrefix(r.Header.Get("Authorization"), "Bearer ")
		if m == nil || !strings.HasPrefix(r.Header.Get("Authorization"), "Bearer ") || subtle.ConstantTimeCompare([]byte(token), []byte(m.s.cfg.Token)) != 1 {
			http.Error(w, "UNAUTHORIZED", 401)
			return
		}
		mux.ServeHTTP(w, r)
	})
}

// Encode before writing headers or bytes. Binary search keeps the largest
// prefix within BOTH count and actual JSON byte limits, including the envelope.
// A record which cannot fit alone is explicit; an empty success page would lose
// pagination progress and could hide an incident from the operator.
func writeRawMaintenancePage[T any](w http.ResponseWriter, records []T, offset, maxRecords int, maxBytes int64) {
	type page struct {
		Records    []T `json:"records"`
		NextOffset int `json:"next_offset"`
		Total      int `json:"total"`
	}
	lo, hi := offset, min(len(records), offset+maxRecords)
	var encoded []byte
	best := offset
	for lo <= hi {
		end := lo + (hi-lo)/2
		raw, e := json.Marshal(page{records[offset:end], end, len(records)})
		if e != nil {
			rawMaintenancePageError(w, "RAW_PAGE_ENCODING", http.StatusInternalServerError, maxBytes)
			return
		}
		if int64(len(raw)) <= maxBytes {
			best, encoded = end, raw
			lo = end + 1
		} else {
			hi = end - 1
		}
	}
	if encoded == nil || (offset < len(records) && best == offset) {
		rawMaintenancePageError(w, "RAW_PAGE_RECORD_BUDGET", http.StatusRequestEntityTooLarge, maxBytes)
		return
	}
	w.Header().Set("Content-Type", "application/json")
	_, _ = w.Write(encoded)
}

func rawMaintenancePageError(w http.ResponseWriter, message string, status int, maxBytes int64) {
	w.Header().Set("X-Raw-Error", message)
	if int64(len(message)+1) > maxBytes {
		// Even a legal tiny byte budget must not be exceeded by the error body.
		w.WriteHeader(status)
		return
	}
	http.Error(w, message, status)
}
