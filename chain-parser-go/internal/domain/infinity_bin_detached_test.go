package domain

import (
	"encoding/json"
	"math/big"
	"os"
	"testing"

	"google.golang.org/protobuf/encoding/protowire"
	"google.golang.org/protobuf/proto"
	pb "xone/common/protoc/golang/dexpb"
)

func infinityBinDetachedSnapshot(t *testing.T) Snapshot {
	t.Helper()
	s, _ := infinityBinFixture(t)
	s.Pools = []Pool{s.Pools[0].Clone()}
	v := s.Pools[0].InfinityBin
	v.ActiveID++
	v.DetachedActive = &InfinityBinEntry{ID: v.ActiveID, Reserve0: "7", Reserve1: "11", TotalShares: "1000"}
	return s
}

func TestInfinityBinDetachedActiveSnapshotRoundtrip(t *testing.T) {
	s := infinityBinDetachedSnapshot(t)
	if err := s.Validate(s.Reference.OwnerService, s.Reference.OwnerAssignmentVersion); err != nil {
		t.Fatal(err)
	}
	message, err := SnapshotToProto(s)
	if err != nil {
		t.Fatal(err)
	}
	field := message.Pools[0].InfinityBin.ProtoReflect().Descriptor().Fields().ByName("detached_active")
	if field == nil || field.Number() != 29 {
		t.Fatal("detached_active must use field 29")
	}
	wire, err := proto.Marshal(message)
	if err != nil {
		t.Fatal(err)
	}
	decoded := new(pb.OnlineSnapshot)
	if err = proto.Unmarshal(wire, decoded); err != nil {
		t.Fatal(err)
	}
	got, err := SnapshotFromProto(decoded)
	if err != nil {
		t.Fatal(err)
	}
	a, err := Hash(s)
	if err != nil {
		t.Fatal(err)
	}
	b, err := Hash(got)
	if err != nil || a != b {
		t.Fatal("detached active snapshot lost", err)
	}
	if id, err := got.Pools[0].InfinityBin.KeyID(got.Pools[0].Token0, got.Pools[0].Token1); err != nil || id != s.Pools[0].InfinityBin.PoolID {
		t.Fatal("detached state changes pool key", err)
	}
	for _, mutate := range []func(*Snapshot){
		func(s *Snapshot) { s.Pools[0].InfinityBin.Timestamp++ },
		func(s *Snapshot) { s.Pools[0].Token1 = "0x0000000000000000000000000000000000000003" },
	} {
		raw, err := json.Marshal(s)
		if err != nil {
			t.Fatal(err)
		}
		var bad Snapshot
		if err = json.Unmarshal(raw, &bad); err != nil {
			t.Fatal(err)
		}
		mutate(&bad)
		if bad.Validate(s.Reference.OwnerService, s.Reference.OwnerAssignmentVersion) == nil {
			t.Fatal("detached snapshot lost time/key binding")
		}
	}
}

// Assemble field 29 independently of the generated InfinityBinState encoder.
// This locks the cross-language tag and the entry/Amount byte representation.
func TestInfinityBinDetachedActiveIndependentWire(t *testing.T) {
	_, fixture := infinityBinFixture(t)
	message := proto.Clone(fixture.Pools[0].InfinityBin).(*pb.InfinityBinState)
	message.ActiveId++
	wire, err := proto.Marshal(message)
	if err != nil {
		t.Fatal(err)
	}
	entry := protowire.AppendVarint(protowire.AppendTag(nil, 1, protowire.VarintType), uint64(message.ActiveId))
	for i, n := range []uint64{7, 11, 1000} {
		amount := new(big.Int).SetUint64(n).FillBytes(make([]byte, 32))
		amountMessage := protowire.AppendBytes(protowire.AppendTag(nil, 1, protowire.BytesType), amount)
		entry = protowire.AppendBytes(protowire.AppendTag(entry, protowire.Number(i+2), protowire.BytesType), amountMessage)
	}
	wire = protowire.AppendBytes(protowire.AppendTag(wire, 29, protowire.BytesType), entry)
	message.Reset()
	if err = proto.Unmarshal(wire, message); err != nil {
		t.Fatal(err)
	}
	s, err := InfinityBinFromProto(message)
	if err != nil {
		t.Fatal(err)
	}
	if s.DetachedActive == nil || *s.DetachedActive != (InfinityBinEntry{ID: message.ActiveId, Reserve0: "7", Reserve1: "11", TotalShares: "1000"}) {
		t.Fatal("field 29 lost or misinterpreted")
	}
	encoded, err := InfinityBinToProto(s)
	if err != nil || !proto.Equal(encoded, message) {
		t.Fatal("independent detached encoding differs", err)
	}
}

func TestInfinityBinDetachedActiveDeepClone(t *testing.T) {
	s := infinityBinDetachedSnapshot(t)
	original := s.Pools[0]
	cloned := original.InfinityBin.Clone()
	cloned.DetachedActive.Reserve0 = "13"
	cloned.TreeLevel1[0].Value = "0"
	cloned.TreeLevel2[0].Value = "0"
	cloned.Bins[0].Reserve0 = "0"
	if original.InfinityBin.DetachedActive.Reserve0 != "7" || original.InfinityBin.TreeLevel1[0].Value == "0" || original.InfinityBin.TreeLevel2[0].Value == "0" || original.InfinityBin.Bins[0].Reserve0 == "0" {
		t.Fatal("state clone aliases detached active or tree")
	}
	poolClone := original.Clone()
	poolClone.InfinityBin.DetachedActive.Reserve1 = "17"
	if original.InfinityBin.DetachedActive.Reserve1 != "11" {
		t.Fatal("Pool.Clone aliases detached active")
	}
}

func TestInfinityBinDetachedActiveEmptyTree(t *testing.T) {
	s := infinityBinDetachedSnapshot(t)
	v := s.Pools[0].InfinityBin
	v.Bins = []InfinityBinEntry{}
	v.TreeLevel0 = "0"
	v.TreeLevel1 = []InfinityBinTreeWord{}
	v.TreeLevel2 = []InfinityBinTreeWord{}
	v.MaxBins = 1
	v.MaxTreeWords = 1
	maxReserve := Amount(new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), 128), big.NewInt(1)).String())
	v.DetachedActive.Reserve0 = maxReserve
	v.DetachedActive.Reserve1 = maxReserve
	for _, id := range []uint32{1, 0xfffffe} {
		v.ActiveID = id
		v.DetachedActive.ID = id
		if err := s.Validate(s.Reference.OwnerService, s.Reference.OwnerAssignmentVersion); err != nil {
			t.Fatal("valid zero-tree ghost", err)
		}
		message, err := SnapshotToProto(s)
		if err != nil {
			t.Fatal(err)
		}
		got, err := SnapshotFromProto(message)
		if err != nil || got.Pools[0].InfinityBin.DetachedActive == nil {
			t.Fatal("zero-tree ghost wire", err)
		}
	}
}

func TestInfinityBinDetachedActiveRejectsInvalidState(t *testing.T) {
	original := infinityBinDetachedSnapshot(t)
	for name, mutate := range map[string]func(*InfinityBinState){
		"wrong id":            func(s *InfinityBinState) { s.DetachedActive.ID++ },
		"zero id":             func(s *InfinityBinState) { s.ActiveID = 0; s.DetachedActive.ID = 0 },
		"sentinel id":         func(s *InfinityBinState) { s.ActiveID = 0xffffff; s.DetachedActive.ID = 0xffffff },
		"duplicate in tree":   func(s *InfinityBinState) { s.ActiveID = s.Bins[0].ID; s.DetachedActive.ID = s.ActiveID },
		"shares zero":         func(s *InfinityBinState) { s.DetachedActive.TotalShares = "0" },
		"shares 999":          func(s *InfinityBinState) { s.DetachedActive.TotalShares = "999" },
		"shares 1001":         func(s *InfinityBinState) { s.DetachedActive.TotalShares = "1001" },
		"shares noncanonical": func(s *InfinityBinState) { s.DetachedActive.TotalShares = "01000" },
		"reserve0 overflow": func(s *InfinityBinState) {
			s.DetachedActive.Reserve0 = Amount(new(big.Int).Lsh(big.NewInt(1), 128).String())
		},
		"reserve1 overflow": func(s *InfinityBinState) {
			s.DetachedActive.Reserve1 = Amount(new(big.Int).Lsh(big.NewInt(1), 128).String())
		},
		"reserve invalid":        func(s *InfinityBinState) { s.DetachedActive.Reserve1 = "-1" },
		"actual count":           func(s *InfinityBinState) { s.MaxBins = uint32(len(s.Bins)) },
		"root includes detached": func(s *InfinityBinState) { s.TreeLevel2[1].Value = "3" },
	} {
		t.Run(name, func(t *testing.T) {
			raw, err := json.Marshal(original)
			if err != nil {
				t.Fatal(err)
			}
			var s Snapshot
			if err = json.Unmarshal(raw, &s); err != nil {
				t.Fatal(err)
			}
			mutate(s.Pools[0].InfinityBin)
			if s.Pools[0].Validate() == nil {
				t.Fatal("invalid detached active admitted")
			}
			if _, err = InfinityBinToProto(s.Pools[0].InfinityBin); err == nil {
				t.Fatal("invalid detached active encoded")
			}
		})
	}
	v := original.Pools[0].InfinityBin
	v.MaxBins = uint32(len(v.Bins) + 1)
	if err := original.Pools[0].Validate(); err != nil {
		t.Fatal("inclusive actual bin budget", err)
	}
}

func TestInfinityBinDetachedActiveRejectsMalformedWire(t *testing.T) {
	original := infinityBinDetachedSnapshot(t)
	message, err := SnapshotToProto(original)
	if err != nil {
		t.Fatal(err)
	}
	for name, mutate := range map[string]func(*pb.InfinityBinEntry){
		"reserve missing": func(v *pb.InfinityBinEntry) { v.Reserve0 = nil },
		"reserve width":   func(v *pb.InfinityBinEntry) { v.Reserve1.Uint256Be = v.Reserve1.Uint256Be[:31] },
		"shares missing":  func(v *pb.InfinityBinEntry) { v.TotalShares = nil },
		"wrong shares":    func(v *pb.InfinityBinEntry) { v.TotalShares.Uint256Be = make([]byte, 32) },
		"wrong id":        func(v *pb.InfinityBinEntry) { v.Id++ },
	} {
		t.Run(name, func(t *testing.T) {
			bad := proto.Clone(message).(*pb.OnlineSnapshot)
			mutate(bad.Pools[0].InfinityBin.DetachedActive)
			if _, err := SnapshotFromProto(bad); err == nil {
				t.Fatal("malformed detached active wire admitted")
			}
		})
	}
}

func TestOriginalBurnDetachedFixtureMatchesRealTreeDomain(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/infinity-bin-detached.json")
	if e != nil {
		t.Fatal(e)
	}
	var f struct {
		Cases []struct {
			Burn struct {
				Expected struct {
					Post struct {
						Detached struct {
							ID          uint32 `json:"id"`
							Reserve0    Amount `json:"reserve0"`
							Reserve1    Amount `json:"reserve1"`
							TotalShares Amount `json:"total_shares"`
							InTree      bool   `json:"in_tree"`
						} `json:"detached_active"`
					} `json:"post"`
				} `json:"expected"`
			} `json:"burn"`
		} `json:"cases"`
	}
	if e = json.Unmarshal(raw, &f); e != nil {
		t.Fatal(e)
	}
	if len(f.Cases) != 16 {
		t.Fatal("native burn inventory missing")
	}
	for _, c := range f.Cases {
		d := c.Burn.Expected.Post.Detached
		if d.InTree || d.TotalShares != "1000" || d.ID == 0 {
			t.Fatal("native burn must remove tree member and preserve minimum shares", d)
		}
	}
}
