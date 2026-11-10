package domain

import (
	"encoding/hex"
	"encoding/json"
	"google.golang.org/protobuf/proto"
	"os"
	"reflect"
	"testing"
	pb "xone/common/protoc/golang/dexpb"
)

func TestSlipstreamIndependentCppWireAndDerivedSourceClosure(t *testing.T) {
	raw, e := os.ReadFile("../../test/contracts/slipstream-context-wire.json")
	if e != nil {
		t.Fatal(e)
	}
	var file struct {
		Vectors []struct {
			Snapshot Snapshot `json:"snapshot"`
			Hex      string   `json:"protobuf_hex"`
		} `json:"vectors"`
	}
	if e = json.Unmarshal(raw, &file); e != nil {
		t.Fatal(e)
	}
	if len(file.Vectors) != 4 {
		t.Fatal("missing contextual C++ cases")
	}
	for _, vector := range file.Vectors {
		if e = vector.Snapshot.Validate("chain-parser-go", 1); e != nil {
			t.Fatal(e)
		}
		raw, e := hex.DecodeString(vector.Hex)
		if e != nil {
			t.Fatal(e)
		}
		var p pb.OnlineSnapshot
		if e = proto.Unmarshal(raw, &p); e != nil {
			t.Fatal(e)
		}
		decoded, e := SnapshotFromProto(&p)
		if e != nil || !reflect.DeepEqual(decoded, vector.Snapshot) {
			t.Fatal("contextual C++ decode", e)
		}
		encoded, e := SnapshotToProto(vector.Snapshot)
		if e != nil || !proto.Equal(encoded, &p) {
			t.Fatal("contextual C++ encode", e)
		}
		for bad := 0; bad < 13; bad++ {
			q := proto.Clone(&p).(*pb.OnlineSnapshot)
			state := q.Pools[0].SlipstreamContext
			switch bad {
			case 0:
				state.Timestamp = nil
			case 1:
				state.BlockNumber = nil
			case 2:
				state.Derived = nil
			case 3:
				state.Derived = func() *bool { v := true; return &v }()
			case 4:
				state.Unlocked = nil
			case 5:
				state.Core = nil
			case 6:
				state.Ticks[0].LiquidityGross = nil
			case 7:
				state.Ticks[0].StakedLiquidityNet = state.Ticks[0].StakedLiquidityNet[:15]
			case 8:
				state.Observations[0].Initialized = nil
			case 9:
				state.Origins[0].ViewSwapFee = nil
			case 10:
				state.FactoryRegistered = nil
			case 11:
				state.VoterCodeSha256 = nil
			case 12:
				state.Observations = state.Observations[:len(state.Observations)-1]
			}
			out, err := SnapshotFromProto(q)
			if err == nil {
				err = out.Validate("chain-parser-go", 1)
			}
			if err == nil {
				t.Fatal("invalid contextual wire accepted", bad)
			}
		}
		copy := vector.Snapshot.Pools[0].Clone()
		copy.SlipstreamContext.Ticks[0].FeeGrowthOutside0 = "0"
		if copy.SlipstreamContext.Ticks[0].FeeGrowthOutside0 == vector.Snapshot.Pools[0].SlipstreamContext.Ticks[0].FeeGrowthOutside0 {
			t.Fatal("full ticks were shallow cloned")
		}
		copy.SlipstreamContext.Observations[0].TickCumulative = "0"
		if vector.Snapshot.Pools[0].SlipstreamContext.Observations[0].TickCumulative == "0" {
			t.Fatal("Oracle was shallow cloned")
		}
	}
}
