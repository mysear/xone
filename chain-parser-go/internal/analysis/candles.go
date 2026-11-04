// Package analysis is an isolated analytics role; it never writes online pool state.
package analysis

import (
	"errors"
	"fmt"
	"math/big"
	"sort"
	"sync"
	"xone/chain-parser-go/internal/domain"
)

type Trade struct {
	ID          string        `json:"id"`
	Chain       string        `json:"chain_key"`
	Pool        string        `json:"pool_id"`
	Block       string        `json:"block_hash"`
	TimestampMS uint64        `json:"timestamp_ms"`
	TxIndex     uint64        `json:"tx_index"`
	EventIndex  uint64        `json:"event_index"`
	Base        domain.Amount `json:"base_amount"`
	Quote       domain.Amount `json:"quote_amount"`
}
type Candle struct {
	Deleted     bool   `json:"deleted,omitempty"`
	Chain       string `json:"chain_key"`
	Pool        string `json:"pool_id"`
	WindowMS    uint64 `json:"window_ms"`
	Open        string `json:"open_rational"`
	High        string `json:"high_rational"`
	Low         string `json:"low_rational"`
	Close       string `json:"close_rational"`
	BaseVolume  string `json:"base_volume"`
	QuoteVolume string `json:"quote_volume"`
	Count       uint64 `json:"count"`
	Revision    uint64 `json:"revision"`
}
type Analyzer struct {
	mu       sync.Mutex
	interval uint64
	trades   map[string]Trade
	revision map[string]uint64
}

func New(interval uint64) *Analyzer {
	if interval == 0 {
		panic("interval must be positive")
	}
	return &Analyzer{interval: interval, trades: map[string]Trade{}, revision: map[string]uint64{}}
}
func (a *Analyzer) key(tr Trade) string {
	return windowKey(tr.Chain, tr.Pool, tr.TimestampMS/a.interval*a.interval)
}
func (a *Analyzer) Apply(tr Trade) error {
	if domain.ValidateChain(tr.Chain) != nil || tr.ID == "" || tr.Block == "" || tr.Pool == "" {
		return errors.New("INVALID_ANALYTICS_EVENT")
	}
	base, e := domain.ParseAmount(tr.Base.String())
	if e != nil || base.Big().Sign() == 0 {
		return errors.New("INVALID_BASE_AMOUNT")
	}
	if _, e = domain.ParseAmount(tr.Quote.String()); e != nil {
		return e
	}
	a.mu.Lock()
	defer a.mu.Unlock()
	id := tr.Chain + "/" + tr.ID
	if old, ok := a.trades[id]; ok {
		if old != tr {
			return errors.New("EVENT_ID_CONFLICT")
		}
		return nil
	}
	a.trades[id] = tr
	a.revision[a.key(tr)]++
	return nil
}
func (a *Analyzer) Revert(chain, block string) {
	a.mu.Lock()
	defer a.mu.Unlock()
	affected := map[string]bool{}
	for id, tr := range a.trades {
		if tr.Chain == chain && tr.Block == block {
			delete(a.trades, id)
			affected[a.key(tr)] = true
		}
	}
	for k := range affected {
		a.revision[k]++
	}
}
func (a *Analyzer) Candles() []Candle {
	a.mu.Lock()
	defer a.mu.Unlock()
	groups := map[string][]Trade{}
	for _, tr := range a.trades {
		groups[a.key(tr)] = append(groups[a.key(tr)], tr)
	}
	keys := make([]string, 0, len(groups))
	for k := range groups {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	out := make([]Candle, 0, len(keys))
	for _, k := range keys {
		events := groups[k]
		sort.Slice(events, func(i, j int) bool {
			l, r := events[i], events[j]
			if l.TimestampMS != r.TimestampMS {
				return l.TimestampMS < r.TimestampMS
			}
			if l.TxIndex != r.TxIndex {
				return l.TxIndex < r.TxIndex
			}
			if l.EventIndex != r.EventIndex {
				return l.EventIndex < r.EventIndex
			}
			return l.ID < r.ID
		})
		var high, low *big.Rat
		var open, close string
		base, quote := new(big.Int), new(big.Int)
		for i, tr := range events {
			p := new(big.Rat).SetFrac(tr.Quote.Big(), tr.Base.Big())
			if i == 0 {
				open = p.RatString()
			}
			close = p.RatString()
			if high == nil || p.Cmp(high) > 0 {
				high = new(big.Rat).Set(p)
			}
			if low == nil || p.Cmp(low) < 0 {
				low = new(big.Rat).Set(p)
			}
			base.Add(base, tr.Base.Big())
			quote.Add(quote, tr.Quote.Big())
		}
		first := events[0]
		out = append(out, Candle{Chain: first.Chain, Pool: first.Pool, WindowMS: first.TimestampMS / a.interval * a.interval, Open: open, Close: close, High: high.RatString(), Low: low.RatString(), BaseVolume: base.String(), QuoteVolume: quote.String(), Count: uint64(len(events)), Revision: a.revision[k]})
	}
	return out
}

func windowKey(chain, pool string, window uint64) string {
	return fmt.Sprintf("%s/%s/%d", chain, pool, window)
}
