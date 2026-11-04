package domain

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"os"
	"reflect"
	"testing"

	"google.golang.org/protobuf/proto"
	"google.golang.org/protobuf/reflect/protoreflect"
	pb "xone/common/protoc/golang/dexpb"
)

const managedV2WireBankSHA = "1ee7ec5dd981994af6dd42eb75717cf83a5b209857883fa2923885ce7181fb66"
const managedV2WirePoolSHA = "736d758cba41e28332db994c15b3d9fe13fe04fe08f57e9f80ed90425e35002c"

// The Pool is an original local replay observation. The snapshot envelope is
// synthetic; wire agreement supplies neither live-chain trust nor execution proof.
func managedV2WireGolden(t *testing.T) (Snapshot, *pb.OnlineSnapshot, []byte) {
	t.Helper()
	raw, err := os.ReadFile("../../test/contracts/managed-v2-aum-wire.json")
	if err != nil {
		t.Fatal(err)
	}
	var f struct {
		Snapshot           Snapshot        `json:"snapshot"`
		ProtobufHex        string          `json:"protobuf_hex"`
		ProtobufSHA256     string          `json:"protobuf_sha256"`
		OriginalBankSHA256 string          `json:"original_bank_sha256"`
		OriginalBankBytes  uint64          `json:"original_bank_bytes"`
		Authority          string          `json:"authority"`
		SourceRefs         json.RawMessage `json:"source_refs"`
		OriginalFields     json.RawMessage `json:"original_fields"`
	}
	if err = json.Unmarshal(raw, &f); err != nil {
		t.Fatal(err)
	}
	if f.OriginalBankSHA256 != managedV2WireBankSHA || f.OriginalBankBytes != 7905624 || f.Authority != "native-state + synthetic snapshot envelope" {
		t.Fatal("native state / synthetic envelope authority changed")
	}
	var refs any
	if err = json.Unmarshal(f.SourceRefs, &refs); err != nil {
		t.Fatal(err)
	}
	var expectedRefs any
	if err = json.Unmarshal([]byte(`{"bank_sha256":"`+managedV2WireBankSHA+`","snapshot":{"ref":"f2b87673a859fedade77f33b274c251f5e2e76f595705497d0dfc1c6dc455f1d"},"state_header":{"ref":"89b0bd3d9e255afa44d247e4c4b3fb4b10c393b8a4befcffb459533ad7340452"},"origin_snapshot":{"ref":"f2b87673a859fedade77f33b274c251f5e2e76f595705497d0dfc1c6dc455f1d"}}`), &expectedRefs); err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(refs, expectedRefs) {
		t.Fatal("projection source_refs changed")
	}
	var original Pool
	if err = json.Unmarshal(f.OriginalFields, &original); err != nil {
		t.Fatal(err)
	}
	// Pin the complete source Pool, including fields outside ManagedV2.
	var originalValue any
	dec := json.NewDecoder(bytes.NewReader(f.OriginalFields))
	dec.UseNumber()
	if err = dec.Decode(&originalValue); err != nil {
		t.Fatal(err)
	}
	if h, e := Hash(originalValue); e != nil || h != managedV2WirePoolSHA {
		t.Fatalf("original Pool changed: %s %v", h, e)
	}
	s := f.Snapshot
	if len(s.Pools) != 1 || !reflect.DeepEqual(s.Pools[0], original) {
		t.Fatal("snapshot differs from complete original_fields Pool")
	}
	encodedPool, err := json.Marshal(s.Pools[0])
	if err != nil {
		t.Fatal(err)
	}
	var poolValue any
	dec = json.NewDecoder(bytes.NewReader(encodedPool))
	dec.UseNumber()
	if err = dec.Decode(&poolValue); err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(originalValue, poolValue) {
		t.Fatal("original_fields JSON lost or added Pool fields")
	}
	b := s.Reference.Block
	if b.ChainKey != "evm:31337" || b.Number != 26 || b.TimestampMS != 1767225600000 || b.Hash != "0xc37cd406f5e09888d3d5a8517cb32e277a7d1ba2f438a87b248407e7b1dc5f4b" || b.ParentHash != "0xf4cce27eb31871df28e471708277aa7c57dac7a919fc98e2ff086d5dcf7922c1" {
		t.Fatal("snapshot is not bound to original before header")
	}
	if s.Source == "CURRENT_CHAIN_STATE" {
		t.Fatal("synthetic envelope claimed current chain authority")
	}
	if err = s.Validate(s.Reference.OwnerService, s.Reference.OwnerAssignmentVersion); err != nil {
		t.Fatal(err)
	}
	scope, err := ScopeHash(s.Pools)
	if err != nil {
		t.Fatal(err)
	}
	adapters, err := Hash([]string{s.Pools[0].AdapterVersion})
	if err != nil {
		t.Fatal(err)
	}
	if s.Reference.ScopeHash != scope || s.Reference.AdapterSetHash != adapters {
		t.Fatal("synthetic envelope scope/adapter hash is incorrect")
	}
	wireBytes, err := hex.DecodeString(f.ProtobufHex)
	if err != nil {
		t.Fatal(err)
	}
	sum := sha256.Sum256(wireBytes)
	if hex.EncodeToString(sum[:]) != f.ProtobufSHA256 {
		t.Fatal("C++ protobuf SHA-256 mismatch")
	}
	w := new(pb.OnlineSnapshot)
	if err = proto.Unmarshal(wireBytes, w); err != nil {
		t.Fatal(err)
	}
	return s, w, wireBytes
}

func TestManagedV2NativeWireGolden(t *testing.T) {
	s, w, raw := managedV2WireGolden(t)
	marshaled, err := SnapshotToProto(s)
	if err != nil {
		t.Fatal(err)
	}
	got, err := proto.MarshalOptions{Deterministic: true}.Marshal(marshaled)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(got, raw) {
		t.Fatal("Go marshal differs from independent C++ protoc bytes")
	}
	round, err := SnapshotFromProto(w)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(round, s) {
		t.Fatal("complete Snapshot/Pool changed on wire decode")
	}
	a, err := Hash(s)
	if err != nil {
		t.Fatal(err)
	}
	b, err := Hash(round)
	if err != nil || a != b {
		t.Fatal("complete Snapshot hash changed", err)
	}
	reencoded, err := SnapshotToProto(round)
	if err != nil {
		t.Fatal(err)
	}
	back, err := proto.MarshalOptions{Deterministic: true}.Marshal(reencoded)
	if err != nil || !bytes.Equal(back, raw) {
		t.Fatal("decode/domain/re-encode lost wire fields", err)
	}
	m := w.Pools[0].MultiAsset.ManagedV2
	if len(m.Dependencies) != 5 || len(m.TokenInfo) != len(s.Pools[0].MultiAsset.Assets) {
		t.Fatal("native dependencies/token_info incomplete")
	}
	// Reflection covers all 39 fields, all five dependencies and every token_info.
	if m.ProtoReflect().Descriptor().Fields().Len() != 39 {
		t.Fatal("ManagedV2 field count changed")
	}
	paths := []string{"state", "dependencies", "token_info"}
	for _, path := range paths {
		n := 1
		if path == "dependencies" {
			n = len(m.Dependencies)
		}
		if path == "token_info" {
			n = len(m.TokenInfo)
		}
		for index := 0; index < n; index++ {
			message := managedV2WireMessage(m, path, index)
			fields := message.Descriptor().Fields()
			for i := 0; i < fields.Len(); i++ {
				field := fields.Get(i)
				t.Run(path+"/"+string(field.Name())+"/"+string(rune('0'+index)), func(t *testing.T) {
					q := proto.Clone(w).(*pb.OnlineSnapshot)
					target := managedV2WireMessage(q.Pools[0].MultiAsset.ManagedV2, path, index)
					if !target.Has(field) {
						t.Fatal("native required field not present")
					}
					target.Clear(field)
					if _, e := SnapshotFromProto(q); e == nil {
						t.Fatal("missing required native wire field accepted")
					}
				})
			}
		}
	}
	// Flip a byte inside the original block hash. Protobuf stays valid but its
	// source bank binding must fail; merely comparing fixture digests is insufficient.
	offset := bytes.Index(raw, m.BlockHash)
	if offset < 0 {
		t.Fatal("source hash missing in protobuf")
	}
	corrupt := append([]byte(nil), raw...)
	corrupt[offset] ^= 1
	if sha256.Sum256(corrupt) == sha256.Sum256(raw) {
		t.Fatal("corruption did not change digest")
	}
	var bad pb.OnlineSnapshot
	if err = proto.Unmarshal(corrupt, &bad); err != nil {
		t.Fatal("counterexample must remain decodable", err)
	}
	if _, err = SnapshotFromProto(&bad); err == nil {
		t.Fatal("flipped source-bank byte accepted")
	}
	// A valid dependency identity change must also affect the full state hash.
	q := proto.Clone(w).(*pb.OnlineSnapshot)
	q.Pools[0].MultiAsset.ManagedV2.Dependencies[0].CodeSha256[0] ^= 1
	changed, err := SnapshotFromProto(q)
	if err != nil {
		t.Fatal(err)
	}
	h, err := Hash(changed)
	if err != nil || h == a {
		t.Fatal("full hash omits dependency identity", err)
	}
}

func managedV2WireMessage(m *pb.BalancerManagedV2State, path string, index int) protoreflect.Message {
	switch path {
	case "dependencies":
		return m.Dependencies[index].ProtoReflect()
	case "token_info":
		return m.TokenInfo[index].ProtoReflect()
	default:
		return m.ProtoReflect()
	}
}

func TestManagedV2RequiredJSONWireGolden(t *testing.T) {
	s, _, _ := managedV2WireGolden(t)
	raw, err := json.Marshal(s.Pools[0])
	if err != nil {
		t.Fatal(err)
	}
	var original map[string]any
	if err = json.Unmarshal(raw, &original); err != nil {
		t.Fatal(err)
	}
	state := original["multi_asset"].(map[string]any)["managed_v2"].(map[string]any)
	if len(state) != 39 {
		t.Fatal("ManagedV2 JSON field count changed")
	}
	for _, part := range []string{"state", "dependencies", "token_info"} {
		objects := []any{state}
		if part != "state" {
			objects = state[part].([]any)
		}
		for index, object := range objects {
			for field := range object.(map[string]any) {
				for _, null := range []bool{false, true} {
					var p map[string]any
					if err = json.Unmarshal(raw, &p); err != nil {
						t.Fatal(err)
					}
					m := p["multi_asset"].(map[string]any)["managed_v2"].(map[string]any)
					target := m
					if part != "state" {
						target = m[part].([]any)[index].(map[string]any)
					}
					if null {
						target[field] = nil
					} else {
						delete(target, field)
					}
					b, e := json.Marshal(p)
					if e != nil {
						t.Fatal(e)
					}
					var decoded Pool
					if e = json.Unmarshal(b, &decoded); e == nil {
						t.Fatalf("missing/null %s[%d].%s accepted", part, index, field)
					}
				}
			}
		}
	}
}

func TestManagedV2KnownZeroPresenceWireGolden(t *testing.T) {
	s, _, _ := managedV2WireGolden(t)
	state := s.Pools[0].Clone().MultiAsset
	m := state.ManagedV2
	m.Timestamp = 0
	m.BlockNumber = 0
	m.LastCollectionTimestamp = 0
	m.BPTIndex = 0
	m.PauseWindowEnd = 0
	m.BufferPeriodEnd = 0
	m.GradualWeightEnd = 0
	m.GradualFeeEnd = 0
	m.SwapsEnabled = false
	m.Paused = false
	m.RecoveryMode = false
	m.AllowlistEnabled = false
	m.ProviderFeeID = "0"
	m.ManagementAUMFee = "0"
	m.LiveProviderAUMPercentage = "0"
	m.CachedProtocolAUMPercentage = "0"
	for i := range m.TokenInfo {
		m.TokenInfo[i].LastChangeBlock = 0
		m.TokenInfo[i].CircuitBreakerEnabled = false
	}
	w, err := MultiAssetToProto(state)
	if err != nil {
		t.Fatal(err)
	}
	// Inspect presence after actual serialization, not just freshly built pointers.
	raw, err := proto.Marshal(w)
	if err != nil {
		t.Fatal(err)
	}
	var decoded pb.MultiAssetState
	if err = proto.Unmarshal(raw, &decoded); err != nil {
		t.Fatal(err)
	}
	r := decoded.ManagedV2.ProtoReflect()
	fields := r.Descriptor().Fields()
	for i := 0; i < fields.Len(); i++ {
		f := fields.Get(i)
		if f.HasPresence() && !r.Has(f) {
			t.Fatalf("Known(0/false) lost: %s", f.Name())
		}
	}
	round, err := MultiAssetFromProto(&decoded)
	if err != nil || !reflect.DeepEqual(round, state) {
		t.Fatal("Known(0/false) changed during wire roundtrip", err)
	}
}
