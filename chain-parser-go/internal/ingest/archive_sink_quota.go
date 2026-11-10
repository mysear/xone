package ingest

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"syscall"
)

type LocalArchiveLimits struct {
	MaxBytes   int64
	MaxRecords int
}
type LocalArchiveStatus struct {
	Bytes   int64
	Records int
	Limits  LocalArchiveLimits
	Failed  bool
	Closed  bool
}

// BoundedLocalArchiveSink owns an exclusive quota lock, inventories actual files
// on reopen and before each write, and never deletes immutable objects to make
// space. Limits include encoded manifests and orphan/pending files. Filesystem
// allocation overhead still requires deployment headroom.
type BoundedLocalArchiveSink struct {
	mu             sync.Mutex
	sink           *LocalArchiveSink
	limits         LocalArchiveLimits
	lock           *os.File
	failed, closed bool
	usage          LocalArchiveStatus
}

func OpenBoundedLocalArchiveSink(path string, l LocalArchiveLimits) (*BoundedLocalArchiveSink, error) {
	if l.MaxBytes <= 0 || l.MaxRecords <= 0 || l.MaxRecords > 1000000 {
		return nil, ErrBudget
	}
	base, err := OpenLocalArchiveSink(path)
	if err != nil {
		return nil, err
	}
	p := filepath.Join(base.Directory(), ".quota-lock")
	if info, e := os.Lstat(p); e == nil && !info.Mode().IsRegular() {
		return nil, ErrCorrupt
	}
	lock, err := os.OpenFile(p, os.O_CREATE|os.O_RDWR, 0600)
	if err != nil {
		return nil, err
	}
	if err = syscall.Flock(int(lock.Fd()), syscall.LOCK_EX|syscall.LOCK_NB); err != nil {
		lock.Close()
		return nil, err
	}
	s := &BoundedLocalArchiveSink{sink: base, limits: l, lock: lock}
	if err = s.scan(); err != nil {
		s.Close()
		return nil, err
	}
	if err = directorySync(base.Directory()); err != nil {
		s.Close()
		return nil, err
	}
	return s, nil
}
func (s *BoundedLocalArchiveSink) Directory() string { return s.sink.Directory() }
func (s *BoundedLocalArchiveSink) scan() error {
	s.usage = LocalArchiveStatus{Limits: s.limits, Failed: s.failed, Closed: s.closed}
	d, err := os.Open(s.Directory())
	if err != nil {
		return err
	}
	defer d.Close()
	for {
		entries, e := d.ReadDir(128)
		for _, entry := range entries {
			if entry.Name() == ".quota-lock" {
				continue
			}
			info, err := entry.Info()
			if err != nil {
				return err
			}
			if !info.Mode().IsRegular() || (!strings.HasSuffix(entry.Name(), ".blob") && !strings.HasSuffix(entry.Name(), ".manifest") && !strings.HasPrefix(entry.Name(), ".pending-")) {
				return ErrCorrupt
			}
			if info.Size() < 0 || info.Size() > s.limits.MaxBytes-s.usage.Bytes || s.usage.Records >= s.limits.MaxRecords {
				return ErrBudget
			}
			s.usage.Bytes += info.Size()
			s.usage.Records++
		}
		if e != nil {
			if errors.Is(e, io.EOF) {
				break
			}
			return e
		}
		if len(entries) == 0 {
			break
		}
	}
	return nil
}
func (s *BoundedLocalArchiveSink) Store(ctx context.Context, m ArchiveManifest, data []byte) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed {
		return ErrClosed
	}
	if s.failed {
		return errors.New("INGEST_SINK_REOPEN_REQUIRED")
	}
	if ctx.Err() != nil {
		return ctx.Err()
	}
	fail := func(err error) error { s.failed = true; return err }
	if err := s.scan(); err != nil {
		return fail(err)
	}
	if !m.valid() || int64(len(data)) != m.Raw.Bytes || digest(data) != m.ContentDigest {
		return fail(ErrCorrupt)
	}
	encoded, err := json.Marshal(m)
	if err != nil {
		return fail(err)
	}
	additional := int64(0)
	records := 0
	for name, size := range map[string]int64{m.ObjectKey(): int64(len(data)), m.ManifestKey(): int64(len(encoded))} {
		if _, err := os.Lstat(filepath.Join(s.Directory(), name)); errors.Is(err, os.ErrNotExist) {
			additional += size
			records++
		} else if err != nil {
			return fail(err)
		}
	}
	if additional > s.limits.MaxBytes-s.usage.Bytes || records > s.limits.MaxRecords-s.usage.Records {
		return fail(ErrBudget)
	}
	if err := s.sink.Store(ctx, m, data); err != nil {
		return fail(err)
	}
	if err := s.scan(); err != nil {
		return fail(err)
	}
	return nil
}
func (s *BoundedLocalArchiveSink) Verify(ctx context.Context, m ArchiveManifest) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed {
		return ErrClosed
	}
	if s.failed {
		return errors.New("INGEST_SINK_REOPEN_REQUIRED")
	}
	if err := s.sink.Verify(ctx, m); err != nil {
		s.failed = true
		return err
	}
	return nil
}
func (s *BoundedLocalArchiveSink) Read(ctx context.Context, m ArchiveManifest, max int64) ([]byte, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed {
		return nil, ErrClosed
	}
	if s.failed {
		return nil, errors.New("INGEST_SINK_REOPEN_REQUIRED")
	}
	b, err := s.sink.Read(ctx, m, max)
	if err != nil {
		s.failed = true
	}
	return b, err
}
func (s *BoundedLocalArchiveSink) Status() (LocalArchiveStatus, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	err := s.scan()
	if err != nil {
		s.failed = true
	}
	s.usage.Failed = s.failed
	s.usage.Closed = s.closed
	return s.usage, err
}
func (s *BoundedLocalArchiveSink) Close() error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed {
		return nil
	}
	s.closed = true
	return errors.Join(syscall.Flock(int(s.lock.Fd()), syscall.LOCK_UN), s.lock.Close())
}
