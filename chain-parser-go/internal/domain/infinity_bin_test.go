package domain

import (
	"encoding/hex"
	"encoding/json"
	"google.golang.org/protobuf/proto"
	"math/big"
	"os"
	"strings"
	"testing"
	pb "xone/common/protoc/golang/dexpb"
)

func infinityBinFixture(t *testing.T) (Snapshot, *pb.OnlineSnapshot) {
	t.Helper()
	raw, err := os.ReadFile("../../test/contracts/infinity-bin-wire.json")
	if err != nil {
		t.Fatal(err)
	}
	var fixture struct {
		Snapshot    Snapshot `json:"snapshot"`
		ProtobufHex string   `json:"protobuf_hex"`
	}
	if err = json.Unmarshal(raw, &fixture); err != nil {
		t.Fatal(err)
	}
	wire, err := hex.DecodeString(fixture.ProtobufHex)
	if err != nil {
		t.Fatal(err)
	}
	message := new(pb.OnlineSnapshot)
	if err = proto.Unmarshal(wire, message); err != nil {
		t.Fatal(err)
	}
	return fixture.Snapshot, message
}
func TestInfinityBinIndependentSnapshotWireRoundtrip(t *testing.T) {
	s, message := infinityBinFixture(t)
	if err := s.Validate(s.Reference.OwnerService, s.Reference.OwnerAssignmentVersion); err != nil {
		t.Fatal(err)
	}
	got, err := SnapshotFromProto(message)
	if err != nil {
		t.Fatal(err)
	}
	a, err := Hash(s)
	if err != nil {
		t.Fatal(err)
	}
	b, err := Hash(got)
	if err != nil || a != b {
		t.Fatal("full snapshot lost", a, b, err)
	}
	encoded, err := SnapshotToProto(got)
	if err != nil || !proto.Equal(message, encoded) {
		t.Fatal("independent protobuf differs", err)
	}
	p := s.Pools[0]
	id, err := p.InfinityBin.KeyID(p.Token0, p.Token1)
	if err != nil || id != p.InfinityBin.PoolID {
		t.Fatal("key identity", id, err)
	}
}
func TestInfinityBinCloneAndSourceBank(t *testing.T) {
	s, _ := infinityBinFixture(t)
	p := s.Pools[0]
	q := p.Clone()
	q.InfinityBin.TreeLevel1[0].Value = "0"
	q.InfinityBin.TreeLevel2[0].Value = "0"
	q.InfinityBin.Bins[0].Reserve0 = "0"
	if p.InfinityBin.TreeLevel1[0].Value == "0" || p.InfinityBin.TreeLevel2[0].Value == "0" || p.InfinityBin.Bins[0].Reserve0 == "0" {
		t.Fatal("clone aliases source")
	}
	for _, mutate := range []func(*InfinityBinState){func(v *InfinityBinState) { v.BlockNumber++ }, func(v *InfinityBinState) { v.Timestamp++ }, func(v *InfinityBinState) { v.BlockHash = "0x" + strings.Repeat("b", 64) }} {
		bad := s
		bad.Pools = []Pool{p.Clone()}
		mutate(bad.Pools[0].InfinityBin)
		if bad.Validate(s.Reference.OwnerService, s.Reference.OwnerAssignmentVersion) == nil {
			t.Fatal("another source bank admitted")
		}
	}
}
func TestInfinityBinInvalidTreeAndState(t *testing.T) {
	s, _ := infinityBinFixture(t)
	original := s.Pools[0]
	for name, mutate := range map[string]func(*Pool){
		"missing": func(p *Pool) { p.InfinityBin = nil }, "legacy": func(p *Pool) { p.Protocol = "uniswap-v2" }, "ambiguous": func(p *Pool) { p.V2 = &V2State{} }, "adapter": func(p *Pool) { p.AdapterVersion = "bad" }, "chain": func(p *Pool) { p.ChainKey = "solana:test" }, "chain invalid": func(p *Pool) { p.ChainKey = "evm:0" }, "behavior": func(p *Pool) { p.TokenBehavior = "fee-on-transfer" }, "fee": func(p *Pool) { p.FeeNumerator++ }, "denom": func(p *Pool) { p.FeeDenominator++ }, "reserve": func(p *Pool) { p.Reserve0 = "1" }, "physical": func(p *Pool) { p.PoolID = p.InfinityBin.PoolID },
		"root": func(p *Pool) { p.InfinityBin.TreeLevel0 = "0" }, "missing l1": func(p *Pool) { p.InfinityBin.TreeLevel1 = p.InfinityBin.TreeLevel1[:1] }, "missing l2": func(p *Pool) { p.InfinityBin.TreeLevel2 = p.InfinityBin.TreeLevel2[:1] }, "zero word": func(p *Pool) { p.InfinityBin.TreeLevel2[0].Value = "0" }, "l1 index": func(p *Pool) { p.InfinityBin.TreeLevel1[0].Index = 256 }, "l2 index": func(p *Pool) { p.InfinityBin.TreeLevel2[0].Index = 65536 }, "l1 duplicate": func(p *Pool) { p.InfinityBin.TreeLevel1[1] = p.InfinityBin.TreeLevel1[0] }, "l2 duplicate": func(p *Pool) { p.InfinityBin.TreeLevel2[1] = p.InfinityBin.TreeLevel2[0] }, "extra bit": func(p *Pool) { p.InfinityBin.TreeLevel2[0].Value = "3" },
		"bin order": func(p *Pool) { p.InfinityBin.Bins[1] = p.InfinityBin.Bins[0] }, "zero id": func(p *Pool) { p.InfinityBin.Bins[0].ID = 0 }, "sentinel id": func(p *Pool) { p.InfinityBin.Bins[2].ID = 0xffffff }, "active zero": func(p *Pool) { p.InfinityBin.ActiveID = 0 }, "active sentinel": func(p *Pool) { p.InfinityBin.ActiveID = 0xffffff }, "shares zero": func(p *Pool) { p.InfinityBin.Bins[0].TotalShares = "0" }, "reserve width": func(p *Pool) { p.InfinityBin.Bins[0].Reserve0 = Amount(new(big.Int).Lsh(big.NewInt(1), 128).String()) }, "shares width": func(p *Pool) {
			p.InfinityBin.Bins[0].TotalShares = Amount(new(big.Int).Lsh(big.NewInt(1), 256).String())
		},
		"bins zero": func(p *Pool) { p.InfinityBin.MaxBins = 0 }, "bins limit": func(p *Pool) { p.InfinityBin.MaxBins = 1025 }, "bins budget": func(p *Pool) { p.InfinityBin.MaxBins = 2 }, "words zero": func(p *Pool) { p.InfinityBin.MaxTreeWords = 0 }, "words limit": func(p *Pool) { p.InfinityBin.MaxTreeWords = 1025 }, "words root counted": func(p *Pool) { p.InfinityBin.MaxTreeWords = 5 }, "steps zero": func(p *Pool) { p.InfinityBin.MaxSteps = 0 }, "steps limit": func(p *Pool) { p.InfinityBin.MaxSteps = 4097 }, "step zero": func(p *Pool) { p.InfinityBin.BinStep = 0 }, "step limit": func(p *Pool) { p.InfinityBin.BinStep = 101 }, "lp limit": func(p *Pool) { p.InfinityBin.LPFee = 100001; p.FeeNumerator = 100001 }, "protocol fee0": func(p *Pool) { p.InfinityBin.ProtocolFee0 = 4001 }, "protocol fee1": func(p *Pool) { p.InfinityBin.ProtocolFee1 = 4001 },
		"hook": func(p *Pool) { p.InfinityBin.Hooks = p.Token0 }, "parameters": func(p *Pool) { p.InfinityBin.Parameters = "0x" + strings.Repeat("0", 64) }, "key": func(p *Pool) {
			p.InfinityBin.PoolID = "0x" + strings.Repeat("a", 64)
			p.PoolID = p.InfinityBin.Manager + "/" + p.InfinityBin.PoolID
		}, "native token": func(p *Pool) { p.Token0 = SingletonZeroAddress }, "tokens order": func(p *Pool) { p.Token0, p.Token1 = p.Token1, p.Token0 }, "manager asset": func(p *Pool) { p.InfinityBin.Manager = p.Token0 }, "vault asset": func(p *Pool) { p.InfinityBin.Vault = p.Token1 }, "manager zero": func(p *Pool) { p.InfinityBin.Manager = SingletonZeroAddress }, "vault zero": func(p *Pool) { p.InfinityBin.Vault = SingletonZeroAddress }, "code missing": func(p *Pool) { p.InfinityBin.Token0CodeSHA256 = "" }, "code uppercase": func(p *Pool) { p.InfinityBin.Token1CodeSHA256 = strings.Repeat("A", 64) }, "timestamp zero": func(p *Pool) { p.InfinityBin.Timestamp = 0 }, "timestamp overflow": func(p *Pool) { p.InfinityBin.Timestamp = ^uint64(0) }, "hash zero": func(p *Pool) { p.InfinityBin.BlockHash = "0x" + strings.Repeat("0", 64) }, "hash uppercase": func(p *Pool) { p.InfinityBin.BlockHash = "0x" + strings.Repeat("A", 64) }, "global invalid": func(p *Pool) { p.InfinityBin.AppReserve0 = "-1" },
	} {
		t.Run(name, func(t *testing.T) {
			p := original.Clone()
			mutate(&p)
			if p.Validate() == nil {
				t.Fatal("invalid infinity bin state admitted")
			}
		})
	}
}
func TestInfinityBinExhaustedBinsAndAbsentActive(t *testing.T) {
	s, _ := infinityBinFixture(t)
	p := s.Pools[0].Clone()
	p.InfinityBin.ActiveID++
	p.InfinityBin.Bins[0].Reserve0 = "0"
	p.InfinityBin.Bins[0].Reserve1 = "0"
	if err := p.Validate(); err != nil {
		t.Fatal(err)
	}
	p.InfinityBin.MaxBins = 1024
	p.InfinityBin.MaxTreeWords = 1024
	p.InfinityBin.MaxSteps = 4096
	if err := p.Validate(); err != nil {
		t.Fatal(err)
	}
}
func TestInfinityBinWireRejectsMalformed(t *testing.T) {
	_, original := infinityBinFixture(t)
	for name, mutate := range map[string]func(*pb.InfinityBinState){
		"manager code": func(s *pb.InfinityBinState) { s.ManagerCodeSha256 = nil }, "vault code": func(s *pb.InfinityBinState) { s.VaultCodeSha256 = s.VaultCodeSha256[:31] }, "token code": func(s *pb.InfinityBinState) { s.Token0CodeSha256 = nil }, "key": func(s *pb.InfinityBinState) { s.PoolId = nil }, "parameters": func(s *pb.InfinityBinState) { s.Parameters = nil }, "hash": func(s *pb.InfinityBinState) { s.BlockHash = nil }, "root": func(s *pb.InfinityBinState) { s.TreeLevel0 = nil }, "amount width": func(s *pb.InfinityBinState) { s.AppReserve0.Uint256Be = []byte{1} }, "word nil": func(s *pb.InfinityBinState) { s.TreeLevel1[0] = nil }, "bin nil": func(s *pb.InfinityBinState) { s.Bins[0] = nil }, "shares nil": func(s *pb.InfinityBinState) { s.Bins[0].TotalShares = nil }, "tree invalid": func(s *pb.InfinityBinState) { s.TreeLevel2[0].Value = &pb.Amount{Uint256Be: make([]byte, 32)} },
	} {
		t.Run(name, func(t *testing.T) {
			message := proto.Clone(original).(*pb.OnlineSnapshot)
			mutate(message.Pools[0].InfinityBin)
			if _, err := SnapshotFromProto(message); err == nil {
				t.Fatal("malformed wire admitted")
			}
		})
	}
}

func TestInfinityBinRejectsEmptyInventoryAndManagerVaultAlias(t *testing.T) {
	s, _ := infinityBinFixture(t)
	for _, mutate := range []func(*InfinityBinState){
		func(v *InfinityBinState) { v.Vault = v.Manager },
		func(v *InfinityBinState) { v.Bins = nil; v.TreeLevel0 = "0"; v.TreeLevel1 = nil; v.TreeLevel2 = nil },
	} {
		p := s.Pools[0].Clone()
		mutate(p.InfinityBin)
		if p.Validate() == nil {
			t.Error("empty inventory or manager/vault alias admitted")
		}
	}
}
