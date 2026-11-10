package domain

import (
	"encoding/json"
	"google.golang.org/protobuf/proto"
	"os"
	"reflect"
	"strings"
	"testing"
	pb "xone/common/protoc/golang/dexpb"
)

func bptStateFixture(t *testing.T) Pool {
	t.Helper()
	raw, e := os.ReadFile("../../test/contracts/managed-v2-scheduled-wire.json")
	if e != nil {
		t.Fatal(e)
	}
	var v struct {
		Snapshot struct {
			Pools []Pool `json:"pools"`
		} `json:"snapshot"`
	}
	if e = json.Unmarshal(raw, &v); e != nil {
		t.Fatal(e)
	}
	p := v.Snapshot.Pools[0]
	p.Protocol = ManagedV2BptProtocol
	p.AdapterVersion = ManagedV2BptAdapterVersion
	p.MultiAsset.ManagedV2.BPT = &ManagedV2BptState{JoinExitEnabled: true, MinimumBPTLocked: "1000000", AllowlistMembers: []ManagedV2AllowlistMember{{Address: "0x0000000000000000000000000000000000000001", Allowed: false}}}
	return p
}
func TestManagedV2BptProfileControlsAndWire(t *testing.T) {
	p := bptStateFixture(t)
	if e := p.Validate(); e != nil {
		t.Fatal(e)
	}
	raw, e := MultiAssetToProto(p.MultiAsset)
	if e != nil {
		t.Fatal(e)
	}
	encoded, e := proto.Marshal(raw)
	if e != nil {
		t.Fatal(e)
	}
	w := new(pb.MultiAssetState)
	if e = proto.Unmarshal(encoded, w); e != nil {
		t.Fatal(e)
	}
	back, e := MultiAssetFromProto(w)
	if e != nil {
		t.Fatal(e)
	}
	if !reflect.DeepEqual(back, p.MultiAsset) {
		t.Fatal("wire lost BPT state")
	}
	for _, old := range []string{ManagedV2Protocol, ManagedV2ScheduledProtocol} {
		q := p.Clone()
		q.Protocol = old
		profile, _ := Protocol(old)
		q.AdapterVersion = profile.AdapterVersion
		if q.Validate() == nil {
			t.Fatal("old profile accepted BPT")
		}
	}
	b := p.MultiAsset.ManagedV2.BPT
	if allowed, e := b.MemberAllowed(b.AllowlistMembers[0].Address); e != nil || allowed {
		t.Fatal("explicit false lost")
	}
	if _, e := b.MemberAllowed("0x0000000000000000000000000000000000000002"); e == nil {
		t.Fatal("unknown member allowed")
	}
	q := p.Clone()
	q.MultiAsset.ManagedV2.BPT.AllowlistMembers[0].Allowed = true
	if b.AllowlistMembers[0].Allowed {
		t.Fatal("clone aliases BPT membership")
	}
	for _, mutate := range []func(*Pool){func(q *Pool) { q.MultiAsset.ManagedV2.BPT = nil }, func(q *Pool) { q.MultiAsset.ManagedV2.Schedule = nil }, func(q *Pool) { q.MultiAsset.ManagedV2.BPT.JoinExitEnabled = false }, func(q *Pool) { q.MultiAsset.ManagedV2.BPT.MinimumBPTLocked = "0" }, func(q *Pool) { q.MultiAsset.ManagedV2.BPT.BPTLastChangeBlock = 1 << 32 }, func(q *Pool) {
		q.MultiAsset.ManagedV2.BPT.AllowlistMembers = append(q.MultiAsset.ManagedV2.BPT.AllowlistMembers, b.AllowlistMembers[0])
	}} {
		q := p.Clone()
		mutate(&q)
		if q.Validate() == nil {
			t.Fatal("incomplete or conflicting BPT control accepted")
		}
	}
	bp, e := b.toProto()
	if e != nil {
		t.Fatal(e)
	}
	bp.JoinExitEnabled = nil
	if _, e := ManagedV2BptStateFromProto(bp); e == nil {
		t.Fatal("absent false accepted")
	}
	bp, _ = b.toProto()
	bp.AllowlistMembers[0].Allowed = nil
	if _, e := ManagedV2BptStateFromProto(bp); e == nil {
		t.Fatal("absent membership result accepted")
	}
}
func TestManagedV2BptJSONRequiredFields(t *testing.T) {
	b := bptStateFixture(t).MultiAsset.ManagedV2.BPT
	raw, _ := json.Marshal(b)
	var fields map[string]json.RawMessage
	_ = json.Unmarshal(raw, &fields)
	for key := range fields {
		for _, missing := range []bool{true, false} {
			var v map[string]json.RawMessage
			_ = json.Unmarshal(raw, &v)
			if missing {
				delete(v, key)
			} else {
				v[key] = json.RawMessage("null")
			}
			data, _ := json.Marshal(v)
			var out ManagedV2BptState
			if json.Unmarshal(data, &out) == nil {
				t.Fatalf("missing/null %s accepted", key)
			}
		}
	}
	for _, data := range []string{strings.Replace(string(raw), "join_exit_enabled", "JOIN_EXIT_ENABLED", 1), strings.Replace(string(raw), "{", "{\"unknown\":false,", 1), strings.Replace(string(raw), "{", "{\"join_exit_enabled\":true,", 1)} {
		var out ManagedV2BptState
		if json.Unmarshal([]byte(data), &out) == nil {
			t.Fatal("unknown/case alias/duplicate accepted")
		}
	}
}
