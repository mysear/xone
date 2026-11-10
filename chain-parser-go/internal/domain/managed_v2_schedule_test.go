package domain

import (
	"encoding/json"
	"fmt"
	"math/big"
	"reflect"
	"strings"
	"testing"
)

// Synthetic words use the fixed published 20230411 layout. The integer
// checkpoints below are literal original-VM observations, not local formulas.
func managedScheduleWord(values ...struct {
	value  string
	offset uint
}) Amount {
	out := new(big.Int)
	for _, entry := range values {
		v, ok := new(big.Int).SetString(entry.value, 10)
		if !ok {
			panic("test integer")
		}
		out.Or(out, new(big.Int).Lsh(v, entry.offset))
	}
	return Amount(out.String())
}
func scheduleEntry(value string, offset uint) struct {
	value  string
	offset uint
} {
	return struct {
		value  string
		offset uint
	}{value, offset}
}
func managedScheduleFixture() ManagedV2Schedule {
	start, end := uint64(1767225600), uint64(1769817600)
	word := managedScheduleWord(scheduleEntry(fmt.Sprint(start), 0), scheduleEntry(fmt.Sprint(end), 32), scheduleEntry(fmt.Sprint(start), 64), scheduleEntry(fmt.Sprint(end), 96), scheduleEntry("10000000000000000", 128), scheduleEntry("30000000000000000", 190), scheduleEntry("1", 252), scheduleEntry("1", 255))
	return ManagedV2Schedule{PoolStateWord: word, Tokens: []ManagedV2ScheduledToken{
		{Token: "0x0000000000000000000000000000000000000001", StateWord: managedScheduleWord(scheduleEntry("800000000000000000", 0), scheduleEntry("200000000000000000", 64))},
		{Token: "0x0000000000000000000000000000000000000002", StateWord: managedScheduleWord(scheduleEntry("200000000000000000", 0), scheduleEntry("800000000000000000", 64), scheduleEntry("12", 128))},
	}}
}
func TestManagedScheduleOriginalIntegerCheckpoints(t *testing.T) {
	s := managedScheduleFixture()
	before, _ := json.Marshal(s)
	for _, c := range []struct {
		time                  uint64
		weight0, weight1, fee string
	}{
		{1767225599, "800000000000000000", "200000000000000000", "10000000000000000"},
		{1767225600, "800000000000000000", "200000000000000000", "10000000000000000"},
		{1767225601, "799999768518518519", "200000231481481481", "10000007716049382"},
		{1768521600, "500000000000000000", "500000000000000000", "20000000000000000"},
		{1769817599, "200000231481481482", "799999768518518518", "29999992283950617"},
		{1769817600, "200000000000000000", "800000000000000000", "30000000000000000"},
		{uint64(1) << 32, "200000000000000000", "800000000000000000", "30000000000000000"},
	} {
		t.Run(fmt.Sprint(c.time), func(t *testing.T) {
			v, e := s.At(c.time)
			if e != nil {
				t.Fatal(e)
			}
			if !reflect.DeepEqual(v.Weights, []Amount{Amount(c.weight0), Amount(c.weight1)}) || v.SwapFee != Amount(c.fee) {
				t.Fatalf("original floor ordering differs: %+v", v)
			}
			if !v.SwapsEnabled || v.MustAllowlistLPs || v.RecoveryMode || !v.JoinExitEnabled {
				t.Fatal("flags lost")
			}
			if !reflect.DeepEqual(v.Scaling, []Amount{"1000000000000000000", "1000000000000000000000000000000"}) {
				t.Fatal("decimal factor", v.Scaling)
			}
		})
	}
	after, _ := json.Marshal(s)
	if string(after) != string(before) {
		t.Fatal("source words mutated")
	}
}
func TestManagedScheduleComputedStartIsNotCallerTargetSum(t *testing.T) {
	// RS0 source/current and first new-clock weights were observed after a
	// successful original manager transaction with computed start sum ONE+1.
	s := managedScheduleFixture()
	s.PoolStateWord = managedScheduleWord(scheduleEntry("1767225661", 0), scheduleEntry("1767226662", 32), scheduleEntry("0", 64), scheduleEntry("0", 96), scheduleEntry("10000000000000000", 128), scheduleEntry("10000000000000000", 190))
	starts := []string{"499600399600399601", "300499500499500499", "199900099900099901"}
	ends := []string{"200000000000000000", "300000000000000000", "500000000000000000"}
	s.Tokens = nil
	for i := range starts {
		s.Tokens = append(s.Tokens, ManagedV2ScheduledToken{Token: fmt.Sprintf("0x%040x", i+1), StateWord: managedScheduleWord(scheduleEntry(starts[i], 0), scheduleEntry(ends[i], 64))})
	}
	if e := s.Validate(); e != nil {
		t.Fatal("legal computed start drift rejected", e)
	}
	v, e := s.At(1767225661)
	if e != nil {
		t.Fatal(e)
	}
	for i, w := range v.Weights {
		if w != Amount(starts[i]) {
			t.Fatal("computed start was normalized")
		}
	}
	sum := new(big.Int)
	for _, w := range v.Weights {
		sum.Add(sum, w.Big())
	}
	if sum.String() != "1000000000000000001" {
		t.Fatal(sum)
	}
	bad := s
	bad.Tokens = append([]ManagedV2ScheduledToken(nil), s.Tokens...)
	bad.Tokens[0].StateWord = managedScheduleWord(scheduleEntry(starts[0], 0), scheduleEntry("200000000000000001", 64))
	if e := bad.Validate(); e == nil {
		t.Fatal("caller target nonnormalized accepted")
	}
}
func TestManagedScheduleStrictJSONAndWordBounds(t *testing.T) {
	s := managedScheduleFixture()
	raw, _ := json.Marshal(s)
	for _, bad := range []string{`{}`, `{"pool_state_word":null,"tokens":[]}`, strings.Replace(string(raw), `"pool_state_word":`, `"Pool_State_Word":`, 1), strings.Replace(string(raw), `"token":`, `"token":null,"unexpected":`, 1), strings.Replace(string(raw), `"pool_state_word":`, `"pool_state_word":"0","pool_state_word":`, 1)} {
		var q ManagedV2Schedule
		if json.Unmarshal([]byte(bad), &q) == nil {
			t.Fatal("invalid JSON accepted", bad)
		}
	}
	for name, mutate := range map[string]func(*ManagedV2Schedule){
		"word overflow": func(s *ManagedV2Schedule) { s.PoolStateWord = Amount(new(big.Int).Lsh(big.NewInt(1), 256).String()) },
		"reserved token bits": func(s *ManagedV2Schedule) {
			s.Tokens[0].StateWord = Amount(new(big.Int).Or(s.Tokens[0].StateWord.Big(), new(big.Int).Lsh(big.NewInt(1), 133)).String())
		},
		"token count":     func(s *ManagedV2Schedule) { s.Tokens = s.Tokens[:1] },
		"duplicate token": func(s *ManagedV2Schedule) { s.Tokens[1].Token = s.Tokens[0].Token },
		"order":           func(s *ManagedV2Schedule) { s.Tokens[0], s.Tokens[1] = s.Tokens[1], s.Tokens[0] },
		"unknown amount":  func(s *ManagedV2Schedule) { s.Tokens[0].StateWord = "" },
	} {
		t.Run(name, func(t *testing.T) {
			q := s
			q.Tokens = append([]ManagedV2ScheduledToken(nil), s.Tokens...)
			mutate(&q)
			if q.Validate() == nil {
				t.Fatal("invalid packed source accepted")
			}
		})
	}
	if _, e := s.At(^uint64(0)); e == nil {
		t.Fatal("unrepresentable clock milliseconds accepted")
	}
}

func TestManagedScheduleAllFlagsAndEqualTimeTransitions(t *testing.T) {
	for flags := uint64(0); flags < 16; flags++ {
		s := managedScheduleFixture()
		lowMask := new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), 252), big.NewInt(1))
		word := new(big.Int).And(s.PoolStateWord.Big(), lowMask)
		word.Or(word, new(big.Int).Lsh(new(big.Int).SetUint64(flags), 252))
		s.PoolStateWord = Amount(word.String())
		view, e := s.At(1767225600)
		if e != nil {
			t.Fatal(e)
		}
		if view.SwapsEnabled != (flags&1 != 0) || view.MustAllowlistLPs != (flags&2 != 0) || view.RecoveryMode != (flags&4 != 0) || view.JoinExitEnabled != (flags&8 != 0) {
			t.Fatal("packed flags differ", flags, view)
		}
	}
	s := managedScheduleFixture()
	s.PoolStateWord = managedScheduleWord(scheduleEntry("42", 0), scheduleEntry("42", 32), scheduleEntry("42", 64), scheduleEntry("42", 96), scheduleEntry("10000000000000000", 128), scheduleEntry("30000000000000000", 190))
	before, e := s.At(41)
	if e != nil {
		t.Fatal(e)
	}
	at, e := s.At(42)
	if e != nil {
		t.Fatal(e)
	}
	if before.Weights[0] != "800000000000000000" || at.Weights[0] != "200000000000000000" || before.SwapFee != "10000000000000000" || at.SwapFee != "30000000000000000" {
		t.Fatal("equal-time schedule must jump to target exactly at end")
	}
}
func TestManagedScheduleOriginalRescheduledStartAndCurrentDrift(t *testing.T) {
	for _, c := range []struct {
		time, duration        uint64
		starts, ends, current []string
	}{
		{1767225661, 1001, []string{"499600399600399601", "300499500499500499", "199900099900099901"}, []string{"200000000000000000", "300000000000000000", "500000000000000000"}, []string{"499301098501897704", "300499001498002497", "200199900000099801"}},
		{1767226723, 1000, []string{"100399600399600399", "799500499500499501", "100099900099900099"}, []string{"400000000000000000", "400000000000000000", "200000000000000000"}, []string{"100699200799200798", "799100999000999002", "100199800199800198"}},
	} {
		s := managedScheduleFixture()
		s.PoolStateWord = managedScheduleWord(scheduleEntry(fmt.Sprint(c.time), 0), scheduleEntry(fmt.Sprint(c.time+c.duration), 32), scheduleEntry("0", 64), scheduleEntry("0", 96), scheduleEntry("10000000000000000", 128), scheduleEntry("10000000000000000", 190))
		s.Tokens = nil
		for i := range c.starts {
			s.Tokens = append(s.Tokens, ManagedV2ScheduledToken{Token: fmt.Sprintf("0x%040x", i+1), StateWord: managedScheduleWord(scheduleEntry(c.starts[i], 0), scheduleEntry(c.ends[i], 64))})
		}
		v, e := s.At(c.time + 1)
		if e != nil {
			t.Fatal(e)
		}
		for i, w := range v.Weights {
			if w != Amount(c.current[i]) {
				t.Fatalf("original reschedule view %d: %v", c.time, v.Weights)
			}
		}
	}
}
func TestManagedScheduleRawParametersRejectImpossibleOriginalValues(t *testing.T) {
	s := managedScheduleFixture()
	set := func(word Amount, value string, offset, width uint) Amount {
		n := word.Big()
		mask := new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), width), big.NewInt(1))
		mask.Lsh(mask, offset)
		n.AndNot(n, mask)
		v, _ := new(big.Int).SetString(value, 10)
		return Amount(n.Or(n, new(big.Int).Lsh(v, offset)).String())
	}
	for name, mutate := range map[string]func(*ManagedV2Schedule){
		"reversed weights": func(s *ManagedV2Schedule) { s.PoolStateWord = set(s.PoolStateWord, "1", 32, 32) },
		"reversed fee":     func(s *ManagedV2Schedule) { s.PoolStateWord = set(s.PoolStateWord, "1", 96, 32) },
		"fee below min":    func(s *ManagedV2Schedule) { s.PoolStateWord = set(s.PoolStateWord, "999999999999", 128, 62) },
		"fee above max":    func(s *ManagedV2Schedule) { s.PoolStateWord = set(s.PoolStateWord, "950000000000000001", 190, 62) },
		"decimals 19":      func(s *ManagedV2Schedule) { s.Tokens[0].StateWord = set(s.Tokens[0].StateWord, "19", 128, 5) },
		"startweight below min": func(s *ManagedV2Schedule) {
			s.Tokens[0].StateWord = set(s.Tokens[0].StateWord, "9999999999999999", 0, 64)
		},
		"endweight above one": func(s *ManagedV2Schedule) {
			s.Tokens[0].StateWord = set(s.Tokens[0].StateWord, "1000000000000000001", 64, 64)
		},
		"bad address": func(s *ManagedV2Schedule) { s.Tokens[0].Token = "0x1" },
	} {
		t.Run(name, func(t *testing.T) {
			q := s
			q.Tokens = append([]ManagedV2ScheduledToken(nil), s.Tokens...)
			mutate(&q)
			if q.Validate() == nil {
				t.Fatal("impossible schedule accepted")
			}
		})
	}
}
