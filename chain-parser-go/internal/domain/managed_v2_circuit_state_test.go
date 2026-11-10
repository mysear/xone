package domain

import (
	"encoding/json"
	"fmt"
	"math/big"
	"os"
	"reflect"
	"strings"
	"testing"

	"google.golang.org/protobuf/proto"
	pb "xone/common/protoc/golang/dexpb"
)

// Synthetic closed-form cache-hit fixture, independent of the native producer.
func circuitRow(token string, weight, scale Amount, lower, upper, price, cacheLower, cacheUpper *big.Int) ManagedV2CircuitToken {
	one := big.NewInt(1_000_000_000_000_000_000)
	word := new(big.Int)
	for _, f := range []struct {
		value *big.Int
		shift uint
	}{{lower, 0}, {upper, 16}, {weight.Big(), 32}, {price, 96}, {cacheLower, 192}, {cacheUpper, 224}} {
		word.Or(word, new(big.Int).Lsh(new(big.Int).Set(f.value), f.shift))
	}
	bound := func(side, cache *big.Int, ceil bool) Amount {
		if side.Sign() == 0 || price.Sign() == 0 {
			return "0"
		}
		ratio := new(big.Int).Quo(new(big.Int).Mul(cache, new(big.Int).Mul(big.NewInt(10), one)), new(big.Int).SetUint64(1<<32-1))
		x := new(big.Int).Mul(price, ratio)
		if ceil {
			x.Add(x, new(big.Int).Sub(one, big.NewInt(1)))
		}
		x.Quo(x, one).Mul(x, scale.Big()).Quo(x, one)
		return Amount(x.String())
	}
	return ManagedV2CircuitToken{Token: token, StateWord: Amount(word.String()), ReferenceBPTPrice: Amount(new(big.Int).Quo(new(big.Int).Mul(price, scale.Big()), one).String()), ReferenceWeight: weight, LowerPercentage: Amount(new(big.Int).Lsh(new(big.Int).Set(lower), 48).String()), UpperPercentage: Amount(new(big.Int).Lsh(new(big.Int).Set(upper), 48).String()), LowerBPTPriceBound: bound(lower, cacheLower, true), UpperBPTPriceBound: bound(upper, cacheUpper, false)}
}
func circuitFixture(t *testing.T) Pool {
	t.Helper()
	raw, e := os.ReadFile("../../test/contracts/managed-v2-bpt-wire.json")
	if e != nil {
		t.Fatal(e)
	}
	var f struct {
		Snapshot struct {
			Pools []Pool `json:"pools"`
		} `json:"snapshot"`
	}
	if e = json.Unmarshal(raw, &f); e != nil {
		t.Fatal(e)
	}
	p := f.Snapshot.Pools[0]
	p.Protocol = ManagedV2BreakerProtocol
	p.AdapterVersion = ManagedV2BreakerAdapterVersion
	s := p.MultiAsset
	s.ManagedV2.CircuitBreakers = &ManagedV2CircuitState{Tokens: []ManagedV2CircuitToken{}}
	for i, a := range s.Assets {
		row := circuitRow(a.Token, s.Weights[i], a.Scaling, big.NewInt(355), big.NewInt(7105), big.NewInt(1234567890123456789), new(big.Int).SetUint64(1<<31), new(big.Int).SetUint64(1<<32-1))
		s.ManagedV2.CircuitBreakers.Tokens = append(s.ManagedV2.CircuitBreakers.Tokens, row)
		s.ManagedV2.TokenInfo[i].CircuitBreakerEnabled = true
	}
	return p
}
func TestManagedV2CircuitClosedWireClone(t *testing.T) {
	p := circuitFixture(t)
	if e := p.Validate(); e != nil {
		t.Fatal(e)
	}
	if profile, ok := Protocol(ManagedV2BreakerProtocol); !ok || profile.Family != "multi" || profile.AdapterVersion != ManagedV2BreakerAdapterVersion {
		t.Fatal("profile")
	}
	m := p.MultiAsset.ManagedV2
	if m.CircuitBreakers.Tokens[0].StateWord.Big().BitLen() != 256 {
		t.Fatal("high cached32 bit lost")
	}
	w, e := MultiAssetToProto(p.MultiAsset)
	if e != nil {
		t.Fatal(e)
	}
	if f := w.ManagedV2.ProtoReflect().Descriptor().Fields().ByName("circuit_breakers"); f.Number() != 42 || !f.HasPresence() {
		t.Fatal("tag42")
	}
	b, e := proto.Marshal(w)
	if e != nil {
		t.Fatal(e)
	}
	q := new(pb.MultiAssetState)
	if e = proto.Unmarshal(b, q); e != nil {
		t.Fatal(e)
	}
	back, e := MultiAssetFromProto(q)
	if e != nil || !reflect.DeepEqual(back, p.MultiAsset) {
		t.Fatal("roundtrip", e)
	}
	clone := p.Clone()
	clone.MultiAsset.ManagedV2.CircuitBreakers.Tokens[0].StateWord = "0"
	if m.CircuitBreakers.Tokens[0].StateWord == "0" {
		t.Fatal("shallow clone")
	}
	if old, e := ManagedV2CircuitStateFromProto(nil); e != nil || old != nil {
		t.Fatal("absence gained capability", e)
	}
	for i := 0; i < 7; i++ {
		q := proto.Clone(w).(*pb.MultiAssetState)
		token := q.ManagedV2.CircuitBreakers.Tokens[0]
		fields := []**pb.Amount{&token.StateWord, &token.ReferenceBptPrice, &token.ReferenceWeight, &token.LowerPercentage, &token.UpperPercentage, &token.LowerBptPriceBound, &token.UpperBptPriceBound}
		*fields[i] = nil
		if _, e := MultiAssetFromProto(q); e == nil {
			t.Fatalf("missing amount %d", i)
		}
	}
	q = proto.Clone(w).(*pb.MultiAssetState)
	q.ManagedV2.CircuitBreakers.Tokens[0].StateWord.Uint256Be = make([]byte, 33)
	if _, e := MultiAssetFromProto(q); e == nil {
		t.Fatal("wide word")
	}
}
func TestManagedV2CircuitStrictJSON(t *testing.T) {
	p := circuitFixture(t)
	row := p.MultiAsset.ManagedV2.CircuitBreakers.Tokens[0]
	raw, _ := json.Marshal(row)
	var fields map[string]json.RawMessage
	_ = json.Unmarshal(raw, &fields)
	for key := range fields {
		for _, null := range []bool{false, true} {
			q := map[string]json.RawMessage{}
			for k, v := range fields {
				q[k] = v
			}
			if null {
				q[key] = json.RawMessage(`null`)
			} else {
				delete(q, key)
			}
			b, _ := json.Marshal(q)
			var s ManagedV2CircuitToken
			if json.Unmarshal(b, &s) == nil {
				t.Fatalf("missing/null %s", key)
			}
		}
	}
	for _, b := range []string{`{}`, `{"tokens":null}`, `{"tokens":[],"extra":0}`, `{"Tokens":[]}`, `{"tokens":[],"tokens":[]}`} {
		var s ManagedV2CircuitState
		if json.Unmarshal([]byte(b), &s) == nil {
			if e := s.Validate(); e == nil {
				t.Fatal("bad state", b)
			}
		}
	}
	for _, tail := range []string{`,"Token":"x"}`, `,"unknown":0}`, `,"token":"x"}`} {
		var s ManagedV2CircuitToken
		if json.Unmarshal([]byte(strings.TrimSuffix(string(raw), "}")+tail), &s) == nil {
			t.Fatal("unknown/case/duplicate", tail)
		}
	}
	parent, _ := json.Marshal(p.MultiAsset.ManagedV2)
	var parentFields map[string]json.RawMessage
	if e := json.Unmarshal(parent, &parentFields); e != nil {
		t.Fatal(e)
	}
	parentFields["circuit_breakers"] = json.RawMessage(`null`)
	parent, _ = json.Marshal(parentFields)
	var m BalancerManagedV2State
	if json.Unmarshal(parent, &m) == nil {
		t.Fatal("parent null")
	}
}
func TestManagedV2CircuitRejectMutations(t *testing.T) {
	base := circuitFixture(t)
	cases := map[string]func(*Pool){
		"missing breaker":  func(p *Pool) { p.MultiAsset.ManagedV2.CircuitBreakers = nil },
		"missing bpt":      func(p *Pool) { p.MultiAsset.ManagedV2.BPT = nil },
		"missing schedule": func(p *Pool) { p.MultiAsset.ManagedV2.Schedule = nil },
		"enabled false":    func(p *Pool) { p.MultiAsset.ManagedV2.TokenInfo[0].CircuitBreakerEnabled = false },
		"word width": func(p *Pool) {
			p.MultiAsset.ManagedV2.CircuitBreakers.Tokens[0].StateWord = Amount(new(big.Int).Lsh(big.NewInt(1), 256).String())
		},
		"word invalid":  func(p *Pool) { p.MultiAsset.ManagedV2.CircuitBreakers.Tokens[0].StateWord = "" },
		"ref weight":    func(p *Pool) { p.MultiAsset.ManagedV2.CircuitBreakers.Tokens[0].ReferenceWeight = "0" },
		"ref price":     func(p *Pool) { p.MultiAsset.ManagedV2.CircuitBreakers.Tokens[0].ReferenceBPTPrice = "0" },
		"lower percent": func(p *Pool) { p.MultiAsset.ManagedV2.CircuitBreakers.Tokens[0].LowerPercentage = "0" },
		"upper percent": func(p *Pool) { p.MultiAsset.ManagedV2.CircuitBreakers.Tokens[0].UpperPercentage = "0" },
		"lower rounding": func(p *Pool) {
			r := &p.MultiAsset.ManagedV2.CircuitBreakers.Tokens[0]
			r.LowerBPTPriceBound = Amount(new(big.Int).Add(r.LowerBPTPriceBound.Big(), big.NewInt(1)).String())
		},
		"upper rounding": func(p *Pool) {
			r := &p.MultiAsset.ManagedV2.CircuitBreakers.Tokens[0]
			r.UpperBPTPriceBound = Amount(new(big.Int).Add(r.UpperBPTPriceBound.Big(), big.NewInt(1)).String())
		},
		"identity": func(p *Pool) {
			p.MultiAsset.ManagedV2.CircuitBreakers.Tokens[0].Token = "0x0000000000000000000000000000000000000000"
		},
		"order": func(p *Pool) {
			s := p.MultiAsset.ManagedV2.CircuitBreakers
			s.Tokens[0], s.Tokens[1] = s.Tokens[1], s.Tokens[0]
		},
		"duplicate":  func(p *Pool) { s := p.MultiAsset.ManagedV2.CircuitBreakers; s.Tokens[1] = s.Tokens[0] },
		"length":     func(p *Pool) { s := p.MultiAsset.ManagedV2.CircuitBreakers; s.Tokens = s.Tokens[:1] },
		"nil tokens": func(p *Pool) { p.MultiAsset.ManagedV2.CircuitBreakers.Tokens = nil },
		"too many": func(p *Pool) {
			s := p.MultiAsset.ManagedV2.CircuitBreakers
			for len(s.Tokens) < 51 {
				s.Tokens = append(s.Tokens, s.Tokens[0])
			}
		},
		"adapter":     func(p *Pool) { p.AdapterVersion = ManagedV2BptAdapterVersion },
		"nonstandard": func(p *Pool) { p.TokenBehavior = "fee-on-transfer" },
	}
	for name, change := range cases {
		t.Run(name, func(t *testing.T) {
			p := base.Clone()
			change(&p)
			if e := p.Validate(); e == nil {
				t.Fatal("accepted")
			}
		})
	}
	for _, old := range []string{ManagedV2Protocol, ManagedV2ScheduledProtocol, ManagedV2BptProtocol} {
		for _, zero := range []bool{false, true} {
			p := base.Clone()
			p.Protocol = old
			pr, _ := Protocol(old)
			p.AdapterVersion = pr.AdapterVersion
			if old != ManagedV2BptProtocol {
				p.MultiAsset.ManagedV2.BPT = nil
			}
			if old == ManagedV2Protocol {
				p.MultiAsset.ManagedV2.Schedule = nil
			}
			if zero {
				for i, a := range p.MultiAsset.Assets {
					p.MultiAsset.ManagedV2.CircuitBreakers.Tokens[i] = circuitRow(a.Token, "0", a.Scaling, big.NewInt(0), big.NewInt(0), big.NewInt(0), big.NewInt(0), big.NewInt(0))
					p.MultiAsset.ManagedV2.TokenInfo[i].CircuitBreakerEnabled = false
				}
			}
			if p.Validate() == nil {
				t.Fatal("old smuggled breaker", old, zero)
			}
		}
	}
}
func TestManagedV2CircuitZeroDisabledAndDynamic(t *testing.T) {
	for _, sides := range [][3]int64{{0, 0, 0}, {0, 0, 123}, {0, 7105, 123}, {355, 0, 123}, {355, 7105, 0}} {
		p := circuitFixture(t)
		s := p.MultiAsset
		for i, a := range s.Assets {
			weight := s.Weights[i]
			if sides == [3]int64{0, 0, 0} {
				weight = "0"
			}
			r := circuitRow(a.Token, weight, a.Scaling, big.NewInt(sides[0]), big.NewInt(sides[1]), big.NewInt(sides[2]), big.NewInt(0), big.NewInt(0))
			s.ManagedV2.CircuitBreakers.Tokens[i] = r
			s.ManagedV2.TokenInfo[i].CircuitBreakerEnabled = r.StateWord != "0"
		}
		if e := p.Validate(); e != nil {
			t.Fatal(sides, e)
		}
	}
	p := circuitFixture(t)
	s := p.MultiAsset
	r := &s.ManagedV2.CircuitBreakers.Tokens[0]
	*r = circuitRow(r.Token, "10000000000000000", s.Assets[0].Scaling, big.NewInt(355), big.NewInt(7105), big.NewInt(123), big.NewInt(1), big.NewInt(1))
	r.LowerBPTPriceBound = "7"
	r.UpperBPTPriceBound = "9"
	if e := p.Validate(); e != nil {
		t.Fatal("dynamic bounds retained pending pricing math", e)
	}
	m := s.ManagedV2
	m.ExecutionContext = &ManagedV2ExecutionContext{BlockNumber: m.BlockNumber, Timestamp: m.Timestamp}
	if e := p.Validate(); e != nil {
		t.Fatal(e)
	}
	if _, e := MultiAssetToProto(s); e == nil {
		t.Fatal("derived canonical wire")
	}
}

func TestManagedV2CircuitClosedRoundingScalingAndRanges(t *testing.T) {
	p := circuitFixture(t)
	assets := append([]MultiAsset{}, p.MultiAsset.Assets...)
	weights := p.MultiAsset.Weights
	c := *p.MultiAsset.ManagedV2.CircuitBreakers
	c.Tokens = append([]ManagedV2CircuitToken{}, c.Tokens...)
	assets[0].Scaling = "10000000000000000000"
	c.Tokens[0] = circuitRow(assets[0].Token, weights[0], assets[0].Scaling, big.NewInt(355), big.NewInt(7105), big.NewInt(1), new(big.Int).SetUint64(1<<31), new(big.Int).SetUint64(1<<31))
	r := c.Tokens[0]
	if r.ReferenceBPTPrice != "10" || r.LowerBPTPriceBound != "60" || r.UpperBPTPriceBound != "50" || r.LowerPercentage != "99923616732282880" {
		t.Fatal("closed ceil/floor/scaling constants", r)
	}
	if e := c.validatePool(assets, weights); e != nil {
		t.Fatal(e)
	}
	bad := c
	bad.Tokens = append([]ManagedV2CircuitToken{}, c.Tokens...)
	bad.Tokens[0].ReferenceBPTPrice = "1"
	if e := bad.validatePool(assets, weights); e == nil {
		t.Fatal("unscaled price mismatch")
	}
	assets[0].Scaling = "1000000000000000000"
	if e := c.validatePool(assets, weights); e == nil {
		t.Fatal("scaling mismatch")
	}
	for _, tc := range []struct {
		weight       Amount
		lower, upper int64
		ok           bool
	}{{"10000000000000000", 355, 3552, true}, {"1000000000000000000", 3552, 35527, true}, {"9999999999999999", 355, 7105, false}, {"1000000000000000001", 355, 7105, false}, {"0", 355, 7105, false}, {weights[0], 3553, 7105, false}, {weights[0], 355, 35528, false}} {
		q := c
		q.Tokens = append([]ManagedV2CircuitToken{}, c.Tokens...)
		q.Tokens[0] = circuitRow(assets[0].Token, tc.weight, assets[0].Scaling, big.NewInt(tc.lower), big.NewInt(tc.upper), big.NewInt(1), big.NewInt(0), big.NewInt(0))
		if (q.Validate() == nil) != tc.ok {
			t.Fatal("range", tc)
		}
	}
	max96 := new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), 96), big.NewInt(1))
	c.Tokens[0] = circuitRow(assets[0].Token, weights[0], assets[0].Scaling, big.NewInt(0), big.NewInt(0), max96, big.NewInt(0), big.NewInt(0))
	if e := c.validatePool(assets, weights); e != nil {
		t.Fatal("full raw96", e)
	}
	for _, field := range []string{"lower", "upper"} {
		q := c
		q.Tokens = append([]ManagedV2CircuitToken{}, c.Tokens...)
		if field == "lower" {
			q.Tokens[0].LowerBPTPriceBound = "1"
		} else {
			q.Tokens[0].UpperBPTPriceBound = "1"
		}
		if q.Validate() == nil {
			t.Fatal("disabled getter nonzero")
		}
	}
}
func TestManagedV2CircuitSourceHAndDerivedIsolation(t *testing.T) {
	p := circuitFixture(t)
	s := circuitSnapshot(t, p)
	if e := s.Validate("test", 1); e != nil {
		t.Fatal(e)
	}
	w, e := SnapshotToProto(s)
	if e != nil {
		t.Fatal(e)
	}
	back, e := SnapshotFromProto(w)
	if e != nil || !reflect.DeepEqual(s, back) {
		t.Fatal("snapshot roundtrip", e)
	}
	for i := 0; i < 4; i++ {
		q := circuitSnapshot(t, p.Clone())
		switch i {
		case 0:
			q.Reference.Block.Number++
		case 1:
			q.Reference.Block.TimestampMS++
		case 2:
			q.Reference.Block.Hash = "0x" + strings.Repeat("b", 64)
		case 3:
			m := q.Pools[0].MultiAsset.ManagedV2
			m.TokenInfo[0].LastChangeBlock = m.BlockNumber + 1
		}
		if q.Validate("test", 1) == nil {
			t.Fatal("foreign source H", i)
		}
	}
	m := p.MultiAsset.ManagedV2
	m.ExecutionContext = &ManagedV2ExecutionContext{BlockNumber: m.BlockNumber, Timestamp: m.Timestamp}
	if p.Validate() != nil {
		t.Fatal("derived domain")
	}
	s = circuitSnapshot(t, p)
	if s.Validate("test", 1) == nil {
		t.Fatal("derived canonical snapshot")
	}
	if _, e := SnapshotToProto(s); e == nil {
		t.Fatal("derived canonical snapshot wire")
	}
}

func circuitSnapshot(t *testing.T, p Pool) Snapshot {
	t.Helper()
	raw, e := os.ReadFile("../../test/contracts/managed-v2-bpt-wire.json")
	if e != nil {
		t.Fatal(e)
	}
	var f struct {
		Snapshot Snapshot `json:"snapshot"`
	}
	if e = json.Unmarshal(raw, &f); e != nil {
		t.Fatal(e)
	}
	s := f.Snapshot
	s.Pools = []Pool{p}
	s.Reference.OwnerService = "test"
	s.Reference.ScopeHash, e = ScopeHash(s.Pools)
	if e != nil {
		t.Fatal(e)
	}
	s.Reference.AdapterSetHash, e = Hash([]string{p.AdapterVersion})
	if e != nil {
		t.Fatal(e)
	}
	return s
}
func TestManagedV2CircuitKnownZeroPresence(t *testing.T) {
	p := circuitFixture(t)
	m := p.MultiAsset.ManagedV2
	for i, a := range p.MultiAsset.Assets {
		m.CircuitBreakers.Tokens[i] = circuitRow(a.Token, "0", a.Scaling, big.NewInt(0), big.NewInt(0), big.NewInt(0), big.NewInt(0), big.NewInt(0))
		m.TokenInfo[i].CircuitBreakerEnabled = false
	}
	if e := p.Validate(); e != nil {
		t.Fatal(e)
	}
	w, e := MultiAssetToProto(p.MultiAsset)
	if e != nil {
		t.Fatal(e)
	}
	if w.ManagedV2.TokenInfo[0].CircuitBreakerEnabled == nil || *w.ManagedV2.TokenInfo[0].CircuitBreakerEnabled {
		t.Fatal("known false")
	}
	token := w.ManagedV2.CircuitBreakers.Tokens[0]
	for _, a := range []*pb.Amount{token.StateWord, token.ReferenceBptPrice, token.ReferenceWeight, token.LowerPercentage, token.UpperPercentage, token.LowerBptPriceBound, token.UpperBptPriceBound} {
		if a == nil || len(a.Uint256Be) != 32 || new(big.Int).SetBytes(a.Uint256Be).Sign() != 0 {
			t.Fatal("known zero required amount")
		}
	}
	q, e := MultiAssetFromProto(w)
	if e != nil || !reflect.DeepEqual(q, p.MultiAsset) {
		t.Fatal("zero wire", e)
	}
	old := bptStateFixture(t)
	wire, e := MultiAssetToProto(old.MultiAsset)
	if e != nil {
		t.Fatal(e)
	}
	if wire.ManagedV2.CircuitBreakers != nil {
		t.Fatal("old wire gained breaker")
	}
	back, e := MultiAssetFromProto(wire)
	if e != nil || back.ManagedV2.CircuitBreakers != nil {
		t.Fatal("old None gained breaker", e)
	}
	raw, _ := json.Marshal(back.ManagedV2)
	if strings.Contains(string(raw), "circuit_breakers") {
		t.Fatal("None JSON not omitted")
	}
}

func TestManagedV2CircuitProtoWidthsAndCanonicalTokens(t *testing.T) {
	p := circuitFixture(t)
	state := p.MultiAsset.ManagedV2.CircuitBreakers
	w, e := ManagedV2CircuitStateToProto(state)
	if e != nil {
		t.Fatal(e)
	}
	for field := 0; field < 7; field++ {
		for _, width := range []int{0, 31, 33} {
			q := proto.Clone(w).(*pb.ManagedV2CircuitState)
			r := q.Tokens[0]
			amounts := []*pb.Amount{r.StateWord, r.ReferenceBptPrice, r.ReferenceWeight, r.LowerPercentage, r.UpperPercentage, r.LowerBptPriceBound, r.UpperBptPriceBound}
			amounts[field].Uint256Be = make([]byte, width)
			if _, e := ManagedV2CircuitStateFromProto(q); e == nil {
				t.Fatalf("field %d width %d accepted", field, width)
			}
		}
	}
	q := proto.Clone(w).(*pb.ManagedV2CircuitState)
	q.Tokens[0] = nil
	if _, e := ManagedV2CircuitStateFromProto(q); e == nil {
		t.Fatal("nil token")
	}
	for _, token := range []string{"", strings.ToUpper(state.Tokens[0].Token), "0x1"} {
		q := *state
		q.Tokens = append([]ManagedV2CircuitToken{}, state.Tokens...)
		q.Tokens[0].Token = token
		if q.Validate() == nil {
			t.Fatal("noncanonical token", token)
		}
	}
	fifty := ManagedV2CircuitState{Tokens: []ManagedV2CircuitToken{}}
	for i := 0; i < 50; i++ {
		r := circuitRow(fmt.Sprintf("0x%040x", i+1), "0", "1000000000000000000", big.NewInt(0), big.NewInt(0), big.NewInt(0), big.NewInt(0), big.NewInt(0))
		fifty.Tokens = append(fifty.Tokens, r)
	}
	if e := fifty.Validate(); e != nil {
		t.Fatal("valid fifty", e)
	}
	fifty.Tokens = append(fifty.Tokens, fifty.Tokens[49])
	if fifty.Validate() == nil {
		t.Fatal("fifty-one")
	}
}

func TestManagedV2CircuitDependencyEntryRequiresManagedState(t *testing.T) {
	for _, protocol := range []string{ManagedV2Protocol, ManagedV2ScheduledProtocol, ManagedV2BptProtocol, ManagedV2BreakerProtocol} {
		for _, missingMulti := range []bool{false, true} {
			p := circuitFixture(t)
			p.Protocol = protocol
			if missingMulti {
				p.MultiAsset = nil
			} else {
				p.MultiAsset.ManagedV2 = nil
			}
			if ValidateManagedV2PoolDependencies([]Pool{p}) == nil {
				t.Errorf("%s missingMulti=%v skipped declaration validation", protocol, missingMulti)
			}
			if _, e := ManagedV2ExecutionCapabilities([]Pool{p}); e == nil {
				t.Errorf("%s missingMulti=%v capability declaration accepted", protocol, missingMulti)
			}
		}
	}
}

func TestManagedV2CircuitUpperInputRules(t *testing.T) {
	compress := func(input int64) *big.Int { return new(big.Int).Rsh(big.NewInt(input), 48) }
	for _, tc := range []struct {
		name         string
		lower, upper int64
		valid        bool
	}{
		{"upper_half_lower_disabled", 0, 500_000_000_000_000_000, true},
		{"upper_point_two_lower_point_one", 100_000_000_000_000_000, 200_000_000_000_000_000, true},
		{"equal_compressed_bounds", 100_000_000_000_000_000, 100_000_000_000_000_000, true},
		{"upper_one_compresses_to_disabled", 0, 1, true},
		{"upper_zero_with_lower", 100_000_000_000_000_000, 0, true},
		{"upper_small_nonzero_word", 0, 1 << 48, true},
		{"lower_above_nonzero_upper", 200_000_000_000_000_000, 100_000_000_000_000_000, false},
		{"lower_above_small_nonzero_upper", 100_000_000_000_000_000, 1 << 48, false},
	} {
		t.Run(tc.name, func(t *testing.T) {
			p := circuitFixture(t)
			for i, a := range p.MultiAsset.Assets {
				p.MultiAsset.ManagedV2.CircuitBreakers.Tokens[i] = circuitRow(a.Token, p.MultiAsset.Weights[i], a.Scaling, compress(tc.lower), compress(tc.upper), big.NewInt(1), big.NewInt(1<<31), big.NewInt(1<<31))
			}
			err := p.Validate()
			if (err == nil) != tc.valid {
				t.Fatalf("input lower=%d upper=%d: %v", tc.lower, tc.upper, err)
			}
			if !tc.valid {
				return
			}
			w, e := MultiAssetToProto(p.MultiAsset)
			if e != nil {
				t.Fatal(e)
			}
			back, e := MultiAssetFromProto(w)
			if e != nil || !reflect.DeepEqual(back, p.MultiAsset) {
				t.Fatal("legal compressed bounds lost in wire", e)
			}
		})
	}
}
