package journal

import (
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"google.golang.org/protobuf/proto"
	"xone/chain-parser-go/internal/domain"
	pb "xone/common/protoc/golang/dexpb"
)

func EncodeUnit(unit Unit) ([]byte, error) {
	if e := unit.ValidateFacts(); e != nil {
		return nil, e
	}
	s, e := domain.SnapshotToProto(unit.Snapshot)
	if e != nil {
		return nil, e
	}
	facts, e := factsToProto(unit.NormalizedFacts)
	if e != nil {
		return nil, e
	}
	return proto.MarshalOptions{Deterministic: true}.Marshal(&pb.JournalUnit{Kind: unit.Kind, Snapshot: s, NormalizedFacts: facts})
}
func DecodeUnit(raw []byte) (Unit, error) {
	if len(raw) > 64<<20 {
		return Unit{}, errors.New("JOURNAL_UNIT_LIMIT")
	}
	var u pb.JournalUnit
	if proto.Unmarshal(raw, &u) != nil {
		return Unit{}, errors.New("JOURNAL_PROTOBUF_INVALID")
	}
	s, e := domain.SnapshotFromProto(u.Snapshot)
	if e != nil {
		return Unit{}, e
	}
	facts, e := factsFromProto(u.NormalizedFacts)
	if e != nil {
		return Unit{}, e
	}
	unit := Unit{Kind: u.Kind, Snapshot: s, NormalizedFacts: facts}
	if e = unit.ValidateFacts(); e != nil {
		return Unit{}, e
	}
	return unit, nil
}
func EncodeFrame(frame Frame) ([]byte, error) {
	r, e := domain.RefToProto(frame.Reference)
	if e != nil {
		return nil, e
	}
	hash, e := hex.DecodeString(frame.PayloadSHA256)
	if e != nil || len(hash) != 32 {
		return nil, errors.New("JOURNAL_PAYLOAD_CHECKSUM")
	}
	data, e := hex.DecodeString(frame.DataHex)
	if e != nil {
		return nil, errors.New("JOURNAL_CHUNK_HEX")
	}
	return proto.MarshalOptions{Deterministic: true}.Marshal(&pb.JournalFrame{SchemaVersion: frame.SchemaVersion, UnitId: frame.UnitID, Kind: frame.Kind, Reference: r, PayloadSha256: hash, PayloadBytes: frame.PayloadBytes, ChunkCount: frame.ChunkCount, ChunkIndex: frame.ChunkIndex, Data: data})
}
func DecodeFrame(raw []byte) (Frame, error) {
	if len(raw) > 64<<20 {
		return Frame{}, errors.New("JOURNAL_FRAME_LIMIT")
	}
	var f pb.JournalFrame
	if proto.Unmarshal(raw, &f) != nil || f.SchemaVersion != 1 || len(f.PayloadSha256) != 32 {
		return Frame{}, errors.New("JOURNAL_PROTOBUF_FRAME")
	}
	r, e := domain.RefFromProto(f.Reference)
	if e != nil {
		return Frame{}, e
	}
	return Frame{SchemaVersion: f.SchemaVersion, UnitID: f.UnitId, Kind: f.Kind, Reference: r, PayloadSHA256: hex.EncodeToString(f.PayloadSha256), PayloadBytes: f.PayloadBytes, ChunkCount: f.ChunkCount, ChunkIndex: f.ChunkIndex, DataHex: hex.EncodeToString(f.Data)}, nil
}
func EncodeBootstrap(b Bootstrap) ([]byte, error) {
	s, e := domain.SnapshotToProto(b.Snapshot)
	if e != nil {
		return nil, e
	}
	if b.Cursor.Partition != 0 || b.Cursor.Topic == "" || b.Cursor.Offset < 1 {
		return nil, errors.New("BOOTSTRAP_CURSOR_IDENTITY")
	}
	return proto.MarshalOptions{Deterministic: true}.Marshal(&pb.OwnerBootstrap{Snapshot: s, Cursor: &pb.JournalCursor{Topic: b.Cursor.Topic, Partition: b.Cursor.Partition, NextOffset: b.Cursor.Offset}})
}

// CanonicalHash binds the public deterministic protobuf, not private JSON nil
// versus empty list representations used by local prepared storage.
func (u Unit) CanonicalHash() (string, error) {
	raw, e := EncodeUnit(u)
	if e != nil {
		return "", e
	}
	h := sha256.Sum256(raw)
	return hex.EncodeToString(h[:]), nil
}
