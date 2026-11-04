// Package journal defines transactional, complete publication units.
package journal

import (
	"crypto/sha256"
	"encoding/hex"

	"errors"
	"fmt"
	"xone/chain-parser-go/internal/domain"
)

type Cursor struct {
	Topic     string `json:"topic"`
	Partition int32  `json:"partition"`
	Offset    int64  `json:"offset"`
}
type Unit struct {
	Kind            string            `json:"kind"`
	Snapshot        domain.Snapshot   `json:"snapshot"`
	NormalizedFacts *domain.FactBatch `json:"normalized_facts,omitempty"`
}
type Bootstrap struct {
	Snapshot domain.Snapshot `json:"snapshot"`
	Cursor   Cursor          `json:"cursor"`
}
type Frame struct {
	SchemaVersion uint32             `json:"schema_version"`
	UnitID        string             `json:"unit_id"`
	Kind          string             `json:"kind"`
	Reference     domain.SnapshotRef `json:"reference"`
	PayloadSHA256 string             `json:"payload_sha256"`
	PayloadBytes  uint64             `json:"payload_bytes"`
	ChunkCount    uint32             `json:"chunk_count"`
	ChunkIndex    uint32             `json:"chunk_index"`
	DataHex       string             `json:"data_hex"`
}

func Encode(unit Unit, chunkBytes int) ([]Frame, error) {
	if chunkBytes < 1024 || chunkBytes > 1048576 {
		return nil, errors.New("JOURNAL_CHUNK_LIMIT")
	}
	if e := unit.Snapshot.Validate(unit.Snapshot.Reference.OwnerService, unit.Snapshot.Reference.OwnerAssignmentVersion); e != nil {
		return nil, e
	}
	if unit.Kind != "ONLINE_BASELINE_COMMIT" && unit.Kind != "BLOCK_COMMIT" && unit.Kind != "REORG_COMMIT" && unit.Kind != "REORG_BEGIN" && unit.Kind != "ONLINE_SCOPE_REVOKED" {
		return nil, errors.New("JOURNAL_UNIT_KIND")
	}
	b, e := EncodeUnit(unit)
	if e != nil {
		return nil, e
	}
	if len(b) > 64<<20 {
		return nil, errors.New("JOURNAL_UNIT_LIMIT")
	}
	h := sha256.Sum256(b)
	r := unit.Snapshot.Reference
	id := fmt.Sprintf("%s/%d/%d/%d", r.Block.ChainKey, r.OwnerAssignmentVersion, r.WriterEpoch, r.JournalSeq)
	count := (len(b) + chunkBytes - 1) / chunkBytes
	base := Frame{SchemaVersion: 1, UnitID: id, Reference: r, PayloadSHA256: hex.EncodeToString(h[:]), PayloadBytes: uint64(len(b)), ChunkCount: uint32(count)}
	frames := make([]Frame, 0, count+2)
	begin := base
	begin.Kind = "BEGIN"
	frames = append(frames, begin)
	for i := 0; i < count; i++ {
		f := base
		f.Kind = "CHUNK"
		f.ChunkIndex = uint32(i)
		end := min(len(b), (i+1)*chunkBytes)
		f.DataHex = hex.EncodeToString(b[i*chunkBytes : end])
		frames = append(frames, f)
	}
	commit := base
	commit.Kind = "COMMIT"
	frames = append(frames, commit)
	return frames, nil
}

type Assembler struct {
	max   int
	begin *Frame
	data  []byte
	next  uint32
}

func NewAssembler(maxBytes int) *Assembler { return &Assembler{max: maxBytes} }
func (a *Assembler) Reset()                { a.begin = nil; a.data = nil; a.next = 0 }
func (a *Assembler) Push(f Frame) (unit *Unit, result error) {
	defer func() {
		if result != nil {
			a.Reset()
		}
	}()
	if f.SchemaVersion != 1 || f.PayloadBytes == 0 || f.PayloadBytes > uint64(a.max) || f.ChunkCount == 0 || f.ChunkCount > 65536 {
		return nil, errors.New("JOURNAL_FRAME_INVALID")
	}
	if f.Kind == "BEGIN" {
		if a.begin != nil || f.DataHex != "" || f.ChunkIndex != 0 {
			return nil, errors.New("JOURNAL_INTERLEAVED_BEGIN")
		}
		a.begin = &f
		a.data = make([]byte, 0, min(int(f.PayloadBytes), 1<<20))
		return nil, nil
	}
	if a.begin == nil {
		return nil, errors.New("JOURNAL_BEGIN_REQUIRED")
	}
	b := a.begin
	if f.UnitID != b.UnitID || f.Reference != b.Reference || f.PayloadSHA256 != b.PayloadSHA256 || f.PayloadBytes != b.PayloadBytes || f.ChunkCount != b.ChunkCount {
		return nil, errors.New("JOURNAL_FRAME_IDENTITY")
	}
	switch f.Kind {
	case "CHUNK":
		if f.ChunkIndex != a.next || a.next >= f.ChunkCount {
			return nil, errors.New("JOURNAL_CHUNK_SEQUENCE")
		}
		raw, e := hex.DecodeString(f.DataHex)
		if e != nil || len(raw) == 0 || len(a.data)+len(raw) > int(f.PayloadBytes) {
			return nil, errors.New("JOURNAL_CHUNK_SIZE")
		}
		a.data = append(a.data, raw...)
		a.next++
		return nil, nil
	case "COMMIT":
		if a.next != f.ChunkCount || len(a.data) != int(f.PayloadBytes) || f.DataHex != "" {
			return nil, errors.New("JOURNAL_INCOMPLETE_COMMIT")
		}
		h := sha256.Sum256(a.data)
		if hex.EncodeToString(h[:]) != f.PayloadSHA256 {
			return nil, errors.New("JOURNAL_PAYLOAD_CHECKSUM")
		}
		u, e := DecodeUnit(a.data)
		if e != nil || u.Snapshot.Reference != f.Reference {
			return nil, errors.New("JOURNAL_PAYLOAD_IDENTITY")
		}
		if e := u.Snapshot.Validate(f.Reference.OwnerService, f.Reference.OwnerAssignmentVersion); e != nil {
			return nil, e
		}
		if u.Kind != "ONLINE_BASELINE_COMMIT" && u.Kind != "BLOCK_COMMIT" && u.Kind != "REORG_COMMIT" && u.Kind != "REORG_BEGIN" && u.Kind != "ONLINE_SCOPE_REVOKED" {
			return nil, errors.New("JOURNAL_UNIT_KIND")
		}
		a.Reset()
		return &u, nil
	default:
		return nil, errors.New("JOURNAL_FRAME_KIND")
	}
}
