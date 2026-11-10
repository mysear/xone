package evm

import (
	"context"
	"errors"
	"strings"
	"sync"

	"xone/chain-parser-go/internal/config"
)

type nativeReadSession struct {
	mu        sync.Mutex
	ctx       context.Context
	limits    ReceiptProofLimits
	remaining int
}

// NativeRuntimeLimits is shared by normal, factory and history readers. The
// optional config section belongs to config, not the chain codec package.
func NativeRuntimeLimits(chain config.Chain) (ReceiptProofLimits, error) {
	var limits ReceiptProofLimits
	if chain.NativeProof == nil {
		return limits, errors.New("NATIVE_PROOF_CONFIG_REQUIRED")
	}
	p := chain.NativeProof
	limits = ReceiptProofLimits{
		MaxBytes: p.MaxBytes, MaxHeaders: p.MaxHeaders,
		MaxWindowBlocks: p.MaxWindowBlocks, MaxTransactions: p.MaxTransactions,
		MaxLookahead: p.MaxLookahead,
	}
	return limits, limits.validate()
}
func (s *nativeReadSession) available() (ReceiptProofLimits, error) {
	l := s.limits
	l.MaxBytes = s.remaining
	if l.MaxBytes <= 0 {
		return l, errors.New("NATIVE_RUNTIME_PROOF_BUDGET")
	}
	return l, nil
}
func (s *nativeReadSession) consume(bytes int) error {
	if bytes < 0 || bytes > s.remaining {
		return errors.New("NATIVE_RUNTIME_PROOF_BUDGET")
	}
	s.remaining -= bytes
	return nil
}

func (a *Adapter) nativeRuntimeBlock(ctx context.Context, tag string) (RawBlock, error) {
	var empty RawBlock
	limits, err := NativeRuntimeLimits(a.Chain)
	if err != nil {
		return empty, err
	}
	session := &nativeReadSession{ctx: ctx, limits: limits, remaining: limits.MaxBytes}
	var proof ChainBlockBodyProof
	if strings.HasPrefix(tag, "0x") {
		proof, err = a.ReadNativeBlockProof(ctx, tag, limits)
	} else {
		proof, err = a.ReadNativeSettledBlockProof(ctx, tag, limits)
	}
	if err != nil {
		return empty, err
	}
	if err = session.consume(proof.ProofBytesUsed); err != nil {
		return empty, err
	}
	block := proof.Block
	block.NativeBodyProof = &proof
	block.ObservedHead = proof.ObservedHead
	block.nativeSession = session
	block.nativeContext = ctx
	return block, nil
}

// ReadQualifiedReceipts is the normal entry point. V2 always carries typed
// coverage and archive evidence; it never falls through to own-header roots or
// legacy witnesses after a successor/system-isolation proof has succeeded.
func (a *Adapter) ReadQualifiedReceipts(ctx context.Context, block RawBlock, requireState bool) (RawBlock, []Receipt, error) {
	if a.Chain.BlockIntegrity != ChainNativeCommitmentsV2 {
		receipts, err := a.ReadReceiptsWithMode(ctx, block, a.Chain.ReceiptsMode)
		if err != nil {
			return block, nil, err
		}
		if err = VerifyChainReceiptRoot(ctx, a.Chain.Key, block, receipts); err != nil {
			return block, nil, err
		}
		if err = a.verifyReceiptWitnesses(ctx, block, receipts); err != nil {
			return block, nil, err
		}
		return block, receipts, nil
	}
	if block.NativeBodyProof == nil || block.nativeSession == nil || block.nativeSession.ctx != ctx {
		return block, nil, errors.New("NATIVE_NORMAL_BODY_PROOF_REQUIRED")
	}
	session := block.nativeSession
	session.mu.Lock()
	defer session.mu.Unlock()
	limits, err := session.available()
	if err != nil {
		return block, nil, err
	}
	coverage, err := a.readNativeReceiptCoverageCached(ctx, block, a.Chain.ReceiptsMode, limits, block.NativeBodyProof)
	if err != nil {
		return block, nil, err
	}
	if err = session.consume(coverage.ProofBytesUsed); err != nil {
		return block, nil, err
	}
	addresses := nativeBusinessAddresses(a.Chain, coverage.Block)
	if len(addresses) > 0 {
		if _, err = coverage.SelectBusinessLogs(addresses, CanonicalTransactionEvents); err != nil {
			return block, nil, err
		}
	}
	limits, err = session.available()
	if err != nil {
		return block, nil, err
	}
	witnessBytes, err := a.VerifyNativeReceiptWitnessesUsage(ctx, coverage, a.Chain.ReceiptsMode, limits)
	if err != nil {
		return block, nil, err
	}
	if err = session.consume(witnessBytes); err != nil {
		return block, nil, err
	}
	state, stateErr := coverage.TargetStateAnchor()
	if stateErr != nil && requireState {
		return block, nil, stateErr
	}
	block.NativeCoverage = &coverage
	block.CurrentStateAnchor = state
	block.nativeContext = ctx
	return block, coverage.CanonicalReceipts, nil
}
func nativeBusinessAddresses(chain config.Chain, block RawBlock) []string {
	seen := map[string]bool{}
	add := func(address string) {
		if receiptHex(address, 20) {
			seen[strings.ToLower(address)] = true
		}
	}
	for _, pool := range chain.Pools {
		add(pool.ID)
		if pool.Weighted != nil {
			add(pool.Weighted.ExpectedVault)
		}
		if pool.Stable != nil {
			add(pool.Stable.ExpectedVault)
			add(pool.Stable.ExpectedFactory)
		}
		if pool.SingletonCL != nil {
			add(pool.SingletonCL.Manager)
		}
		if pool.SingletonAdvanced != nil {
			add(pool.SingletonAdvanced.Manager)
		}
		if pool.V2 != nil {
			add(pool.V2.ExpectedFactory)
		}
		if pool.V3 != nil {
			add(pool.V3.ExpectedFactory)
		}
	}
	height, _ := canonicalIndex(block.Number)
	if chain.DynamicCatalog != nil {
		for _, source := range chain.DynamicCatalog.Sources {
			if height >= source.Since && (source.Until == 0 || height <= source.Until) {
				add(source.Address)
			}
		}
	}
	out := make([]string, 0, len(seen))
	for address := range seen {
		out = append(out, address)
	}
	return out
}
func (a *Adapter) recheckBlock(ctx context.Context, block RawBlock) (RawBlock, error) {
	if a.Chain.BlockIntegrity != ChainNativeCommitmentsV2 {
		return a.Block(ctx, block.Number)
	}
	if block.nativeSession == nil || block.nativeSession.ctx != ctx {
		return RawBlock{}, errors.New("NATIVE_NORMAL_BODY_PROOF_REQUIRED")
	}
	session := block.nativeSession
	session.mu.Lock()
	defer session.mu.Unlock()
	limits, err := session.available()
	if err != nil {
		return RawBlock{}, err
	}
	proof, err := a.ReadNativeBlockProof(ctx, block.Number, limits)
	if err != nil {
		return RawBlock{}, err
	}
	if err = session.consume(proof.ProofBytesUsed); err != nil {
		return RawBlock{}, err
	}
	// Only SAE has successor work after the final body proof. Own-header
	// completion may use the last byte without requiring a budget for a no-op.
	if block.NativeCoverage != nil && block.NativeCoverage.Evidence != nil && block.NativeCoverage.Evidence.SAE != nil {
		limits, err = session.available()
		if err != nil {
			return RawBlock{}, err
		}
		used, err := a.recheckNativeSettling(ctx, *block.NativeCoverage, limits)
		if err != nil {
			return RawBlock{}, err
		}
		if err = session.consume(used); err != nil {
			return RawBlock{}, err
		}
	}
	return proof.Block, nil
}
func (b RawBlock) NativeReadContext() context.Context { return b.nativeContext }

// VerifyNativeBusinessCoverage binds replay qualification to the current closed
// pool/factory scope without reclassifying uncommitted system observations.
func VerifyNativeBusinessCoverage(chain config.Chain, coverage VerifiedReceiptCoverage) error {
	addresses := nativeBusinessAddresses(chain, coverage.Block)
	if len(addresses) == 0 {
		return nil
	}
	_, err := coverage.SelectBusinessLogs(addresses, CanonicalTransactionEvents)
	return err
}

// RecheckQualifiedBlock is the final canonical check for normal Factory reads.
// It shares the original body/receipt/witness budget and settling authority.
func (a *Adapter) RecheckQualifiedBlock(ctx context.Context, block RawBlock) error {
	check, err := a.recheckBlock(ctx, block)
	if err != nil {
		return err
	}
	if !sameReceiptAnchor(block, check) {
		return errors.New("ANCHOR_REORGED")
	}
	return ctx.Err()
}

// RecheckNativeReceiptEvidence attaches archived, fully verified inputs to a
// fresh native body session, then checks target and successor canonicality. The
// original read budget covers evidence verification and both final anchors.
func (a *Adapter) RecheckNativeReceiptEvidence(ctx context.Context, block RawBlock, evidence NativeReceiptEvidence) error {
	if a.Chain.BlockIntegrity != ChainNativeCommitmentsV2 || block.nativeSession == nil || block.nativeSession.ctx != ctx {
		return errors.New("NATIVE_NORMAL_BODY_PROOF_REQUIRED")
	}
	session := block.nativeSession
	session.mu.Lock()
	limits, err := session.available()
	if err != nil {
		session.mu.Unlock()
		return err
	}
	coverage, err := VerifyNativeReceiptEvidence(ctx, a.Chain.Key, block, evidence, limits)
	if err == nil {
		err = session.consume(coverage.ProofBytesUsed)
	}
	session.mu.Unlock()
	if err != nil {
		return err
	}
	if err = VerifyNativeBusinessCoverage(a.Chain, coverage); err != nil {
		return err
	}
	block.NativeCoverage = &coverage
	return a.RecheckQualifiedBlock(ctx, block)
}

func (a *Adapter) recheckNativeSettling(ctx context.Context, coverage VerifiedReceiptCoverage, limits ReceiptProofLimits) (used int, err error) {
	if coverage.Evidence == nil || coverage.Evidence.SAE == nil {
		return 0, nil
	}
	headers := coverage.Evidence.SAE.Headers
	if len(headers) < 2 {
		return 0, errors.New("SAE_PROOF_RANGE_INVALID")
	}
	reader, err := newNativeProofReader(ctx, a, limits)
	if err != nil {
		return 0, err
	}
	defer reader.cancel()
	defer func() { used = limits.MaxBytes - reader.budget.remaining }()
	if err = reader.budget.charge(2*len(headers[len(headers)-1]) + 256); err != nil {
		return 0, err
	}
	expected, err := VerifyChainHeaderJSON(reader.ctx, a.Chain.Key, headers[len(headers)-1])
	if err != nil {
		return 0, err
	}
	raw, err := reader.call("eth_getBlockByNumber", []any{expected.Block.Number, false})
	if err != nil {
		return 0, err
	}
	if err = reader.budget.charge(len(raw) + 256); err != nil {
		return 0, err
	}
	actual, err := VerifyChainHeaderJSON(reader.ctx, a.Chain.Key, raw)
	if err != nil {
		return 0, err
	}
	if !sameReceiptAnchor(expected.Block, actual.Block) {
		return 0, errors.New("SAE_SETTLING_ANCHOR_CHANGED")
	}
	return 0, reader.ctx.Err()
}

// ReadNativeContractAccountProof consumes a normal native block's shared read
// session. The exact receipt/state authority is reverified before a logical
// contract key may be mapped to the physical eth_getProof request.
func (a *Adapter) ReadNativeContractAccountProof(ctx context.Context, block RawBlock, address string, logicalKeys []string) (out VerifiedContractAccountCommitment, err error) {
	if err = ctx.Err(); err != nil {
		return out, err
	}
	if a.Chain.BlockIntegrity != ChainNativeCommitmentsV2 || block.nativeSession == nil || block.nativeSession.ctx != ctx || block.NativeCoverage == nil || block.NativeCoverage.Evidence == nil {
		return out, errors.New("NATIVE_NORMAL_BODY_PROOF_REQUIRED")
	}
	s := block.nativeSession
	s.mu.Lock()
	defer s.mu.Unlock()
	limits, err := s.available()
	if err != nil {
		return out, err
	}
	// Validate untrusted request shape/encoding before any proof IO.
	preflight := receiptProofBudget{remaining: limits.MaxBytes}
	if block.NativeCoverage.StateAnchor == nil {
		return out, errors.New("EXECUTION_STATE_TARGET_NOT_DIRECTLY_SETTLED")
	}
	plan, err := planContractStorage(ctx, ContractStorageSource{ChainKey: a.Chain.Key, GenesisHash: a.Chain.Genesis}, *block.NativeCoverage.StateAnchor, logicalKeys, &preflight)
	if err != nil {
		return out, err
	}
	if !receiptHex(address, 20) {
		return out, errors.New("STATE_PROOF_REQUEST_INVALID")
	}
	// After proof work starts, failure terminates this session: unknown partial
	// constructor/retry costs must never be refunded into a reusable read scope.
	defer func() {
		if err != nil {
			s.remaining = 0
		}
	}()
	planCost := limits.MaxBytes - preflight.remaining
	if err = s.consume(planCost); err != nil {
		return out, err
	}
	limits, err = s.available()
	if err != nil {
		return out, err
	}
	coverage, err := VerifyNativeReceiptEvidence(ctx, a.Chain.Key, block, *block.NativeCoverage.Evidence, limits)
	if err != nil {
		return out, err
	}
	if err = s.consume(coverage.ProofBytesUsed); err != nil {
		return out, err
	}
	if err = VerifyNativeBusinessCoverage(a.Chain, coverage); err != nil {
		return out, err
	}
	limits, err = s.available()
	if err != nil {
		return out, err
	}
	account, used, err := a.readNativeContractAccountProof(ctx, coverage, address, plan, limits)
	if err != nil {
		return out, err
	}
	if err = s.consume(used); err != nil {
		return out, err
	}
	account.ProofBytesUsed += planCost + coverage.ProofBytesUsed
	return account, nil
}
