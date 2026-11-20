package evm

import (
	"bytes"
	"context"
	"encoding/binary"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"syscall"
	"testing"
)

func TestOPSpanCaptureRange(t *testing.T) {
	prefix := binary.AppendUvarint([]byte{1}, (159774755-opBedrockHeight)*opBlockSeconds)
	prefix = binary.AppendUvarint(prefix, 26453861)
	prefix = append(prefix, make([]byte, 40)...)
	prefix = binary.AppendUvarint(prefix, 24)
	first, count, err := opSpanHeightRange(prefix)
	if err != nil || first != 159774755 || count != 24 {
		t.Fatal(first, count, err)
	}
	for i := range prefix {
		if _, _, err := opSpanHeightRange(prefix[:i]); err == nil {
			t.Fatal("truncated prefix accepted", i)
		}
	}
	for _, bad := range [][]byte{{1, 1}, append(bytes.Clone(prefix[:len(prefix)-1]), 0), binary.AppendUvarint(bytes.Clone(prefix[:len(prefix)-1]), 1000001)} {
		if _, _, err := opSpanHeightRange(bad); err == nil {
			t.Fatal("invalid range")
		}
	}
}

func TestOPSpanRefreshPreservesPreviousObservation(t *testing.T) {
	path := filepath.Join(t.TempDir(), "owner.json")
	good := func() error { return nil }
	raw, err := json.Marshal(OPChannelObservation{Blocks: []json.RawMessage{json.RawMessage(`{"number":"0xb","hash":"one"}`)}})
	if err != nil {
		t.Fatal(err)
	}
	previous := &OPSpanReport{FirstBlock: 11, LastBlock: 11, blockHashes: []string{"one"}}
	if err = installOPSpan(path+".beacon", path, raw, previous, good, false); err != nil {
		t.Fatal(err)
	}
	for _, tc := range []struct {
		tip    *OPSpanReport
		gate   func() error
		reason string
	}{{&OPSpanReport{FirstBlock: 10, LastBlock: 10}, good, "OP_SPAN_REFRESH_CONFLICT"}, {&OPSpanReport{FirstBlock: 11, LastBlock: 12, blockHashes: []string{"other", "last"}}, good, "OP_SPAN_REFRESH_CONFLICT"}, {previous, func() error { return errors.New("OP_SPAN_REFRESH_HEAD_CHANGED") }, "OP_SPAN_REFRESH_HEAD_CHANGED"}, {previous, func() error { return context.Canceled }, context.Canceled.Error()}} {
		if err = installOPSpan(path+".beacon", path, []byte("changed"), tc.tip, tc.gate, false); err == nil || err.Error() != tc.reason {
			t.Fatal(err)
		}
		content, e := os.ReadFile(path)
		if e != nil || !bytes.Equal(content, raw) {
			t.Fatal(e, string(content))
		}
	}
}

func TestOPSpanPublicationPinsBeaconThroughFinalCheck(t *testing.T) {
	dir := t.TempDir()
	proof, path := filepath.Join(dir, "beacon"), filepath.Join(dir, "channel")
	lock, err := os.OpenFile(proof+".lock", os.O_CREATE|os.O_RDWR, 0600)
	if err != nil {
		t.Fatal(err)
	}
	defer lock.Close()
	writer := func() error { return syscall.Flock(int(lock.Fd()), syscall.LOCK_EX|syscall.LOCK_NB) }
	if err := writer(); err != nil {
		t.Fatal(err)
	}
	raw := []byte(`{"blocks":[{"number":"0xb","hash":"one"}]}`)
	tip := &OPSpanReport{FirstBlock: 11, LastBlock: 11, blockHashes: []string{"one"}}
	if err := os.WriteFile(path, raw, 0600); err != nil {
		t.Fatal(err)
	}
	err = installOPSpan(proof, path, []byte("changed"), tip, func() error { t.Fatal("entered gate during beacon refresh"); return nil }, false)
	if err == nil || err.Error() != "OP_SPAN_BEACON_REFRESH_BUSY" {
		t.Fatal(err)
	}
	if got, err := os.ReadFile(path); err != nil || !bytes.Equal(got, raw) {
		t.Fatal("previous channel lost", err)
	}
	if err := syscall.Flock(int(lock.Fd()), syscall.LOCK_UN); err != nil {
		t.Fatal(err)
	}
	err = installOPSpan(proof, path, raw, tip, func() error {
		if err := writer(); !errors.Is(err, syscall.EWOULDBLOCK) {
			t.Fatalf("beacon writer at final check: %v", err)
		}
		return nil
	}, false)
	if err != nil {
		t.Fatal(err)
	}
	if err := writer(); err != nil {
		t.Fatal("beacon lock not released", err)
	}
}

func TestOPForwardPublisherRequiresAdjacentBlockAndParent(t *testing.T) {
	dir := t.TempDir()
	proof, path := filepath.Join(dir, "beacon"), filepath.Join(dir, "span")
	raw := []byte(`{"blocks":[{"number":"0xa","hash":"parent"}]}`)
	if err := os.WriteFile(path, raw, 0600); err != nil {
		t.Fatal(err)
	}
	for _, tip := range []OPSpanReport{{FirstBlock: 12, LastBlock: 12, firstParent: "parent"}, {FirstBlock: 11, LastBlock: 11, firstParent: "other"}} {
		if err := installOPSpan(proof, path, []byte("new"), &tip, nil, true); err == nil || err.Error() != "OP_SPAN_REFRESH_DISCONTINUITY" {
			t.Fatal(err)
		}
		if got, err := os.ReadFile(path); err != nil || !bytes.Equal(got, raw) {
			t.Fatal(err)
		}
	}
	tip := &OPSpanReport{FirstBlock: 11, LastBlock: 11, firstParent: "parent", blockHashes: []string{"next"}}
	if err := installOPSpan(proof, path, []byte(`{"blocks":[{"number":"0xb","hash":"next"}]}`), tip, func() error { return nil }, true); err != nil {
		t.Fatal(err)
	}
}
