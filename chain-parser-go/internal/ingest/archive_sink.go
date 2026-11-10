package ingest

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"io"
	"os"
	"path/filepath"
	"sync"
)

// LocalArchiveSink copies/fsyncs actual public data. A separate mounted failure
// domain is required for disaster recovery. It provides no WORM, encryption or
// remote replication. Root/ancestors must be operator-controlled. Ancestor
// symlinks (including macOS /var) are resolved once; object symlinks are rejected.
type LocalArchiveSink struct {
	mu        sync.Mutex
	directory string
	failed    bool
	syncDir   func(string) error
}

func (s *LocalArchiveSink) Directory() string { return s.directory }
func OpenLocalArchiveSink(path string) (*LocalArchiveSink, error) {
	absolute, err := filepath.Abs(path)
	if err != nil {
		return nil, err
	}
	// Resolve the nearest existing ancestor before creating child directories.
	var suffix []string
	ancestor := absolute
	for {
		info, statErr := os.Lstat(ancestor)
		if statErr == nil {
			if ancestor == absolute && !info.IsDir() {
				return nil, ErrCorrupt
			}
			resolved, resolveErr := filepath.EvalSymlinks(ancestor)
			if resolveErr != nil {
				return nil, resolveErr
			}
			absolute = resolved
			for i := len(suffix) - 1; i >= 0; i-- {
				absolute = filepath.Join(absolute, suffix[i])
			}
			break
		}
		if !errors.Is(statErr, os.ErrNotExist) {
			return nil, statErr
		}
		suffix = append(suffix, filepath.Base(ancestor))
		ancestor = filepath.Dir(ancestor)
	}
	var missing []string
	for current := absolute; ; current = filepath.Dir(current) {
		if _, statErr := os.Stat(current); statErr == nil {
			break
		} else if !errors.Is(statErr, os.ErrNotExist) {
			return nil, statErr
		}
		missing = append(missing, current)
	}
	if err := os.MkdirAll(absolute, 0700); err != nil {
		return nil, err
	}
	for i := len(missing) - 1; i >= 0; i-- {
		if err := directorySync(filepath.Dir(missing[i])); err != nil {
			return nil, err
		}
	}
	if err := directorySync(absolute); err != nil {
		return nil, err
	}
	return &LocalArchiveSink{directory: absolute, syncDir: directorySync}, nil
}
func (s *LocalArchiveSink) check(ctx context.Context, m ArchiveManifest) error {
	if ctx.Err() != nil {
		return ctx.Err()
	}
	if s.failed {
		return errors.New("INGEST_SINK_REOPEN_REQUIRED")
	}
	if !m.valid() {
		return ErrCorrupt
	}
	return nil
}
func openRegular(path string) (*os.File, error) {
	info, err := os.Lstat(path)
	if err != nil {
		return nil, err
	}
	if !info.Mode().IsRegular() {
		return nil, ErrCorrupt
	}
	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	opened, err := f.Stat()
	if err != nil {
		f.Close()
		return nil, err
	}
	if !os.SameFile(info, opened) {
		f.Close()
		return nil, ErrCorrupt
	}
	return f, nil
}

// link(2) cannot overwrite an existing object. Collisions compare exact bytes.
func (s *LocalArchiveSink) immutableCopy(ctx context.Context, name string, data []byte) error {
	path := filepath.Join(s.directory, name)
	if _, err := os.Lstat(path); err == nil {
		f, err := openRegular(path)
		if err != nil {
			return err
		}
		defer f.Close()
		buffer := make([]byte, 64<<10)
		for offset := 0; offset < len(data); {
			if ctx.Err() != nil {
				return ctx.Err()
			}
			count := len(data) - offset
			if count > len(buffer) {
				count = len(buffer)
			}
			if _, err := io.ReadFull(f, buffer[:count]); err != nil {
				return errors.Join(ErrCorrupt, err)
			}
			if !bytes.Equal(buffer[:count], data[offset:offset+count]) {
				return ErrCorrupt
			}
			offset += count
		}
		var extra [1]byte
		if n, err := f.Read(extra[:]); n != 0 || err != io.EOF {
			return ErrCorrupt
		}
		if err := f.Sync(); err != nil {
			return err
		}
		if err := s.syncDir(s.directory); err != nil {
			return err
		}
		return ctx.Err()
	} else if !errors.Is(err, os.ErrNotExist) {
		return err
	}
	f, err := os.CreateTemp(s.directory, ".pending-")
	if err != nil {
		return err
	}
	temp := f.Name()
	defer os.Remove(temp)
	for offset := 0; offset < len(data); {
		if ctx.Err() != nil {
			f.Close()
			return ctx.Err()
		}
		end := offset + (64 << 10)
		if end > len(data) {
			end = len(data)
		}
		n, err := f.Write(data[offset:end])
		if err != nil {
			f.Close()
			return err
		}
		if n != end-offset {
			f.Close()
			return io.ErrShortWrite
		}
		offset = end
	}
	if err := f.Sync(); err != nil {
		f.Close()
		return err
	}
	if err := f.Close(); err != nil {
		return err
	}
	if err := os.Link(temp, path); err != nil {
		if errors.Is(err, os.ErrExist) {
			return s.immutableCopy(ctx, name, data)
		}
		return err
	}
	if err := os.Remove(temp); err != nil {
		return err
	}
	return s.syncDir(s.directory)
}
func (s *LocalArchiveSink) Store(ctx context.Context, m ArchiveManifest, data []byte) (err error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if err = s.check(ctx, m); err != nil {
		return err
	}
	if int64(len(data)) != m.Raw.Bytes || digest(data) != m.ContentDigest {
		return ErrCorrupt
	}
	defer func() {
		if err != nil {
			s.failed = true
		}
	}()
	if err = s.immutableCopy(ctx, m.ObjectKey(), data); err != nil {
		return err
	}
	encoded, err := json.Marshal(m)
	if err != nil {
		return err
	}
	if err = s.immutableCopy(ctx, m.ManifestKey(), encoded); err != nil {
		return err
	}
	return ctx.Err()
}
func (s *LocalArchiveSink) Verify(ctx context.Context, m ArchiveManifest) (err error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if err = s.check(ctx, m); err != nil {
		return err
	}
	defer func() {
		if err != nil {
			s.failed = true
		}
	}()
	f, err := openRegular(filepath.Join(s.directory, m.ManifestKey()))
	if err != nil {
		return err
	}
	expected, err := json.Marshal(m)
	if err != nil {
		f.Close()
		return err
	}
	actual, err := io.ReadAll(io.LimitReader(f, int64(len(expected))+1))
	closeErr := f.Close()
	if err != nil || closeErr != nil {
		return errors.Join(err, closeErr)
	}
	if !bytes.Equal(actual, expected) {
		return ErrCorrupt
	}
	f, err = openRegular(filepath.Join(s.directory, m.ObjectKey()))
	if err != nil {
		return err
	}
	defer f.Close()
	info, err := f.Stat()
	if err != nil {
		return err
	}
	if info.Size() != m.Raw.Bytes {
		return ErrCorrupt
	}
	hash := sha256.New()
	buf := make([]byte, 64<<10)
	var total int64
	for {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		n, readErr := f.Read(buf)
		if n > 0 {
			total += int64(n)
			if total > m.Raw.Bytes {
				return ErrCorrupt
			}
			hash.Write(buf[:n])
		}
		if readErr == io.EOF {
			break
		}
		if readErr != nil {
			return readErr
		}
	}
	if total != m.Raw.Bytes || hex.EncodeToString(hash.Sum(nil)) != m.ContentDigest {
		return ErrCorrupt
	}
	return ctx.Err()
}

// Read returns verified external raw under an explicit allocation bound. It
// rechecks the bytes read after manifest/content verification, so replacing an
// object between Verify and Read cannot return unchecked content.
func (s *LocalArchiveSink) Read(ctx context.Context, m ArchiveManifest, maxBytes int64) ([]byte, error) {
	if maxBytes <= 0 || m.Raw.Bytes > maxBytes {
		return nil, ErrBudget
	}
	if err := s.Verify(ctx, m); err != nil {
		return nil, err
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if err := s.check(ctx, m); err != nil {
		return nil, err
	}
	f, err := openRegular(filepath.Join(s.directory, m.ObjectKey()))
	if err != nil {
		s.failed = true
		return nil, err
	}
	defer f.Close()
	var data []byte
	buffer := make([]byte, 64<<10)
	for {
		if ctx.Err() != nil {
			return nil, ctx.Err()
		}
		n, readErr := f.Read(buffer)
		if n > 0 {
			if int64(len(data))+int64(n) > m.Raw.Bytes {
				s.failed = true
				return nil, ErrCorrupt
			}
			data = append(data, buffer[:n]...)
		}
		if readErr == io.EOF {
			break
		}
		if readErr != nil {
			s.failed = true
			return nil, readErr
		}
	}
	if int64(len(data)) != m.Raw.Bytes || digest(data) != m.ContentDigest {
		s.failed = true
		return nil, ErrCorrupt
	}
	return data, ctx.Err()
}
