package evm

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"strconv"
	"strings"

	"xone/chain-parser-go/internal/rpc"
)

type nativeProofReader struct {
	ctx          context.Context
	cancel       context.CancelFunc
	client       *rpc.Client
	baseMaxBytes int
	budget       receiptProofBudget
	chainKey     string
	genesis      string
	limits       ReceiptProofLimits
}

func newNativeProofReader(ctx context.Context, a *Adapter, limits ReceiptProofLimits) (*nativeProofReader, error) {
	if err := limits.validate(); err != nil {
		return nil, err
	}
	if a.Chain.BlockIntegrity != ChainNativeCommitmentsV2 {
		return nil, errors.New("NATIVE_COMMITMENT_PROFILE_REQUIRED")
	}
	genesis, err := knownBodyGenesis(a.Chain.Key)
	if err != nil {
		return nil, err
	}
	if !strings.EqualFold(genesis, a.Chain.Genesis) {
		return nil, errors.New("BLOCK_BODY_FINGERPRINT")
	}
	if a.Client == nil || a.Client.HTTP == nil || a.Client.HTTP.Timeout <= 0 || a.Client.MaxBytes <= 0 {
		return nil, errors.New("RECEIPT_PROOF_LIMITS_INVALID")
	}
	if err = ctx.Err(); err != nil {
		return nil, err
	}
	ctx, cancel := context.WithTimeout(ctx, a.Client.HTTP.Timeout)
	_, providers := a.Client.ProviderClients()
	if len(providers) == 0 {
		cancel()
		return nil, errors.New("NO_HEALTHY_PROVIDER")
	}
	r := &nativeProofReader{ctx: ctx, cancel: cancel, client: providers[0], baseMaxBytes: a.Client.MaxBytes, budget: receiptProofBudget{remaining: limits.MaxBytes}, chainKey: a.Chain.Key, genesis: genesis, limits: limits}
	httpClient := *r.client.HTTP
	transport := httpClient.Transport
	if transport == nil {
		transport = http.DefaultTransport
	}
	httpClient.Transport = nativeBudgetTransport{base: transport, budget: &r.budget}
	r.client.HTTP = &httpClient
	identity, err := r.call("eth_chainId", []any{})
	if err != nil {
		cancel()
		return nil, err
	}
	var id string
	json.Unmarshal(identity, &id)
	number, e := canonicalIndex(id)
	if e != nil || "evm:"+strconv.FormatUint(number, 10) != a.Chain.Key {
		cancel()
		return nil, errors.New("BLOCK_BODY_FINGERPRINT")
	}
	origin, err := r.call("eth_getBlockByNumber", []any{"0x0", false})
	if err != nil {
		cancel()
		return nil, err
	}
	var hash struct{ Hash string }
	if json.Unmarshal(origin, &hash) != nil || !strings.EqualFold(hash.Hash, genesis) {
		cancel()
		return nil, errors.New("BLOCK_BODY_FINGERPRINT")
	}
	return r, nil
}
func (r *nativeProofReader) call(method string, params []any) (json.RawMessage, error) {
	switch method {
	case "eth_chainId", "eth_getBlockByNumber", "eth_getBlockReceipts", "eth_getTransactionReceipt", "eth_getRawTransactionByHash", "eth_getProof":
	default:
		return nil, errors.New("NATIVE_PROOF_RPC_METHOD_FORBIDDEN")
	}
	if err := r.ctx.Err(); err != nil {
		return nil, err
	}
	_, encodingErr := json.Marshal(map[string]any{"jsonrpc": "2.0", "id": 1, "method": method, "params": params})
	if encodingErr != nil {
		return nil, encodingErr
	}
	// Fixed work belongs to the logical call; payload bytes are charged by
	// the transport before each actual attempt, including retries.
	if err := r.budget.charge(128); err != nil {
		return nil, err
	}
	r.client.MaxBytes = min(r.baseMaxBytes, r.budget.remaining)
	if r.client.MaxBytes <= 0 {
		return nil, errors.New("RECEIPT_PROOF_BUDGET")
	}
	var raw json.RawMessage
	if err := r.client.Call(r.ctx, method, params, &raw); err != nil {
		if r.budget.remaining <= 0 {
			return nil, errors.New("RECEIPT_PROOF_BUDGET")
		}
		return nil, err
	}
	if err := r.budget.charge(len(raw)); err != nil {
		return nil, err
	}
	if err := rejectDuplicateReceiptKeys(raw); err != nil {
		return nil, err
	}
	return raw, nil
}

func projectFullRPCBlock(full json.RawMessage) (json.RawMessage, []json.RawMessage, error) {
	var fields map[string]json.RawMessage
	var body struct{ Transactions []json.RawMessage }
	if json.Unmarshal(full, &fields) != nil || json.Unmarshal(full, &body) != nil || body.Transactions == nil {
		return nil, nil, errors.New("BLOCK_TRANSACTIONS_MISSING")
	}
	hashes := make([]string, len(body.Transactions))
	for i, tx := range body.Transactions {
		var value struct{ Hash string }
		if json.Unmarshal(tx, &value) != nil || !receiptHex(value.Hash, 32) {
			return nil, nil, errors.New("BLOCK_INTEGRITY_TRANSACTION_HASH")
		}
		hashes[i] = canonicalReceiptIdentity(value.Hash)
	}
	fields["transactions"], _ = json.Marshal(hashes)
	header, err := json.Marshal(fields)
	return header, body.Transactions, err
}
func (r *nativeProofReader) fullBlock(height uint64) (ChainHeaderCommitment, ExecutionReceiptBlock, error) {
	return r.fullBlockTag(fmt.Sprintf("0x%x", height))
}
func (r *nativeProofReader) fullBlockTag(tag string) (ChainHeaderCommitment, ExecutionReceiptBlock, error) {
	var empty ChainHeaderCommitment
	var block ExecutionReceiptBlock
	raw, err := r.call("eth_getBlockByNumber", []any{tag, true})
	if err != nil {
		return empty, block, err
	}
	if err = r.budget.charge(len(raw) + 128); err != nil {
		return empty, block, err
	}
	header, txs, err := projectFullRPCBlock(raw)
	if err != nil {
		return empty, block, err
	}
	proof, err := VerifyChainHeaderJSON(r.ctx, r.chainKey, header)
	if err != nil {
		return empty, block, err
	}
	if strings.HasPrefix(tag, "0x") && proof.Block.Number != tag {
		return empty, block, errors.New("NATIVE_PROOF_HEIGHT")
	}
	if len(txs) > r.limits.MaxTransactions {
		return empty, block, errors.New("RECEIPT_PROOF_TRANSACTION_BUDGET")
	}
	return proof, ExecutionReceiptBlock{Header: header, TransactionsJSON: txs}, nil
}
func (r *nativeProofReader) prepareBody(header ChainHeaderCommitment, block *ExecutionReceiptBlock) error {
	// Full JSON is an explicit authority choice for V2. Only enumerated layouts
	// are reconstructed; another native type must supply real raw bytes.
	raws := make([]string, len(block.TransactionsJSON))
	needsRaw := false
	for i, raw := range block.TransactionsJSON {
		if err := r.ctx.Err(); err != nil {
			return err
		}
		encoded, err := encodeChainTransactionJSON(r.chainKey, header.Block, uint64(i), raw)
		if err != nil {
			if err.Error() != "NATIVE_TRANSACTION_RAW_REQUIRED" {
				return err
			}
			reply, e := r.call("eth_getRawTransactionByHash", []any{header.Block.Transactions[i]})
			if e != nil {
				return e
			}
			if json.Unmarshal(reply, &raws[i]) != nil || !receiptHex(raws[i], -1) || len(raws[i]) <= 2 {
				return errors.New("NATIVE_TRANSACTION_RAW_UNAVAILABLE")
			}
			needsRaw = true
		} else {
			raws[i] = "0x" + hex.EncodeToString(encoded)
		}
		if err := r.budget.charge(len(raws[i]) + 128); err != nil {
			return err
		}
	}
	if needsRaw {
		block.RawTransactions = raws
		block.TransactionsJSON = nil
	}
	return nil
}
func (r *nativeProofReader) receipts(block RawBlock, mode string) ([]Receipt, error) {
	switch mode {
	case "", "per-tx", "bulk-auto":
	default:
		return nil, errors.New("RECEIPTS_MODE_UNSUPPORTED")
	}
	if len(block.Transactions) > r.limits.MaxTransactions {
		return nil, errors.New("RECEIPT_PROOF_TRANSACTION_BUDGET")
	}
	var receipts []Receipt
	if mode == "bulk-auto" {
		raw, err := r.call("eth_getBlockReceipts", []any{block.Hash})
		if err != nil && rpc.Classify(err) != rpc.Unsupported {
			return nil, err
		}
		if err == nil {
			if json.Unmarshal(raw, &receipts) != nil {
				return nil, errors.New("RPC_RESULT_INVALID")
			}
			if receipts == nil || len(receipts) != len(block.Transactions) {
				return nil, errors.New("INCOMPLETE_RECEIPTS")
			}
		}
		if err != nil {
			mode = "per-tx"
		}
	}
	if mode != "bulk-auto" {
		receipts = make([]Receipt, 0, len(block.Transactions))
		for _, hash := range block.Transactions {
			raw, err := r.call("eth_getTransactionReceipt", []any{hash})
			if err != nil {
				return nil, err
			}
			var receipt Receipt
			if json.Unmarshal(raw, &receipt) != nil {
				return nil, errors.New("RPC_RESULT_INVALID")
			}
			receipts = append(receipts, receipt)
		}
	}
	for i := range receipts {
		if !chargeReceipt(&r.budget.remaining, receipts[i]) {
			return nil, errors.New("RECEIPT_PROOF_BUDGET")
		}
		canonicalizeReadReceipt(&receipts[i])
	}
	return receipts, nil
}

// ReadNativeReceiptCoverage is the explicit V2 path for complete native
// commitments. It uses one deadline and a shared decreasing budget across
// fingerprint, full body, receipts and successor-window acquisition. Legacy
// ReadReceiptsWithMode retains its strict own-header contract unchanged.
func (a *Adapter) ReadNativeReceiptCoverage(ctx context.Context, target RawBlock, mode string, limits ReceiptProofLimits) (VerifiedReceiptCoverage, error) {
	return a.readNativeReceiptCoverageCached(ctx, target, mode, limits, nil)
}
func (a *Adapter) readNativeReceiptCoverageCached(ctx context.Context, target RawBlock, mode string, limits ReceiptProofLimits, cached *ChainBlockBodyProof) (VerifiedReceiptCoverage, error) {
	var empty VerifiedReceiptCoverage
	r, err := newNativeProofReader(ctx, a, limits)
	if err != nil {
		return empty, err
	}
	defer r.cancel()
	height, err := canonicalIndex(target.Number)
	if err != nil {
		return empty, err
	}
	var header ChainHeaderCommitment
	var body ExecutionReceiptBlock
	if cached == nil {
		header, body, err = r.fullBlock(height)
	} else {
		header, err = VerifyChainHeaderJSON(r.ctx, r.chainKey, cached.Header)
		if err == nil {
			canonical := header.Block
			canonical.Transactions = append([]string{}, header.Block.Transactions...)
			if r.chainKey == "evm:137" && height < 80084800 {
				derived, e := derivedBorReceiptHash(header.Block)
				if e != nil {
					return empty, e
				}
				if len(canonical.Transactions) > 0 && strings.EqualFold(canonical.Transactions[len(canonical.Transactions)-1], derived) {
					canonical.Transactions = canonical.Transactions[:len(canonical.Transactions)-1]
				}
			}
			if !sameReceiptAnchor(canonical, cached.CanonicalBlock) {
				return empty, errors.New("NATIVE_CACHED_BODY_IDENTITY")
			}
			canonicalHeader := header
			canonicalHeader.Block = canonical
			err = verifyRawBodyCommitment(r.ctx, r.chainKey, canonicalHeader, cached.RawTransactions, &r.budget)
			body = ExecutionReceiptBlock{Header: cached.Header, RawTransactions: cached.RawTransactions}
		}
	}
	if err != nil {
		return empty, err
	}
	if !sameReceiptAnchor(target, header.Block) {
		return empty, errors.New("NATIVE_PROOF_TARGET_IDENTITY")
	}
	if r.chainKey == "evm:137" {
		receipts, err := r.receipts(header.Block, mode)
		if err != nil {
			return empty, err
		}
		limits.MaxBytes = r.budget.remaining
		coverage, err := VerifyBorReceiptCoverage(r.ctx, header.Block, receipts, BorReceiptProof{ChainKey: r.chainKey, GenesisHash: r.genesis, Header: body.Header, TransactionsJSON: body.TransactionsJSON, RawTransactions: body.RawTransactions}, limits)
		coverage.ProofBytesUsed += r.limits.MaxBytes - r.budget.remaining
		return coverage, err
	}
	if err = r.prepareBody(header, &body); err != nil {
		return empty, err
	}
	if r.chainKey == "evm:43114" && header.SettledHeight != nil {
		return r.saeCoverage(target, header, body, mode)
	}
	receipts, err := r.receipts(header.Block, mode)
	if err != nil {
		return empty, err
	}
	body.Receipts = receipts
	if err = verifyExecutionBody(r.ctx, r.chainKey, header, body, &r.budget); err != nil {
		return empty, err
	}
	if err = VerifyChainReceiptRoot(r.ctx, r.chainKey, header.Block, receipts); err != nil {
		return empty, err
	}
	accountEncoding := "ethereum-account-rlp/v1"
	if r.chainKey == "evm:43114" {
		accountEncoding = "coreth-account-held-xchain-rlp/v1"
	}
	coverage := VerifiedReceiptCoverage{ChainKey: r.chainKey, Block: header.Block, CanonicalBlock: header.Block, CanonicalReceipts: receipts, CanonicalBodyProven: true, CanonicalReceiptsProven: true, WholeBlockReceiptEventsProven: true, SystemObservationCoverage: "all-canonical-receipts-committed", NativeSemanticsID: r.chainKey + "/native-header-body-receipts/v2", ActivationID: header.HeaderEncoding, ReceiptAuthority: "header-receipts-root", ReceiptRoot: header.ReceiptsRoot, WindowStart: height, WindowEnd: height, StateAnchor: &ExecutionStateAnchor{Height: height, BlockHash: header.Block.Hash, StateRoot: header.StateRoot, SettlingHeight: height, SettlingHash: header.Block.Hash, Authority: "native-post-execution-header-state-root", AccountEncodingID: accountEncoding}}
	if err = attachNativeSemantics(&coverage, header); err != nil {
		return empty, err
	}
	bodyCopy := body
	coverage.Evidence = &NativeReceiptEvidence{SchemaVersion: 1, ChainKey: r.chainKey, GenesisHash: r.genesis, NativeSemanticsID: coverage.NativeSemanticsID, Kind: "own-header-receipts", Block: &bodyCopy}
	coverage.ProofBytesUsed += r.limits.MaxBytes - r.budget.remaining
	return coverage, nil
}

func (r *nativeProofReader) saeCoverage(target RawBlock, targetHeader ChainHeaderCommitment, targetBody ExecutionReceiptBlock, mode string) (VerifiedReceiptCoverage, error) {
	var empty VerifiedReceiptCoverage
	height, _ := canonicalIndex(target.Number)
	headers := map[uint64]json.RawMessage{height: targetBody.Header}
	decoded := map[uint64]ChainHeaderCommitment{height: targetHeader}
	var settling uint64
	previous := targetHeader
	for ahead := uint64(1); ahead <= r.limits.MaxLookahead; ahead++ {
		if len(headers) >= r.limits.MaxHeaders {
			return empty, errors.New("SAE_HEADER_BUDGET")
		}
		raw, err := r.call("eth_getBlockByNumber", []any{fmt.Sprintf("0x%x", height+ahead), false})
		if err != nil {
			return empty, err
		}
		if err = r.budget.charge(len(raw) + 256); err != nil {
			return empty, err
		}
		next, err := VerifyChainHeaderJSON(r.ctx, r.chainKey, raw)
		if err != nil {
			return empty, err
		}
		if next.Block.Number != fmt.Sprintf("0x%x", height+ahead) || next.Block.ParentHash != previous.Block.Hash || next.SettledHeight == nil || *next.SettledHeight < *previous.SettledHeight {
			return empty, errors.New("SAE_HEADER_CHAIN_INVALID")
		}
		headers[height+ahead] = raw
		decoded[height+ahead] = next
		if *next.SettledHeight >= height {
			settling = height + ahead
			break
		}
		previous = next
	}
	if settling == 0 {
		return empty, errors.New("SAE_SETTLEMENT_NOT_YET_PROVEN")
	}
	start, end := *decoded[settling-1].SettledHeight+1, *decoded[settling].SettledHeight
	if end < start || end-start+1 > uint64(r.limits.MaxWindowBlocks) || settling-start+1 > uint64(r.limits.MaxHeaders) {
		return empty, errors.New("SAE_WINDOW_BUDGET")
	}
	window := make([]ExecutionReceiptBlock, 0, end-start+1)
	transactions := 0
	for n := start; n <= end; n++ {
		header, block := targetHeader, targetBody
		if n != height {
			var err error
			header, block, err = r.fullBlock(n)
			if err != nil {
				return empty, err
			}
			if known, ok := decoded[n]; ok && known.Block.Hash != header.Block.Hash {
				return empty, errors.New("SAE_WINDOW_REORG")
			}
			if err = r.prepareBody(header, &block); err != nil {
				return empty, err
			}
			headers[n] = block.Header
			decoded[n] = header
		}
		transactions += len(header.Block.Transactions)
		if transactions > r.limits.MaxTransactions {
			return empty, errors.New("RECEIPT_PROOF_TRANSACTION_BUDGET")
		}
		receipts, err := r.receipts(header.Block, mode)
		if err != nil {
			return empty, err
		}
		block.Receipts = receipts
		window = append(window, block)
	}
	proof := SAEExecutionProof{ChainKey: r.chainKey, GenesisHash: r.genesis, Window: window}
	for n := start; n <= settling; n++ {
		raw, ok := headers[n]
		if !ok {
			return empty, errors.New("SAE_WINDOW_HEADER_MISSING")
		}
		proof.Headers = append(proof.Headers, raw)
	}
	limits := r.limits
	limits.MaxBytes = r.budget.remaining
	coverage, err := VerifySAEExecutionProof(r.ctx, target, proof, limits)
	coverage.ProofBytesUsed += r.limits.MaxBytes - r.budget.remaining
	return coverage, err
}

// ReadNativeBlockProof verifies the native header and canonical body. Bor's
// derived RPC suffix is retained explicitly as an uncommitted observation;
// CanonicalBlock, not the full RPC list, identifies transactionsRoot members.
func (a *Adapter) ReadNativeBlockProof(ctx context.Context, tag string, limits ReceiptProofLimits) (ChainBlockBodyProof, error) {
	var empty ChainBlockBodyProof
	r, err := newNativeProofReader(ctx, a, limits)
	if err != nil {
		return empty, err
	}
	defer r.cancel()
	if err = validateNativeBlockTag(tag); err != nil {
		return empty, err
	}
	header, body, err := r.fullBlockTag(tag)
	if err != nil {
		return empty, err
	}
	return r.proveBody(header, body)
}
func (r *nativeProofReader) proveBody(header ChainHeaderCommitment, body ExecutionReceiptBlock) (ChainBlockBodyProof, error) {
	var empty ChainBlockBodyProof
	var err error
	height, _ := canonicalIndex(header.Block.Number)
	canonical := header.Block
	canonical.Transactions = append([]string{}, header.Block.Transactions...)
	system := []string{}
	if r.chainKey == "evm:137" && height < 80084800 {
		derived, err := derivedBorReceiptHash(header.Block)
		if err != nil {
			return empty, err
		}
		for i, hash := range header.Block.Transactions {
			if strings.EqualFold(hash, derived) {
				if i != len(header.Block.Transactions)-1 {
					return empty, errors.New("BOR_SYSTEM_POSITION_INVALID")
				}
				system = append(system, hash)
				canonical.Transactions = canonical.Transactions[:i]
				body.TransactionsJSON = body.TransactionsJSON[:i]
			}
		}
	}
	canonicalHeader := header
	canonicalHeader.Block = canonical
	if err = r.prepareBody(canonicalHeader, &body); err != nil {
		return empty, err
	}
	if err = verifyExecutionBody(r.ctx, r.chainKey, canonicalHeader, body, &r.budget); err != nil {
		return empty, err
	}
	raws := body.RawTransactions
	if len(raws) == 0 {
		raws = make([]string, 0, len(body.TransactionsJSON))
		for i, raw := range body.TransactionsJSON {
			payload, err := encodeChainTransactionJSON(r.chainKey, canonical, uint64(i), raw)
			if err != nil {
				return empty, err
			}
			if err = r.budget.charge(2*len(payload) + 128); err != nil {
				return empty, err
			}
			raws = append(raws, "0x"+hex.EncodeToString(payload))
		}
	}
	sem, err := ResolveNativeSemantics(r.chainKey, header)
	if err != nil {
		return empty, err
	}
	if !sem.Admitted {
		return empty, errors.New("NATIVE_SOURCE_ACTIVATION_NOT_ADMITTED")
	}
	scope := "all-header-body-transactions"
	if r.chainKey == "evm:137" && height < 80084800 {
		scope = "canonical-header-transactions/system-observations-isolated"
	}
	return ChainBlockBodyProof{Block: header.Block, CanonicalBlock: canonical, Header: body.Header, HeaderCommitment: header, RawTransactions: raws, UncommittedSystemTransactionIDs: system, BodyCoverageScope: scope, NativeSemantics: &sem, ProofBytesUsed: r.limits.MaxBytes - r.budget.remaining, TransactionEncodingAuthority: "standard-signed-json+enumerated-native-rlp/native-raw-if-required"}, nil
}

// ReadNativeAccountProof reads explicit physical EIP-1186 storage keys.
// Contract logical slots use ReadNativeContractAccountProof; this physical
// API does not infer imported-asset or skip-transform semantics from key bits.
func (a *Adapter) ReadNativeAccountProof(ctx context.Context, coverage VerifiedReceiptCoverage, address string, keys []string, limits ReceiptProofLimits) (VerifiedAccountCommitment, error) {
	var empty VerifiedAccountCommitment
	if coverage.ChainKey != a.Chain.Key || !coverage.CanonicalBodyProven || !coverage.CanonicalReceiptsProven || coverage.NativeSemanticsID == "" {
		return empty, errors.New("STATE_PROOF_COVERAGE_REQUIRED")
	}
	anchor, err := coverage.TargetStateAnchor()
	if err != nil {
		return empty, err
	}
	if !receiptHex(address, 20) || len(keys) > 4096 {
		return empty, errors.New("STATE_PROOF_REQUEST_INVALID")
	}
	slots := make([]string, 0, len(keys))
	selected := map[string]bool{}
	for _, key := range keys {
		bytes, err := storageProofKey(key)
		if err != nil {
			return empty, err
		}
		normalized := "0x" + hex.EncodeToString(bytes)
		if selected[normalized] {
			return empty, errors.New("STATE_STORAGE_PROOF_DUPLICATE")
		}
		selected[normalized] = true
		slots = append(slots, normalized)
	}
	r, err := newNativeProofReader(ctx, a, limits)
	if err != nil {
		return empty, err
	}
	defer r.cancel()
	raw, err := r.call("eth_getProof", []any{strings.ToLower(address), slots, map[string]any{"blockHash": anchor.BlockHash, "requireCanonical": true}})
	if err != nil {
		return empty, err
	}
	account, err := VerifyExecutionAccountProof(r.ctx, *anchor, raw, r.budget.remaining)
	if err != nil {
		return empty, err
	}
	if !strings.EqualFold(account.Address, address) || len(account.StorageValues) != len(selected) {
		return empty, errors.New("STATE_STORAGE_REQUEST_COVERAGE")
	}
	for key := range selected {
		if _, ok := account.StorageValues[key]; !ok {
			return empty, errors.New("STATE_STORAGE_REQUEST_COVERAGE")
		}
	}
	return account, nil
}

func validateNativeBlockTag(tag string) error {
	switch tag {
	case "latest", "safe", "finalized", "earliest":
		return nil
	default:
		_, err := canonicalIndex(tag)
		if err != nil {
			return errors.New("NATIVE_BLOCK_TAG_INVALID")
		}
		return nil
	}
}

// ReadNativeSettledBlockProof explicitly selects an already-settled execution
// for normal head ingestion. Exact historical requests should instead use
// ReadNativeBlockProof and acquire their successor proof without changing height.
func (a *Adapter) ReadNativeSettledBlockProof(ctx context.Context, tag string, limits ReceiptProofLimits) (ChainBlockBodyProof, error) {
	var empty ChainBlockBodyProof
	if err := validateNativeBlockTag(tag); err != nil {
		return empty, err
	}
	r, err := newNativeProofReader(ctx, a, limits)
	if err != nil {
		return empty, err
	}
	defer r.cancel()
	raw, err := r.call("eth_getBlockByNumber", []any{tag, false})
	if err != nil {
		return empty, err
	}
	head, err := VerifyChainHeaderJSON(r.ctx, r.chainKey, raw)
	if err != nil {
		return empty, err
	}
	height, _ := canonicalIndex(head.Block.Number)
	if head.SettledHeight != nil {
		height = *head.SettledHeight
	}
	header, body, err := r.fullBlock(height)
	if err != nil {
		return empty, err
	}
	if head.SettledHeight == nil && header.Block.Hash != head.Block.Hash {
		return empty, errors.New("NATIVE_PROOF_HEAD_REORG")
	}
	proof, err := r.proveBody(header, body)
	if err != nil {
		return empty, err
	}
	copy := head.Block
	proof.ObservedHead = &copy
	proof.ProofBytesUsed = r.limits.MaxBytes - r.budget.remaining
	return proof, nil
}

// readNativeContractAccountProof uses physical keys on eth_getProof. The MPT
// verifier remains physical; contract interpretation is applied only afterwards.
func (a *Adapter) readNativeContractAccountProof(ctx context.Context, coverage VerifiedReceiptCoverage, address string, plan contractStoragePlan, limits ReceiptProofLimits) (out VerifiedContractAccountCommitment, used int, err error) {
	if err = limits.validate(); err != nil {
		return out, 0, err
	}
	anchor, err := coverage.TargetStateAnchor()
	if err != nil {
		return out, 0, err
	}
	encoding, err := contractStorageEncoding(ContractStorageSource{ChainKey: a.Chain.Key, GenesisHash: a.Chain.Genesis}, *anchor)
	if err != nil {
		return out, 0, err
	}
	if encoding != plan.encoding {
		return out, 0, errors.New("STATE_CONTRACT_STORAGE_ENCODING_UNSUPPORTED")
	}
	if !receiptHex(address, 20) {
		return out, 0, errors.New("STATE_PROOF_REQUEST_INVALID")
	}
	r, err := newNativeProofReader(ctx, a, limits)
	if err != nil {
		return out, 0, err
	}
	defer r.cancel()
	defer func() { used = limits.MaxBytes - r.budget.remaining }()
	raw, err := r.call("eth_getProof", []any{strings.ToLower(address), plan.physicalKeys, map[string]any{"blockHash": anchor.BlockHash, "requireCanonical": true}})
	if err != nil {
		return out, 0, err
	}
	account, err := VerifyExecutionAccountProof(r.ctx, *anchor, raw, r.budget.remaining)
	if err != nil {
		return out, 0, err
	}
	if err = r.budget.charge(account.proofBytesUsed); err != nil {
		return out, 0, err
	}
	if !strings.EqualFold(account.Address, address) {
		return out, 0, errors.New("STATE_STORAGE_REQUEST_COVERAGE")
	}
	out, err = projectContractAccount(r.ctx, plan, account, &r.budget)
	if err != nil {
		return out, 0, err
	}
	out.ProofBytesUsed = limits.MaxBytes - r.budget.remaining
	return out, 0, nil
}
