package checkpoint

import (
	"bytes"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"strconv"
	"xone/chain-parser-go/internal/journal"
)

func (r *Remote) publication(p Publication, m BootstrapManifest) RemotePublication {
	url, _, _ := r.objects(m.SHA256)
	return RemotePublication{ManifestURL: url, ManifestSHA256: p.ManifestSHA256, Reference: m}
}
func (r *Remote) completionName() string { return "remote-" + rawSHA([]byte(r.base)) + ".json" }
func (r *Remote) recordCompletion(directory string, p RemotePublication) error {
	raw, e := json.Marshal(p)
	if e != nil {
		return e
	}
	return immutableFile(directory, r.completionName(), raw)
}

// Catalog qualifies only the exact live state/cursor supplied by the owner.
// It neither scans a directory nor treats an old generation's marker as current.
func (r *Remote) Catalog(dir string, boot journal.Bootstrap) (RemotePublication, error) {
	fail := func(e error) (RemotePublication, error) { return RemotePublication{}, e }
	payload, e := EncodeBootstrap(boot, r.maxBytes)
	if e != nil {
		return fail(e)
	}
	m := BootstrapManifest{Schema: 2, Codec: BootstrapCodec, Reference: boot.Snapshot.Reference,
		Cursor:  ManifestCursor{Topic: boot.Cursor.Topic, Partition: boot.Cursor.Partition, NextOffset: strconv.FormatInt(boot.Cursor.Offset, 10)},
		Payload: "bootstrap.pb", SHA256: rawSHA(payload), Size: uint64(len(payload))}
	manifest, e := json.Marshal(m)
	if e != nil {
		return fail(e)
	}
	if _, e = DecodeBootstrap(manifest, payload, rawSHA(manifest), r.maxBytes); e != nil {
		return fail(e)
	}
	root, e := os.OpenRoot(dir)
	if e != nil {
		return fail(e)
	}
	defer root.Close()
	directory, e := root.OpenRoot(filepath.Join("public-v2", m.SHA256))
	if e != nil {
		return fail(e)
	}
	defer directory.Close()
	p := r.publication(Publication{ManifestSHA256: rawSHA(manifest)}, m)
	expected, e := json.Marshal(p)
	if e != nil {
		return fail(e)
	}
	for _, object := range []struct {
		name  string
		raw   []byte
		limit int64
	}{
		{"manifest.json", manifest, maxManifestBytes}, {"bootstrap.pb", payload, int64(len(payload))}, {r.completionName(), expected, maxManifestBytes},
	} {
		actual, e := readRegular(directory, object.name, object.limit)
		if e != nil {
			return fail(e)
		}
		if !bytes.Equal(actual, object.raw) {
			return fail(errors.New("CHECKPOINT_CATALOG_CONTENT_MISMATCH"))
		}
	}
	return p, nil
}
