package analysis

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"os"
	"path/filepath"
)

type Operation struct {
	Kind  string `json:"kind"`
	Trade *Trade `json:"trade,omitempty"`
	Chain string `json:"chain_key,omitempty"`
	Block string `json:"block_hash,omitempty"`
}
type Definition struct {
	Schema     uint32      `json:"schema_version"`
	Version    string      `json:"plugin_version"`
	Interval   uint64      `json:"interval_ms"`
	Operations []Operation `json:"events"`
}
type Release struct {
	Schema             uint32   `json:"schema_version"`
	PluginVersion      string   `json:"plugin_version"`
	InputEvents        int      `json:"input_events"`
	Candles            []Candle `json:"candles"`
	HistoryIndependent bool     `json:"online_state_unmodified"`
}

func Replay(ctx context.Context, r io.Reader, maxBytes int64) (Release, error) {
	var out Release
	b, e := io.ReadAll(io.LimitReader(r, maxBytes+1))
	if e != nil || int64(len(b)) > maxBytes {
		return out, errors.New("ANALYTICS_INPUT_LIMIT")
	}
	var def Definition
	if e = json.Unmarshal(b, &def); e != nil || def.Schema != 1 || def.Version == "" || def.Interval == 0 {
		return out, errors.New("ANALYTICS_DEFINITION")
	}
	a := New(def.Interval)
	for _, op := range def.Operations {
		if ctx.Err() != nil {
			return out, errors.New("ANALYTICS_CANCELLED")
		}
		switch op.Kind {
		case "SWAP":
			if op.Trade == nil {
				return out, errors.New("ANALYTICS_EVENT_MISSING")
			}
			if e = a.Apply(*op.Trade); e != nil {
				return out, e
			}
		case "REVERT":
			if op.Chain == "" || op.Block == "" {
				return out, errors.New("ANALYTICS_REVERT_IDENTITY")
			}
			a.Revert(op.Chain, op.Block)
		default:
			return out, errors.New("ANALYTICS_EVENT_UNSUPPORTED")
		}
	}
	return Release{1, def.Version, len(def.Operations), a.Candles(), true}, nil
}
func Publish(path string, release Release) error {
	b, e := json.Marshal(release)
	if e != nil {
		return e
	}
	return publishBytes(path, b)
}
func publishBytes(path string, b []byte) error {
	if e := os.MkdirAll(filepath.Dir(path), 0700); e != nil {
		return e
	}
	f, e := os.CreateTemp(filepath.Dir(path), ".analysis-")
	if e != nil {
		return e
	}
	temp := f.Name()
	defer os.Remove(temp)
	if e = f.Chmod(0600); e != nil {
		f.Close()
		return e
	}
	if _, e = f.Write(b); e != nil {
		f.Close()
		return e
	}
	if e = f.Sync(); e != nil {
		f.Close()
		return e
	}
	if e = f.Close(); e != nil {
		return e
	}
	if e = os.Rename(temp, path); e != nil {
		return e
	}
	d, e := os.Open(filepath.Dir(path))
	if e != nil {
		return e
	}
	defer d.Close()
	return d.Sync()
}
