package ingest

import (
	"context"
	"encoding/binary"
	"encoding/hex"
	"errors"
	"math/big"
	"sort"
	"strings"
	"sync"
	"sync/atomic"
	"time"
)

// Original ABI sources, @uniswap/v2-core and @uniswap/v3-core 1.0.1:
// contracts/interfaces/IUniswapV2Factory.sol SHA256
// 51d056199e3f5e41cb1a9f11ce581aa3e190cc982db5771ffeef8d8d1f962a0d
// contracts/interfaces/IUniswapV3Factory.sol SHA256
// 17c72e89a7d0eecca7929ca08d97f46e80930ef7024e4ccbc7b294c588477107
// ABI identity is not deployed-code provenance; VerifySource must prove that.
const (
	UniswapV2Factory       = "uniswap-v2"
	UniswapV3Factory       = "uniswap-v3"
	V2PairCreatedSignature = "PairCreated(address,address,address,uint256)"
	V3PoolCreatedSignature = "PoolCreated(address,address,uint24,int24,address)"
	V2PairCreatedTopic     = "0x0d3648bd0f6ba80134a33ba9275ac585d9d315f0ad8355cddefde31afa28d0e9"
	V3PoolCreatedTopic     = "0x783cca1c0412dd0d695e784568c96da2e9c22ff989357a2e8b1d9b2b4e6b7118"
)

var (
	ErrFactorySource    = errors.New("INGEST_FACTORY_SOURCE")
	ErrFactoryEvent     = errors.New("INGEST_FACTORY_EVENT")
	ErrFactoryCandidate = errors.New("INGEST_FACTORY_CANDIDATE")
	ErrFactoryAdmission = errors.New("INGEST_FACTORY_CURRENT_ADMISSION")
	ErrFactoryStale     = errors.New("INGEST_FACTORY_STALE_ANCHOR")
	ErrFactoryBusy      = errors.New("INGEST_FACTORY_ACTOR_RUNNING")
)

type FactorySource struct {
	Chain        string
	Address      string
	Version      string
	ProvenanceID string
	Since        uint64
	Until        uint64
}
type FactoryLog struct {
	Address          string
	BlockHash        string
	BlockNumber      uint64
	TransactionHash  string
	TransactionIndex uint64
	LogIndex         uint64
	Removed          bool
	Topics           []string
	Data             string
}
type FactoryReceipt struct {
	BlockHash        string
	BlockNumber      uint64
	TransactionHash  string
	TransactionIndex uint64
	Logs             []FactoryLog
}

// FactoryBlock is the complete receipt/log set for one fixed, validated HTTP
// block. VerifyBlock must bind these fields to Raw and the receipts proof.
type FactoryBlock struct {
	Chain    string
	Anchor   Cursor
	Raw      RawRef
	Receipts []FactoryReceipt
}
type FactoryCandidate struct {
	ID               string
	Chain            string
	Factory          string
	Version          string
	ProvenanceID     string
	Height           uint64
	BlockHash        string
	TransactionHash  string
	TransactionIndex uint64
	LogIndex         uint64
	Pool             string
	Token0           string
	Token1           string
	Ordinal          string
	Fee              uint32
	TickSpacing      int32
	Raw              RawRef
}
type CatalogNamespace string

const (
	CatalogOnline  CatalogNamespace = "online"
	CatalogHistory CatalogNamespace = "history"
)

type CatalogAnchor struct {
	Chain      string
	Height     uint64
	Hash       string
	Generation uint64
}
type PoolAdmission struct {
	Candidate   FactoryCandidate
	At          CatalogAnchor
	StateDigest string
}
type CatalogBatch struct {
	Namespace CatalogNamespace
	Anchor    CatalogAnchor
	Entries   []PoolAdmission
}
type FactoryLimits struct {
	Timeout       time.Duration
	MaxReceipts   int
	MaxLogs       int
	MaxBytes      int64
	MaxCandidates int
	MaxPools      int
}
type FactoryOptions struct {
	// FilterSources selects only configured address/height ranges from a complete
	// verified block. All receipts/logs still pass location and integrity checks.
	// The default remains strict for callers supplying a claimed creation source.
	FilterSources bool
	Sources       []FactorySource
	Archive       *Archive
	Limits        FactoryLimits
	VerifyBlock   func(context.Context, FactoryBlock) error
	VerifySource  func(context.Context, FactorySource, Cursor) error
	VerifyCurrent func(context.Context, CatalogAnchor) error
	// ReadCurrent must prove deployed pool identity, factory membership, tokens,
	// version/fee and required state at this verified current HTTP anchor. An
	// event, historical snapshot or caller-provided config is not admission.
	ReadCurrent func(context.Context, FactoryCandidate, CatalogAnchor) (PoolAdmission, error)
	// Publish must atomically publish the entire batch in its explicit namespace,
	// using Anchor.Generation for fencing. On an ambiguous external commit error,
	// rebuild/reconcile this catalog from the durable owner before retrying.
	Publish func(context.Context, CatalogBatch) error
}
type FactoryDiscovery struct {
	busy     atomic.Bool
	mu       sync.Mutex
	options  FactoryOptions
	sources  map[string]FactorySource
	verified map[string]FactoryCandidate
	online   map[string]PoolAdmission
	history  map[string]PoolAdmission
	floors   map[string]CatalogAnchor
	revoked  map[string]bool
}

func NewFactoryDiscovery(options FactoryOptions) (*FactoryDiscovery, error) {
	l := options.Limits
	if options.Archive == nil || l.Timeout <= 0 || l.MaxReceipts <= 0 || l.MaxLogs <= 0 || l.MaxBytes <= 0 || l.MaxCandidates <= 0 || l.MaxPools <= 0 || options.VerifyBlock == nil || options.VerifySource == nil || options.VerifyCurrent == nil || options.ReadCurrent == nil || options.Publish == nil || len(options.Sources) == 0 {
		return nil, errors.New("INGEST_FACTORY_OPTIONS_INVALID")
	}
	d := &FactoryDiscovery{options: options, sources: map[string]FactorySource{}, verified: map[string]FactoryCandidate{}, online: map[string]PoolAdmission{}, history: map[string]PoolAdmission{}, floors: map[string]CatalogAnchor{}, revoked: map[string]bool{}}
	for _, s := range options.Sources {
		if s.Chain == "" || !factoryAddress(s.Address) || s.ProvenanceID == "" || (s.Version != UniswapV2Factory && s.Version != UniswapV3Factory) || s.Until != 0 && s.Until < s.Since {
			return nil, ErrFactorySource
		}
		s.Address = strings.ToLower(s.Address)
		key := s.Chain + ":" + s.Address
		if _, exists := d.sources[key]; exists {
			return nil, ErrFactorySource
		}
		d.sources[key] = s
	}
	return d, nil
}
func factoryAddress(s string) bool {
	if len(s) != 42 || !strings.HasPrefix(s, "0x") || s == "0x"+strings.Repeat("0", 40) {
		return false
	}
	_, err := hex.DecodeString(s[2:])
	return err == nil
}
func wordAddress(word []byte) (string, error) {
	if len(word) != 32 {
		return "", ErrFactoryEvent
	}
	for _, b := range word[:12] {
		if b != 0 {
			return "", ErrFactoryEvent
		}
	}
	address := "0x" + hex.EncodeToString(word[12:])
	if !factoryAddress(address) {
		return "", ErrFactoryEvent
	}
	return address, nil
}
func sameFactoryEvent(a, b FactoryCandidate) bool { a.Raw = RawRef{}; b.Raw = RawRef{}; return a == b }

func factoryPoolKey(c FactoryCandidate) string { return c.Chain + ":" + c.Version + ":" + c.Pool }

// Mutation is a non-queued actor; callbacks run without holding the snapshot
// mutex, so publishers may inspect the prior complete directory safely.
func (d *FactoryDiscovery) enter(ctx context.Context) error {
	if ctx.Err() != nil {
		return ctx.Err()
	}
	if !d.busy.CompareAndSwap(false, true) {
		return ErrFactoryBusy
	}
	return nil
}

func (d *FactoryDiscovery) Discover(ctx context.Context, b FactoryBlock) ([]FactoryCandidate, error) {
	if err := d.enter(ctx); err != nil {
		return nil, err
	}
	defer d.busy.Store(false)
	ctx, cancel := context.WithTimeout(ctx, d.options.Limits.Timeout)
	defer cancel()
	if ctx.Err() != nil {
		return nil, ctx.Err()
	}
	l := d.options.Limits
	if d.revoked[b.Chain+":"+strings.ToLower(b.Anchor.Hash)] {
		return nil, ErrFactoryStale
	}
	if l.MaxBytes <= 0 || l.MaxLogs <= 0 || len(b.Receipts) > l.MaxReceipts || b.Raw.Bytes < 0 || b.Raw.Bytes > l.MaxBytes {
		return nil, ErrBudget
	}
	if b.Chain == "" || !validHeadHash(b.Anchor.Hash) || b.Raw.Owner.Chain != b.Chain || b.Raw.Owner.Height != b.Anchor.Height || !strings.EqualFold(b.Raw.Owner.Hash, b.Anchor.Hash) {
		return nil, ErrFactoryEvent
	}
	if _, err := d.options.Archive.Read(b.Raw); err != nil {
		return nil, err
	}
	bytesLeft := l.MaxBytes - b.Raw.Bytes
	logCount := 0
	positions := map[uint64]FactoryLog{}
	receipts := map[uint64]bool{}
	transactions := map[string]bool{}
	for _, r := range b.Receipts {
		if ctx.Err() != nil {
			return nil, ctx.Err()
		}
		if r.BlockNumber != b.Anchor.Height || !strings.EqualFold(r.BlockHash, b.Anchor.Hash) || !validHeadHash(r.TransactionHash) || r.TransactionIndex >= uint64(len(b.Receipts)) || receipts[r.TransactionIndex] || transactions[strings.ToLower(r.TransactionHash)] || r.Logs == nil {
			return nil, ErrFactoryEvent
		}
		receipts[r.TransactionIndex] = true
		transactions[strings.ToLower(r.TransactionHash)] = true
		for _, log := range r.Logs {
			if ctx.Err() != nil {
				return nil, ctx.Err()
			}
			if len(log.Topics) > 4 {
				return nil, ErrFactoryEvent
			}
			logCount++
			if logCount > l.MaxLogs {
				return nil, ErrBudget
			}
			cost := int64(len(log.Address) + len(log.BlockHash) + len(log.TransactionHash) + len(log.Data) + 64)
			for _, topic := range log.Topics {
				cost += int64(len(topic))
			}
			if cost > bytesLeft {
				return nil, ErrBudget
			}
			bytesLeft -= cost
			if log.Removed || log.BlockNumber != b.Anchor.Height || !strings.EqualFold(log.BlockHash, b.Anchor.Hash) || !strings.EqualFold(log.TransactionHash, r.TransactionHash) || log.TransactionIndex != r.TransactionIndex || !factoryAddress(log.Address) || len(log.Topics) > 4 || log.Topics == nil || !strings.HasPrefix(log.Data, "0x") || len(log.Data)%2 != 0 {
				return nil, ErrFactoryEvent
			}
			if _, err := hex.DecodeString(log.Data[2:]); err != nil {
				return nil, ErrFactoryEvent
			}
			normalized := log
			normalized.Address = strings.ToLower(log.Address)
			normalized.BlockHash = strings.ToLower(log.BlockHash)
			normalized.TransactionHash = strings.ToLower(log.TransactionHash)
			normalized.Data = strings.ToLower(log.Data)
			normalized.Topics = append([]string{}, log.Topics...)
			for i, topic := range log.Topics {
				if !validHeadHash(topic) {
					return nil, ErrFactoryEvent
				}
				normalized.Topics[i] = strings.ToLower(topic)
			}
			if previous, exists := positions[log.LogIndex]; exists {
				if jsonID(previous) != jsonID(normalized) {
					return nil, ErrFactoryEvent
				}
				continue
			}
			positions[log.LogIndex] = normalized
		}
	}
	ordered := make([]FactoryLog, 0, len(positions))
	for _, log := range positions {
		ordered = append(ordered, log)
	}
	sort.Slice(ordered, func(i, j int) bool { return ordered[i].LogIndex < ordered[j].LogIndex })
	for i, log := range ordered {
		if log.LogIndex != uint64(i) || i > 0 && log.TransactionIndex < ordered[i-1].TransactionIndex {
			return nil, ErrFactoryEvent
		}
	}
	if err := d.options.VerifyBlock(ctx, b); err != nil {
		return nil, err
	}
	checked := map[string]bool{}
	var candidates []FactoryCandidate
	for _, log := range ordered {
		if ctx.Err() != nil {
			return nil, ctx.Err()
		}
		if len(log.Topics) == 0 || log.Topics[0] != V2PairCreatedTopic && log.Topics[0] != V3PoolCreatedTopic {
			continue
		}
		if log.Removed {
			return nil, ErrFactoryEvent
		}
		source, exists := d.sources[b.Chain+":"+log.Address]
		if !exists || b.Anchor.Height < source.Since || source.Until != 0 && b.Anchor.Height > source.Until {
			if d.options.FilterSources {
				continue
			}
			return nil, ErrFactorySource
		}
		if !checked[source.Address] {
			if err := d.options.VerifySource(ctx, source, b.Anchor); err != nil {
				return nil, err
			}
			checked[source.Address] = true
		}
		candidate, err := decodeFactoryEvent(b, source, log)
		if err != nil {
			return nil, err
		}
		candidates = append(candidates, candidate)
		if len(candidates) > l.MaxCandidates {
			return nil, ErrBudget
		}
	}
	added := 0
	for i, c := range candidates {
		if previous, exists := d.verified[c.ID]; exists {
			if !sameFactoryEvent(previous, c) {
				return nil, ErrFactoryCandidate
			}
			// A failover re-observation keeps the first durable evidence reference.
			// Raw ownership remains separate in Archive; it is not event identity.
			candidates[i] = previous
		} else {
			added++
		}
	}
	if added > l.MaxCandidates-len(d.verified) {
		return nil, ErrBudget
	}
	if ctx.Err() != nil {
		return nil, ctx.Err()
	}
	for _, c := range candidates {
		d.verified[c.ID] = c
	}
	return candidates, nil
}
func decodeFactoryEvent(b FactoryBlock, s FactorySource, log FactoryLog) (FactoryCandidate, error) {
	c := FactoryCandidate{Chain: b.Chain, Factory: s.Address, Version: s.Version, ProvenanceID: s.ProvenanceID, Height: b.Anchor.Height, BlockHash: strings.ToLower(b.Anchor.Hash), TransactionHash: log.TransactionHash, TransactionIndex: log.TransactionIndex, LogIndex: log.LogIndex, Raw: b.Raw}
	expected := V2PairCreatedTopic
	topics := 3
	if s.Version == UniswapV3Factory {
		expected = V3PoolCreatedTopic
		topics = 4
	}
	if log.Topics[0] != expected || len(log.Topics) != topics {
		return c, ErrFactoryEvent
	}
	token0, _ := hex.DecodeString(log.Topics[1][2:])
	token1, _ := hex.DecodeString(log.Topics[2][2:])
	var err error
	c.Token0, err = wordAddress(token0)
	if err != nil {
		return c, err
	}
	c.Token1, err = wordAddress(token1)
	if err != nil || c.Token0 >= c.Token1 {
		return c, ErrFactoryEvent
	}
	data, err := hex.DecodeString(log.Data[2:])
	if err != nil || len(data) != 64 {
		return c, ErrFactoryEvent
	}
	if s.Version == UniswapV2Factory {
		c.Pool, err = wordAddress(data[:32])
		ordinal := new(big.Int).SetBytes(data[32:])
		if err != nil || ordinal.Sign() <= 0 {
			return c, ErrFactoryEvent
		}
		c.Ordinal = ordinal.String()
	} else {
		feeWord, _ := hex.DecodeString(log.Topics[3][2:])
		for _, v := range feeWord[:29] {
			if v != 0 {
				return c, ErrFactoryEvent
			}
		}
		c.Fee = binary.BigEndian.Uint32(feeWord[28:])
		for _, v := range data[:29] {
			if v != 0 {
				return c, ErrFactoryEvent
			}
		}
		c.TickSpacing = int32(binary.BigEndian.Uint32(data[28:32]))
		c.Pool, err = wordAddress(data[32:])
		if err != nil || c.Fee == 0 || c.Fee >= 1_000_000 || c.TickSpacing <= 0 || c.TickSpacing >= 16384 {
			return c, ErrFactoryEvent
		}
	}
	identity := c
	identity.Raw = RawRef{}
	c.ID = jsonID(identity)
	return c, nil
}

// Admit publishes only candidates produced by this discovery instance. It
// stages all current-state reads before one atomic Publish, then updates its
// bounded local directory. History has a physically separate map/namespace.
func (d *FactoryDiscovery) Admit(ctx context.Context, namespace CatalogNamespace, anchor CatalogAnchor, candidates []FactoryCandidate) error {
	if err := d.enter(ctx); err != nil {
		return err
	}
	defer d.busy.Store(false)
	ctx, cancel := context.WithTimeout(ctx, d.options.Limits.Timeout)
	defer cancel()
	if ctx.Err() != nil {
		return ctx.Err()
	}
	if namespace != CatalogOnline && namespace != CatalogHistory || anchor.Chain == "" || !validHeadHash(anchor.Hash) || anchor.Generation == 0 {
		return ErrFactoryAdmission
	}
	if len(candidates) > d.options.Limits.MaxCandidates {
		return ErrBudget
	}
	if d.revoked[anchor.Chain+":"+strings.ToLower(anchor.Hash)] {
		return ErrFactoryStale
	}
	if err := d.options.VerifyCurrent(ctx, anchor); err != nil {
		return err
	}
	directory := d.online
	if namespace == CatalogHistory {
		directory = d.history
	}
	floorKey := string(namespace) + ":" + anchor.Chain
	floor, hasFloor := d.floors[floorKey]
	if hasFloor && (anchor.Generation < floor.Generation || anchor.Generation == floor.Generation && (anchor.Height < floor.Height || anchor.Height == floor.Height && !strings.EqualFold(anchor.Hash, floor.Hash))) {
		return ErrFactoryStale
	}
	// A new anchor cannot inherit stale numeric admissions for unrefreshed pools.
	reset := hasFloor && anchor != floor
	seen := map[string]FactoryCandidate{}
	staged := make([]PoolAdmission, 0, len(candidates))
	added := 0
	for _, candidate := range candidates {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		if verified, ok := d.verified[candidate.ID]; !ok || verified != candidate || candidate.Chain != anchor.Chain || candidate.Height > anchor.Height {
			return ErrFactoryCandidate
		}
		key := factoryPoolKey(candidate)
		if previous, ok := seen[key]; ok {
			if previous != candidate {
				return ErrFactoryCandidate
			}
			continue
		}
		seen[key] = candidate
		if previous, exists := directory[key]; exists && !reset {
			if previous.Candidate != candidate {
				return ErrFactoryCandidate
			}
			if anchor.Generation < previous.At.Generation || anchor.Generation == previous.At.Generation && (anchor.Height < previous.At.Height || anchor.Height == previous.At.Height && !strings.EqualFold(anchor.Hash, previous.At.Hash)) {
				return ErrFactoryStale
			}
			if previous.At == anchor {
				continue
			}
		} else {
			added++
		}
		admission, err := d.options.ReadCurrent(ctx, candidate, anchor)
		if err != nil {
			return err
		}
		if admission.Candidate != candidate || admission.At != anchor || !validHeadHash(admission.StateDigest) {
			return ErrFactoryAdmission
		}
		staged = append(staged, admission)
	}
	remaining := d.options.Limits.MaxPools - len(d.online) - len(d.history)
	if reset {
		for _, entry := range directory {
			if entry.Candidate.Chain == anchor.Chain {
				remaining++
			}
		}
	}
	if added > remaining {
		return ErrBudget
	}
	if ctx.Err() != nil {
		return ctx.Err()
	}
	if len(staged) == 0 {
		return nil
	}
	if err := d.options.VerifyCurrent(ctx, anchor); err != nil {
		return err
	}
	if ctx.Err() != nil {
		return ctx.Err()
	}
	// A detached slice prevents a publisher from mutating the local staged batch.
	batch := CatalogBatch{Namespace: namespace, Anchor: anchor, Entries: append([]PoolAdmission(nil), staged...)}
	if err := d.options.Publish(ctx, batch); err != nil {
		return err
	}
	d.mu.Lock()
	if reset {
		for key, admission := range directory {
			if admission.Candidate.Chain == anchor.Chain {
				delete(directory, key)
			}
		}
	}
	d.floors[floorKey] = anchor
	for _, admission := range staged {
		directory[factoryPoolKey(admission.Candidate)] = admission
	}
	d.mu.Unlock()
	return nil
}
func (d *FactoryDiscovery) Snapshot(namespace CatalogNamespace) []PoolAdmission {
	d.mu.Lock()
	defer d.mu.Unlock()
	directory := d.online
	if namespace == CatalogHistory {
		directory = d.history
	} else if namespace != CatalogOnline {
		return nil
	}
	keys := make([]string, 0, len(directory))
	for key := range directory {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	entries := make([]PoolAdmission, 0, len(keys))
	for _, key := range keys {
		entries = append(entries, directory[key])
	}
	return entries
}
