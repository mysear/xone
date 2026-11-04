package domain

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"google.golang.org/protobuf/proto"
	"math/big"
	"os"
	"sort"
	"strconv"
	"strings"
	"testing"
	pb "xone/common/protoc/golang/dexpb"
)

// Exercise every independent original-manager bank, including zero-reserve bins
// whose live shares keep their tree bits initialized.
func TestInfinityBinOriginalManagerServiceBanks(t *testing.T) {
	raw, err := os.ReadFile("../../test/contracts/infinity-bin-service.json")
	if err != nil {
		t.Fatal(err)
	}
	var data struct {
		Manager string `json:"manager"`
		Vault   string `json:"vault"`
		Pools   []struct {
			ID      string `json:"id"`
			BinStep uint32 `json:"bin_step"`
			Key     struct {
				Currency0  string `json:"currency0"`
				Currency1  string `json:"currency1"`
				Hooks      string `json:"hooks"`
				Parameters string `json:"parameters"`
			} `json:"key"`
		} `json:"pools"`
		RPCBanks []struct {
			ID    string `json:"id"`
			Block struct {
				Number    uint64 `json:"num"`
				Hash      string `json:"hash"`
				Timestamp uint64 `json:"timestamp"`
			} `json:"block"`
			Code         map[string]string `json:"code"`
			AppReserves  map[string]Amount `json:"app_reserves"`
			ProtocolFees map[string]Amount `json:"protocol_fees"`
			Pools        []struct {
				PoolID      string `json:"pool_id"`
				ActiveID    uint32 `json:"active_id"`
				LPFee       uint32 `json:"lp_fee"`
				ProtocolFee uint32 `json:"protocol_fee"`
				Bins        map[string]struct {
					Reserve0    Amount `json:"reserve0"`
					Reserve1    Amount `json:"reserve1"`
					TotalShares Amount `json:"total_shares"`
				} `json:"bins"`
				Tree struct {
					Level0 string            `json:"level0"`
					Level1 map[string]string `json:"level1"`
					Level2 map[string]string `json:"level2"`
				} `json:"tree"`
			} `json:"pools"`
		} `json:"RPCbanks"`
	}
	if err = json.Unmarshal(raw, &data); err != nil {
		t.Fatal(err)
	}
	if len(data.RPCBanks) != 5 || len(data.Pools) != 2 {
		t.Fatal("incomplete original manager fixture")
	}
	template, _ := infinityBinFixture(t)
	fromHex := func(v string) Amount {
		n, ok := new(big.Int).SetString(strings.TrimPrefix(v, "0x"), 16)
		if !ok {
			t.Fatal("invalid hex", v)
		}
		return Amount(n.String())
	}
	words := func(values map[string]string) []InfinityBinTreeWord {
		out := []InfinityBinTreeWord{}
		for k, v := range values {
			n, err := strconv.ParseUint(strings.TrimPrefix(k, "0x"), 16, 32)
			if err != nil {
				t.Fatal(err)
			}
			out = append(out, InfinityBinTreeWord{Index: uint32(n), Value: fromHex(v)})
		}
		sort.Slice(out, func(i, j int) bool { return out[i].Index < out[j].Index })
		return out
	}
	for _, bank := range data.RPCBanks {
		t.Run(bank.ID, func(t *testing.T) {
			codeHash := func(address string) string {
				b, err := hex.DecodeString(strings.TrimPrefix(bank.Code[address], "0x"))
				if err != nil || len(b) == 0 {
					t.Fatal("missing runtime", address, err)
				}
				sum := sha256.Sum256(b)
				return hex.EncodeToString(sum[:])
			}
			snapshot := template
			snapshot.Pools = []Pool{}
			snapshot.Reference.Block.Number = bank.Block.Number
			snapshot.Reference.Block.Hash = bank.Block.Hash
			snapshot.Reference.Block.TimestampMS = bank.Block.Timestamp * 1000
			for i, b := range bank.Pools {
				key := data.Pools[i]
				if key.ID != b.PoolID {
					t.Fatal("pool order")
				}
				s := &InfinityBinState{Manager: data.Manager, ManagerCodeSHA256: codeHash(data.Manager), Vault: data.Vault, VaultCodeSHA256: codeHash(data.Vault), PoolID: b.PoolID, Hooks: key.Key.Hooks, Parameters: key.Key.Parameters, Token0CodeSHA256: codeHash(key.Key.Currency0), Token1CodeSHA256: codeHash(key.Key.Currency1), BinStep: key.BinStep, ActiveID: b.ActiveID, LPFee: b.LPFee, ProtocolFee0: b.ProtocolFee & 4095, ProtocolFee1: b.ProtocolFee >> 12, MaxBins: 64, MaxTreeWords: 64, MaxSteps: 64, TreeLevel0: fromHex(b.Tree.Level0), TreeLevel1: words(b.Tree.Level1), TreeLevel2: words(b.Tree.Level2), Bins: []InfinityBinEntry{}, AppReserve0: bank.AppReserves[key.Key.Currency0], AppReserve1: bank.AppReserves[key.Key.Currency1], ProtocolFees0: bank.ProtocolFees[key.Key.Currency0], ProtocolFees1: bank.ProtocolFees[key.Key.Currency1], BlockNumber: bank.Block.Number, BlockHash: bank.Block.Hash, Timestamp: bank.Block.Timestamp}
				for id, bin := range b.Bins {
					n, err := strconv.ParseUint(id, 10, 32)
					if err != nil {
						t.Fatal(err)
					}
					s.Bins = append(s.Bins, InfinityBinEntry{ID: uint32(n), Reserve0: bin.Reserve0, Reserve1: bin.Reserve1, TotalShares: bin.TotalShares})
				}
				sort.Slice(s.Bins, func(i, j int) bool { return s.Bins[i].ID < s.Bins[j].ID })
				p := template.Pools[0].Clone()
				p.PoolID = data.Manager + "/" + b.PoolID
				p.Token0 = key.Key.Currency0
				p.Token1 = key.Key.Currency1
				p.FeeNumerator = uint64(b.LPFee)
				p.InfinityBin = s
				if err := p.Validate(); err != nil {
					t.Fatal(fmt.Sprintf("pool %d", i), err)
				}
				snapshot.Pools = append(snapshot.Pools, p)
			}
			snapshot.Reference.ScopeHash, err = ScopeHash(snapshot.Pools)
			if err != nil {
				t.Fatal(err)
			}
			if err := snapshot.Validate(snapshot.Reference.OwnerService, snapshot.Reference.OwnerAssignmentVersion); err != nil {
				t.Fatal(err)
			}
			message, err := SnapshotToProto(snapshot)
			if err != nil {
				t.Fatal(err)
			}
			wire, err := proto.Marshal(message)
			if err != nil {
				t.Fatal(err)
			}
			decoded := proto.Clone(message)
			proto.Reset(decoded)
			if err = proto.Unmarshal(wire, decoded); err != nil {
				t.Fatal(err)
			}
			got, err := SnapshotFromProto(decoded.(*pb.OnlineSnapshot))
			if err != nil {
				t.Fatal(err)
			}
			a, err := Hash(snapshot)
			if err != nil {
				t.Fatal(err)
			}
			b, err := Hash(got)
			if err != nil || a != b {
				t.Fatal("bank state lost", err)
			}
		})
	}
}
