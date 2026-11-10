package evm

import (
	"context"
	"encoding/hex"
	"fmt"
	"math/big"
	"testing"
	"xone/chain-parser-go/internal/domain"
)

func scheduledTestWord(value int64, offset uint) *big.Int {
	return new(big.Int).Lsh(big.NewInt(value), offset)
}
func scheduledTestSlot(token string) string {
	input := append(managedV2AddressWord(token), managedV2Word(big.NewInt(11))...)
	return "0x" + hex.EncodeToString(managedV2Hash(string(input)))
}
func scheduledNativeFixture(t *testing.T, mutate func(*managedTestState)) (Adapter, *managedTestState) {
	t.Helper()
	return managedNativeFixture(t, 0, func(s *managedTestState) {
		s.Pool.Protocol = domain.ManagedV2ScheduledProtocol
		start, e := quantity(s.Header["timestamp"].(string))
		if e != nil {
			t.Fatal(e)
		}
		end := start + 2592000
		word := new(big.Int)
		for _, v := range []struct {
			x      int64
			offset uint
		}{{int64(start), 0}, {int64(end), 32}, {int64(start), 64}, {int64(end), 96}, {10000000000000000, 128}, {30000000000000000, 190}, {1, 252}, {1, 255}} {
			word.Or(word, scheduledTestWord(v.x, v.offset))
		}
		s.Storage = map[string]string{s.Pool.ID + "0x9": "0x" + fmt.Sprintf("%064x", word)}
		values := []int64{800000000000000000, 200000000000000000}
		for i, token := range s.Pool.MultiAsset.Tokens {
			w := scheduledTestWord(values[i], 0)
			w.Or(w, scheduledTestWord(values[1-i], 64))
			s.Storage[s.Pool.ID+scheduledTestSlot(token.Token)] = "0x" + fmt.Sprintf("%064x", w)
		}
		u := func(value uint64) string { return fmt.Sprintf("%064x", value) }
		s.Calls[s.Pool.ID+managedV2Selector("getGradualSwapFeeUpdateParams()")] = "0x" + u(start) + u(end) + u(10000000000000000) + u(30000000000000000)
		s.Calls[s.Pool.ID+managedV2Selector("getGradualWeightUpdateParams()")] = "0x" + u(start) + u(end) + u(128) + u(224) + u(2) + u(uint64(values[0])) + u(uint64(values[1])) + u(2) + u(uint64(values[1])) + u(uint64(values[0]))
		if mutate != nil {
			mutate(s)
		}
	})
}
func TestManagedScheduledRawSourceDispatch(t *testing.T) {
	a, _ := scheduledNativeFixture(t, nil)
	_, pools, e := a.ReadState(context.Background(), "latest", false)
	if e != nil {
		t.Fatal("explicit scheduled source unavailable", e)
	}
	if len(pools) != 1 || pools[0].AdapterVersion != domain.ManagedV2ScheduledAdapterVersion || pools[0].MultiAsset.ManagedV2.Schedule == nil {
		t.Fatal("new profile/raw source state lost")
	}
	if e = pools[0].Validate(); e != nil {
		t.Fatal(e)
	}
}
func TestManagedScheduledSourceRejectsRawGetterContradictions(t *testing.T) {
	for name, mutate := range map[string]func(*managedTestState){
		"missing pool word": func(s *managedTestState) { delete(s.Storage, s.Pool.ID+"0x9") },
		"short word":        func(s *managedTestState) { s.Storage[s.Pool.ID+"0x9"] = "0x01" },
		"unknown token word": func(s *managedTestState) {
			delete(s.Storage, s.Pool.ID+scheduledTestSlot(s.Pool.MultiAsset.Tokens[0].Token))
		},
		"join flag": func(s *managedTestState) {
			value := new(big.Int)
			value.SetString(s.Storage[s.Pool.ID+"0x9"][2:], 16)
			value.Xor(value, new(big.Int).Lsh(big.NewInt(1), 255))
			s.Storage[s.Pool.ID+"0x9"] = "0x" + fmt.Sprintf("%064x", value)
		},
		"reserved token bits": func(s *managedTestState) {
			key := s.Pool.ID + scheduledTestSlot(s.Pool.MultiAsset.Tokens[0].Token)
			value := new(big.Int)
			value.SetString(s.Storage[key][2:], 16)
			value.Or(value, new(big.Int).Lsh(big.NewInt(1), 133))
			s.Storage[key] = "0x" + fmt.Sprintf("%064x", value)
		},
		"raw fee": func(s *managedTestState) {
			value := new(big.Int)
			value.SetString(s.Storage[s.Pool.ID+"0x9"][2:], 16)
			value.Xor(value, new(big.Int).Lsh(big.NewInt(1), 128))
			s.Storage[s.Pool.ID+"0x9"] = "0x" + fmt.Sprintf("%064x", value)
		},
	} {
		t.Run(name, func(t *testing.T) {
			a, _ := scheduledNativeFixture(t, mutate)
			_, pools, e := a.ReadState(context.Background(), "latest", false)
			if e == nil || pools != nil {
				t.Fatal("unqualified packed source published", e)
			}
		})
	}
}
