package evm

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"golang.org/x/crypto/sha3"
	"math/big"
	"strings"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
)

func managedV2Selector(signature string) string {
	h := sha3.NewLegacyKeccak256()
	h.Write([]byte(signature))
	return "0x" + hex.EncodeToString(h.Sum(nil)[:4])
}
func managedV2Hash(s string) []byte {
	h := sha3.NewLegacyKeccak256()
	h.Write([]byte(s))
	return h.Sum(nil)
}

type managedV2Reader struct {
	a         *Adapter
	ctx       context.Context
	anchor    map[string]any
	timestamp uint64
	err       error
}

func (r *managedV2Reader) call(to, signature, args string, words int) []byte {
	if r.err != nil {
		return nil
	}
	var result string
	e := r.a.Client.Call(r.ctx, "eth_call", []any{map[string]string{"to": to, "data": managedV2Selector(signature) + args}, r.anchor}, &result)
	var b []byte
	if e == nil {
		if !strings.HasPrefix(result, "0x") {
			e = errors.New("MANAGED_V2_ABI_HEX")
		} else {
			b, e = hex.DecodeString(result[2:])
		}
	}
	if e == nil && (len(b)%32 != 0 || words >= 0 && len(b) != words*32) {
		e = errors.New("MANAGED_V2_ABI_SIZE")
	}
	if e != nil {
		r.err = fmt.Errorf("MANAGED_V2_GETTER %s: %w", signature, e)
	}
	return b
}
func (r *managedV2Reader) uintWord(b []byte, bits int) *big.Int {
	if r.err != nil {
		return new(big.Int)
	}
	v, e := wordUint(b, bits)
	if e != nil {
		r.err = e
		return new(big.Int)
	}
	return v
}
func (r *managedV2Reader) uint(to, signature, args string, bits int) *big.Int {
	return r.uintWord(r.call(to, signature, args, 1), bits)
}
func (r *managedV2Reader) address(to, signature, args string) string {
	v := r.uint(to, signature, args, 160)
	return "0x" + hex.EncodeToString(managedV2Word(v)[12:])
}
func (r *managedV2Reader) boolean(to, signature, args string) bool {
	return r.uint(to, signature, args, 1).Sign() != 0
}
func (r *managedV2Reader) require(ok bool, label string) {
	if !ok && r.err == nil {
		r.err = errors.New("MANAGED_V2_" + label)
	}
}
func (r *managedV2Reader) code(address, hash string) []byte {
	if r.err != nil {
		return nil
	}
	var result string
	e := r.a.Client.Call(r.ctx, "eth_getCode", []any{address, r.anchor}, &result)
	var b []byte
	if e == nil {
		if !strings.HasPrefix(result, "0x") {
			e = errors.New("MANAGED_V2_CODE_HEX")
		} else {
			b, e = hex.DecodeString(result[2:])
		}
	}
	if e == nil {
		h := sha256.Sum256(b)
		if len(b) == 0 || hex.EncodeToString(h[:]) != hash {
			e = errors.New("MANAGED_V2_CODE_HASH")
		}
	}
	if e != nil {
		r.err = e
	}
	return b
}
func (r *managedV2Reader) words(b []byte, n int) [][]byte {
	if r.err != nil {
		return make([][]byte, n)
	}
	if len(b) != n*32 {
		r.require(false, "ABI_SIZE")
		return make([][]byte, n)
	}
	v := make([][]byte, n)
	for i := range v {
		v[i] = b[i*32 : (i+1)*32]
	}
	return v
}
func (r *managedV2Reader) array(b []byte, n int) []domain.Amount {
	out := make([]domain.Amount, n)
	if r.err != nil {
		return out
	}
	r.require(len(b) == (2+n)*32, "ARRAY_SIZE")
	if r.err != nil {
		return out
	}
	r.require(r.uintWord(b[:32], 64).Uint64() == 32 && r.uintWord(b[32:64], 64).Uint64() == uint64(n), "ARRAY_LAYOUT")
	for i := range out {
		out[i] = domain.Amount(r.uintWord(b[(i+2)*32:(i+3)*32], 256).String())
	}
	return out
}
func (r *managedV2Reader) paused(to string) (bool, []byte, []byte) {
	w := r.words(r.call(to, "getPausedState()", "", 3), 3)
	paused := r.uintWord(w[0], 1).Sign() != 0
	r.uintWord(w[1], 64)
	r.uintWord(w[2], 64)
	r.require(r.uintWord(w[1], 64).Cmp(r.uintWord(w[2], 64)) <= 0, "PAUSE_WINDOWS")
	return paused, w[1], w[2]
}
func (r *managedV2Reader) dynamicString(to, signature string) string {
	b := r.call(to, signature, "", -1)
	if r.err != nil {
		return ""
	}
	r.require(len(b) >= 64, "STRING_SIZE")
	if r.err != nil {
		return ""
	}
	off := r.uintWord(b[:32], 64).Uint64()
	n := r.uintWord(b[32:64], 64).Uint64()
	r.require(off == 32 && n <= uint64(len(b)-64) && uint64(len(b)) == 64+(n+31)/32*32, "STRING_LAYOUT")
	if r.err != nil {
		return ""
	}
	for _, v := range b[64+n:] {
		r.require(v == 0, "STRING_PADDING")
	}
	return string(b[64 : 64+n])
}

func (a *Adapter) readManagedV2(ctx context.Context, p config.PoolConfig, anchor map[string]any, ref domain.BlockRef) (domain.Pool, error) {
	fail := func(e error) (domain.Pool, error) { return domain.Pool{}, e }
	v := p.MultiAsset
	isBreaker := p.Protocol == domain.ManagedV2BreakerProtocol
	isBPT := p.Protocol == domain.ManagedV2BptProtocol || isBreaker
	policy := "standard-no-bpt"
	if isBPT {
		policy = "standard-bpt"
		if isBreaker {
			policy = "standard-breaker-bpt"
		}
	}
	if (p.Protocol != domain.ManagedV2Protocol && p.Protocol != domain.ManagedV2ScheduledProtocol && !isBPT) || v == nil || v.ManagedV2 == nil || v.TokenPolicy != policy || len(v.Tokens) < 2 || len(v.Tokens) > 50 || len(v.PoolID) != 66 || !strings.HasPrefix(v.PoolID, p.ID+"0001") || v.ExpectedImplementation != "" || v.ExpectedImplementationCodeSHA256 != "" {
		return fail(errors.New("MANAGED_V2_CONFIG"))
	}
	c := v.ManagedV2
	if isBPT != (c.BPT != nil) {
		return fail(errors.New("MANAGED_V2_BPT_CONFIG"))
	}
	if isBPT {
		if e := c.BPT.Validate(); e != nil {
			return fail(e)
		}
	}
	if e := domain.ValidateManagedV2Dependencies(c.Dependencies); e != nil {
		return fail(e)
	}
	if !v2CompleteHash(ref.Hash) || !v2CompleteParentHash(ref.Number, ref.ParentHash) || ref.ChainKey != a.Chain.Key || anchor["blockHash"] != ref.Hash || anchor["requireCanonical"] != true || len(anchor) != 2 || ref.TimestampMS%1000 != 0 {
		return fail(errors.New("MANAGED_V2_ANCHOR"))
	}
	if c.ExpectedPoolRuntimeTemplateSHA256 != managedV2Templates["ManagedPool"].SHA256 {
		return fail(errors.New("MANAGED_V2_POOL_TEMPLATE_PIN"))
	}
	r := &managedV2Reader{a: a, ctx: ctx, anchor: anchor, timestamp: ref.TimestampMS / 1000}
	codes := map[string][]byte{p.ID: r.code(p.ID, v.ExpectedCodeSHA256), v.ExpectedVault: r.code(v.ExpectedVault, v.ExpectedVaultCodeSHA256), c.ExpectedFactory: r.code(c.ExpectedFactory, c.ExpectedFactoryCodeSHA256), c.ExpectedFeeProvider: r.code(c.ExpectedFeeProvider, c.ExpectedFeeProviderCodeSHA256), c.ExpectedProtocolCollector: r.code(c.ExpectedProtocolCollector, c.ExpectedProtocolCollectorCodeSHA256)}
	links := map[string]string{}
	for _, d := range c.Dependencies {
		name := managedV2Roles[d.Role]
		r.require(d.TemplateSHA256 == managedV2Templates[name].SHA256, "DEPENDENCY_TEMPLATE_PIN")
		codes[d.Address] = r.code(d.Address, d.CodeSHA256)
		links[name] = d.Address
	}
	r.require(r.address(p.ID, "getVault()", "") == v.ExpectedVault, "POOL_VAULT")
	pid := r.call(p.ID, "getPoolId()", "", 1)
	r.require("0x"+hex.EncodeToString(pid) == v.PoolID, "POOL_ID")
	idArg := v.PoolID[2:]
	poolArg := hex.EncodeToString(managedV2AddressWord(p.ID))
	registration := r.words(r.call(v.ExpectedVault, "getPool(bytes32)", idArg, 2), 2)
	r.require(bytes.Equal(registration[0], managedV2AddressWord(p.ID)) && r.uintWord(registration[1], 8).Uint64() == 1, "REGISTRATION")
	r.require(r.boolean(c.ExpectedFactory, "isPoolFromFactory(address)", poolArg), "FACTORY_MEMBERSHIP")
	for _, target := range []string{c.ExpectedFactory, c.ExpectedFeeProvider, links["RecoveryModeHelper"]} {
		r.require(r.address(target, "getVault()", "") == v.ExpectedVault, "DEPENDENCY_VAULT")
	}
	r.require(r.address(c.ExpectedProtocolCollector, "vault()", "") == v.ExpectedVault, "COLLECTOR_VAULT")
	r.require(r.address(c.ExpectedFactory, "getProtocolFeePercentagesProvider()", "") == c.ExpectedFeeProvider, "FACTORY_PROVIDER")
	r.require(r.address(c.ExpectedFactory, "getWeightedMath()", "") == links["ExternalWeightedMath"], "FACTORY_MATH")
	r.require(r.address(c.ExpectedFactory, "getRecoveryModeHelper()", "") == links["RecoveryModeHelper"], "FACTORY_RECOVERY")
	collector := r.address(p.ID, "getProtocolFeesCollector()", "")
	r.require(collector == c.ExpectedProtocolCollector && r.address(v.ExpectedVault, "getProtocolFeesCollector()", "") == collector, "COLLECTOR")
	manager := r.address(p.ID, "getOwner()", "")
	r.require(manager == c.ExpectedManager, "MANAGER")
	paused, pauseEnd, bufferEnd := r.paused(p.ID)
	vaultPaused, vaultPauseEnd, vaultBufferEnd := r.paused(v.ExpectedVault)
	m := &domain.BalancerManagedV2State{Timestamp: ref.TimestampMS / 1000, BlockNumber: ref.Number, BlockHash: ref.Hash, Factory: c.ExpectedFactory, FactoryCodeSHA256: c.ExpectedFactoryCodeSHA256, PoolRuntimeTemplateSHA256: c.ExpectedPoolRuntimeTemplateSHA256, FeeProvider: c.ExpectedFeeProvider, FeeProviderCodeSHA256: c.ExpectedFeeProviderCodeSHA256, ProtocolCollector: collector, ProtocolCollectorCodeSHA256: c.ExpectedProtocolCollectorCodeSHA256, Dependencies: append([]domain.ManagedV2Dependency(nil), c.Dependencies...), Manager: manager, Paused: paused, VaultPaused: vaultPaused, VaultPauseWindowEnd: r.uintWord(vaultPauseEnd, 64).Uint64(), VaultBufferPeriodEnd: r.uintWord(vaultBufferEnd, 64).Uint64(), PauseWindowEnd: r.uintWord(pauseEnd, 64).Uint64(), BufferPeriodEnd: r.uintWord(bufferEnd, 64).Uint64()}
	if isBreaker {
		m.CircuitBreakers = &domain.ManagedV2CircuitState{Tokens: []domain.ManagedV2CircuitToken{}}
	}
	m.RecoveryMode = r.boolean(p.ID, "inRecoveryMode()", "")
	m.SwapsEnabled = r.boolean(p.ID, "getSwapEnabled()", "")
	m.AllowlistEnabled = r.boolean(p.ID, "getMustAllowlistLPs()", "")
	feeParams := r.words(r.call(p.ID, "getManagementAumFeeParams()", "", 2), 2)
	m.ManagementAUMFee = domain.Amount(r.uintWord(feeParams[0], 256).String())
	m.LastCollectionTimestamp = uint32(r.uintWord(feeParams[1], 32).Uint64())
	r.require(uint64(m.LastCollectionTimestamp) <= m.Timestamp, "COLLECTION_CLOCK")
	// The published getter exposes an effective zero fee during recovery,
	// while the configured fee remains the expected normal-operation value.
	// Recheck that configured value when the pool leaves recovery.
	if m.RecoveryMode {
		r.require(m.ManagementAUMFee == "0", "RECOVERY_EFFECTIVE_FEE")
	} else {
		r.require(m.ManagementAUMFee == c.ExpectedManagementAUMFee, "ANNUAL_FEE")
	}
	feeIDs := new(big.Int)
	for _, kind := range []int64{0, 2, 3} {
		id := r.uint(p.ID, "getProviderFeeId(uint256)", hex.EncodeToString(managedV2Word(big.NewInt(kind))), 8)
		// ManagedPoolSettings fixes swap/yield provider IDs in the original
		// constructor; only AUM is caller-configurable. A forged getter and
		// matching immutable must not redefine that published construction.
		if kind == 0 || kind == 2 {
			r.require(id.Cmp(big.NewInt(kind)) == 0, "FIXED_PROTOCOL_FEE_IDS")
		}
		shift := uint(0)
		if kind == 2 {
			shift = 8
		}
		if kind == 3 {
			shift = 16
			m.ProviderFeeID = domain.Amount(id.String())
		}
		feeIDs.Or(feeIDs, new(big.Int).Lsh(id, shift))
	}
	r.require(m.ProviderFeeID == c.ExpectedProviderFeeID, "PROVIDER_FEE_ID")
	aumArg := hex.EncodeToString(managedV2Word(big.NewInt(3)))
	m.CachedProtocolAUMPercentage = domain.Amount(r.uint(p.ID, "getProtocolFeePercentageCache(uint256)", aumArg, 64).String())
	providerID := new(big.Int)
	if _, ok := providerID.SetString(string(m.ProviderFeeID), 10); !ok {
		r.require(false, "PROVIDER_FEE_ID")
	}
	m.LiveProviderAUMPercentage = domain.Amount(r.uint(c.ExpectedFeeProvider, "getFeeTypePercentage(uint256)", hex.EncodeToString(managedV2Word(providerID)), 256).String())
	cap := r.uint(c.ExpectedFeeProvider, "getFeeTypeMaximumPercentage(uint256)", hex.EncodeToString(managedV2Word(providerID)), 256)
	r.require(cap.Sign() > 0 && cap.Cmp(big.NewInt(1000000000000000000)) <= 0 && m.LiveProviderAUMPercentage.Big().Cmp(cap) <= 0 && m.CachedProtocolAUMPercentage.Big().Cmp(cap) <= 0, "PROVIDER_CAP")
	fee := r.uint(p.ID, "getSwapFeePercentage()", "", 64)
	s := &domain.MultiAssetState{CodeSHA256: v.ExpectedCodeSHA256, Vault: v.ExpectedVault, VaultCodeSHA256: v.ExpectedVaultCodeSHA256, PoolID: v.PoolID, Amplification: "0", ManagedV2: m}
	s.Weights = r.array(r.call(p.ID, "getNormalizedWeights()", "", -1), len(v.Tokens))
	scales := r.array(r.call(p.ID, "getScalingFactors()", "", -1), len(v.Tokens))
	// Complete canonical ABI parsing, including both dynamic tails and every word.
	b := r.call(v.ExpectedVault, "getPoolTokens(bytes32)", idArg, -1)
	n := len(v.Tokens) + 1
	r.require(len(b) == (5+2*n)*32, "POOL_TOKENS_SIZE")
	var last uint64
	if r.err == nil {
		r.require(r.uintWord(b[:32], 64).Uint64() == 96 && r.uintWord(b[32:64], 64).Uint64() == uint64(128+n*32) && r.uintWord(b[96:128], 64).Uint64() == uint64(n) && r.uintWord(b[(4+n)*32:(5+n)*32], 64).Uint64() == uint64(n), "POOL_TOKENS_LAYOUT")
		last = r.uintWord(b[64:96], 32).Uint64()
		r.require(last <= ref.Number, "LAST_CHANGE")
	}
	var maxTokenChange uint64
	for i := 0; i < n && r.err == nil; i++ {
		token := p.ID
		if i > 0 {
			token = v.Tokens[i-1].Token
		}
		r.require(bytes.Equal(b[(4+i)*32:(5+i)*32], managedV2AddressWord(token)), "TOKEN_ORDER")
		balance := r.uintWord(b[(5+n+i)*32:(6+n+i)*32], 112)
		info := r.words(r.call(v.ExpectedVault, "getPoolTokenInfo(bytes32,address)", idArg+hex.EncodeToString(managedV2AddressWord(token)), 4), 4)
		cash := r.uintWord(info[0], 112)
		managed := r.uintWord(info[1], 112)
		changed := r.uintWord(info[2], 32).Uint64()
		if changed > maxTokenChange {
			maxTokenChange = changed
		}
		am := r.uintWord(info[3], 160)
		r.require(new(big.Int).Add(cash, managed).Cmp(balance) == 0 && changed <= last, "TOKEN_BALANCE")
		r.require(managed.Sign() == 0 || am.Sign() != 0, "TOKEN_ASSET_MANAGER")
		if i == 0 {
			// PoolRegistrationLib.registerComposablePool permanently registers
			// BPT first with no asset manager, so its managed balance stays zero.
			r.require(managed.Sign() == 0 && am.Sign() == 0, "BPT_ASSET_MANAGER")
			m.BPTIndex = 0
			if isBPT {
				m.BPT = &domain.ManagedV2BptState{BPTLastChangeBlock: changed, AllowlistMembers: []domain.ManagedV2AllowlistMember{}}
			}
			m.VaultBPTCash = domain.Amount(cash.String())
			m.VaultBPTManaged = domain.Amount(managed.String())
			continue
		}
		t := v.Tokens[i-1]
		r.code(t.Token, t.ExpectedCodeSHA256)
		// The Vault wallet aggregates all pools and internal balances. It must
		// cover this registration's cash, but need not equal that cash.
		wallet := r.uint(token, "balanceOf(address)", hex.EncodeToString(managedV2AddressWord(v.ExpectedVault)), 256)
		r.require(wallet.Cmp(cash) >= 0, "TOKEN_VAULT_CASH")
		decimals := r.uint(t.Token, "decimals()", "", 8)
		r.require(decimals.Uint64() <= 18, "DECIMALS")
		scale := new(big.Int).Exp(big.NewInt(10), new(big.Int).Sub(big.NewInt(36), decimals), nil)
		r.require(scales[i-1] == domain.Amount(scale.String()), "SCALING")
		breaker := r.words(r.call(p.ID, "getCircuitBreakerState(address)", hex.EncodeToString(managedV2AddressWord(token)), 6), 6)
		if isBreaker {
			a.readManagedV2CircuitToken(r, p.ID, token, domain.Amount(scale.String()), s.Weights[i-1], m, breaker, links["CircuitBreakerLib"])
		} else {
			for _, w := range breaker {
				r.require(r.uintWord(w, 256).Sign() == 0, "CIRCUIT_BREAKER_UNSUPPORTED")
			}
		}
		s.Assets = append(s.Assets, domain.MultiAsset{Token: t.Token, Balance: domain.Amount(balance.String()), Scaling: scales[i-1], CodeSHA256: t.ExpectedCodeSHA256})
		m.TokenInfo = append(m.TokenInfo, domain.ManagedV2TokenInfo{Cash: domain.Amount(cash.String()), VaultActualBalance: domain.Amount(wallet.String()), Managed: domain.Amount(managed.String()), AssetManager: "0x" + hex.EncodeToString(managedV2Word(am)[12:]), LastChangeBlock: changed, CircuitBreakerEnabled: isBreaker && m.CircuitBreakers.Tokens[len(m.CircuitBreakers.Tokens)-1].StateWord.Big().Sign() != 0})
	}
	// Vault BalanceAllocation.totalsAndLastChangeBlock returns the maximum
	// across all token registrations, including BPT.
	r.require(last == maxTokenChange, "POOL_LAST_CHANGE")
	m.TotalSupply = domain.Amount(r.uint(p.ID, "totalSupply()", "", 256).String())
	m.ActualSupply = domain.Amount(r.uint(p.ID, "getActualSupply()", "", 256).String())
	if r.err != nil {
		return fail(r.err)
	}
	own := new(big.Int).Add(m.VaultBPTCash.Big(), m.VaultBPTManaged.Big())
	virtual := new(big.Int).Sub(m.TotalSupply.Big(), own)
	pending := new(big.Int).Sub(m.ActualSupply.Big(), virtual)
	r.require(virtual.Sign() >= 0 && pending.Sign() >= 0, "SUPPLY")
	m.VirtualSupply = domain.Amount(virtual.String())
	m.PendingAUM = domain.Amount(pending.String())
	expectedPending, e := managedV2PendingAUM(virtual, m.ManagementAUMFee.Big(), m.Timestamp, uint64(m.LastCollectionTimestamp))
	if e != nil {
		return fail(e)
	}
	r.require(pending.Cmp(expectedPending) == 0, "PENDING_AUM")
	vaultBPT := r.uint(p.ID, "balanceOf(address)", hex.EncodeToString(managedV2AddressWord(v.ExpectedVault)), 256)
	r.require(vaultBPT.Cmp(m.VaultBPTCash.Big()) >= 0, "BPT_VAULT_CASH")
	m.VaultBPTActualBalance = domain.Amount(vaultBPT.String())
	m.ManagerBPTBalance = domain.Amount(r.uint(p.ID, "balanceOf(address)", hex.EncodeToString(managedV2AddressWord(manager)), 256).String())
	m.ProtocolCollectorBPTBalance = domain.Amount(r.uint(p.ID, "balanceOf(address)", hex.EncodeToString(managedV2AddressWord(collector)), 256).String())
	// Count each actual wallet once, including recipient/Vault aliases. The
	// Vault's global balance may include BPT registered to another pool.
	wallets := map[string]*big.Int{v.ExpectedVault: vaultBPT}
	for _, recipient := range []struct {
		address string
		balance *big.Int
	}{{manager, m.ManagerBPTBalance.Big()}, {collector, m.ProtocolCollectorBPTBalance.Big()}} {
		if previous, ok := wallets[recipient.address]; ok {
			r.require(previous.Cmp(recipient.balance) == 0, "BPT_WALLET_ALIAS")
		}
		wallets[recipient.address] = recipient.balance
	}
	walletTotal := new(big.Int)
	for _, balance := range wallets {
		walletTotal.Add(walletTotal, balance)
	}
	r.require(walletTotal.Cmp(m.TotalSupply.Big()) <= 0, "BPT_WALLET_SUPPLY")
	if p.Protocol == domain.ManagedV2ScheduledProtocol || isBPT {
		a.readManagedV2ScheduledState(r, p.ID, s, fee)
	} else {
		a.readManagedV2Schedules(r, p.ID, s.Weights, fee, m)
	}
	if r.err != nil {
		return fail(r.err)
	}
	if isBPT {
		// Published ManagedPoolAumStorageLib: fee u60, collection time u32
		// at bit60, unused bits92..255. Recovery exposes an effective zero
		// getter while retaining the stored normal-operation annual fee.
		aumWord := r.storageWord(p.ID, "0xa").Big()
		rawFee := managedV2ScheduledBits(aumWord, 0, 60)
		rawLast := managedV2ScheduledBits(aumWord, 60, 32)
		r.require(new(big.Int).Rsh(new(big.Int).Set(aumWord), 92).Sign() == 0, "AUM_RAW_RESERVED")
		r.require(rawFee.Cmp(big.NewInt(950000000000000000)) <= 0, "AUM_RAW_FEE_BOUND")
		r.require(rawLast.Uint64() == uint64(m.LastCollectionTimestamp), "AUM_RAW_COLLECTION_CLOCK")
		if !m.RecoveryMode {
			r.require(rawFee.Cmp(m.ManagementAUMFee.Big()) == 0, "AUM_RAW_EFFECTIVE_FEE")
		}
		m.BPT.JoinExitEnabled = r.boolean(p.ID, "getJoinExitEnabled()", "")
		m.BPT.MinimumBPTLocked = domain.Amount(r.uint(p.ID, "balanceOf(address)", hex.EncodeToString(managedV2AddressWord("0x0000000000000000000000000000000000000000")), 256).String())
		for _, subject := range c.BPT.AllowlistSubjects {
			encoded := append(managedV2AddressWord(subject), managedV2Word(big.NewInt(13))...)
			key := "0x" + hex.EncodeToString(managedV2Hash(string(encoded)))
			word := r.storageWord(p.ID, key).Big()
			r.require(word.BitLen() <= 1, "BPT_ALLOWLIST_STORAGE_BOOL")
			allowed := r.boolean(p.ID, "isAddressOnAllowlist(address)", hex.EncodeToString(managedV2AddressWord(subject)))
			r.require(allowed == (word.Sign() != 0), "BPT_ALLOWLIST_SOURCE_MISMATCH")
			m.BPT.AllowlistMembers = append(m.BPT.AllowlistMembers, domain.ManagedV2AllowlistMember{Address: subject, Allowed: allowed})
		}
	}
	if r.err != nil {
		return fail(r.err)
	}
	if e := a.verifyManagedV2Identity(r, p, codes, links, feeIDs, pauseEnd, bufferEnd, vaultPauseEnd, vaultBufferEnd); e != nil {
		return fail(e)
	}
	adapterVersion := domain.ManagedV2AdapterVersion
	if p.Protocol == domain.ManagedV2ScheduledProtocol {
		adapterVersion = domain.ManagedV2ScheduledAdapterVersion
	}
	if isBPT {
		adapterVersion = domain.ManagedV2BptAdapterVersion
	}
	if isBreaker {
		adapterVersion = domain.ManagedV2BreakerAdapterVersion
	}
	pool := domain.Pool{MultiAsset: s, ChainKey: a.Chain.Key, PoolID: p.ID, Protocol: p.Protocol, Token0: p.Token0, Token1: p.Token1, Reserve0: s.Assets[0].Balance, Reserve1: s.Assets[1].Balance, FeeNumerator: fee.Uint64(), FeeDenominator: 1000000000000000000, AdapterVersion: adapterVersion, TokenBehavior: "standard"}
	return pool, pool.Validate()
}

// Match ExternalFees.bptForPoolOwnershipPercentage followed by
// ExternalAUMFees.getAumFeesBptAmount from the pinned 20230411 build input.
// Each original Math.mul is checked before division; wide intermediates must
// not accept a getter result for which the actual Solidity call would revert.
func managedV2PendingAUM(virtual, fee *big.Int, timestamp, last uint64) (*big.Int, error) {
	if virtual.Sign() < 0 || fee.Sign() < 0 || fee.Cmp(big.NewInt(950000000000000000)) > 0 {
		return nil, errors.New("MANAGED_V2_AUM_PARAMETERS")
	}
	pending := new(big.Int)
	if timestamp <= last || fee.Sign() == 0 {
		return pending, nil
	}
	annual := new(big.Int).Mul(virtual, fee)
	if annual.BitLen() > 256 {
		return nil, errors.New("MANAGED_V2_AUM_OVERFLOW")
	}
	annual.Quo(annual, new(big.Int).Sub(big.NewInt(1000000000000000000), fee))
	pending.Mul(annual, new(big.Int).SetUint64(timestamp-last))
	if pending.BitLen() > 256 {
		return nil, errors.New("MANAGED_V2_AUM_OVERFLOW")
	}
	pending.Quo(pending, big.NewInt(365*24*60*60))
	if new(big.Int).Add(virtual, pending).BitLen() > 256 {
		return nil, errors.New("MANAGED_V2_AUM_OVERFLOW")
	}
	return pending, nil
}

func (a *Adapter) readManagedV2Schedules(r *managedV2Reader, pool string, weights []domain.Amount, swapFee *big.Int, m *domain.BalancerManagedV2State) {
	n := len(weights)
	fee := r.words(r.call(pool, "getGradualSwapFeeUpdateParams()", "", 4), 4)
	start := r.uintWord(fee[0], 32).Uint64()
	end := r.uintWord(fee[1], 32).Uint64()
	r.require(start <= end && end <= m.Timestamp, "ACTIVE_FEE_SCHEDULE_UNSUPPORTED")
	m.GradualFeeEnd = end
	r.require(r.uintWord(fee[3], 64).Cmp(swapFee) == 0, "STATIC_FEE_SCHEDULE_MISMATCH")
	for _, w := range fee[2:] {
		v := r.uintWord(w, 64)
		r.require(v.Cmp(big.NewInt(1000000000000)) >= 0 && v.Cmp(big.NewInt(950000000000000000)) <= 0, "SCHEDULE_FEE")
	}
	b := r.call(pool, "getGradualWeightUpdateParams()", "", -1)
	r.require(len(b) == (6+2*n)*32, "WEIGHT_SCHEDULE_SIZE")
	if r.err != nil {
		return
	}
	start = r.uintWord(b[:32], 32).Uint64()
	end = r.uintWord(b[32:64], 32).Uint64()
	m.GradualWeightEnd = end
	r.require(start <= end && end <= m.Timestamp, "ACTIVE_WEIGHT_SCHEDULE_UNSUPPORTED")
	r.require(r.uintWord(b[64:96], 64).Uint64() == 128 && r.uintWord(b[96:128], 64).Uint64() == uint64((5+n)*32) && r.uintWord(b[128:160], 64).Uint64() == uint64(n) && r.uintWord(b[(5+n)*32:(6+n)*32], 64).Uint64() == uint64(n), "WEIGHT_SCHEDULE_LAYOUT")
	for _, at := range []int{5, 6 + n} {
		sum := new(big.Int)
		for i := 0; i < n; i++ {
			v := r.uintWord(b[(at+i)*32:(at+i+1)*32], 64)
			r.require(v.Cmp(big.NewInt(10000000000000000)) >= 0 && v.Cmp(big.NewInt(1000000000000000000)) <= 0, "SCHEDULE_WEIGHT")
			if at == 6+n {
				r.require(v.String() == weights[i].String(), "STATIC_WEIGHT_SCHEDULE_MISMATCH")
			}
			sum.Add(sum, v)
		}
		r.require(sum.Cmp(big.NewInt(1000000000000000000)) == 0, "SCHEDULE_WEIGHT_SUM")
	}
}

func (a *Adapter) verifyManagedV2Identity(r *managedV2Reader, p config.PoolConfig, codes map[string][]byte, links map[string]string, feeIDs *big.Int, pauseEnd, bufferEnd, vaultPauseEnd, vaultBufferEnd []byte) error {
	v := p.MultiAsset
	c := v.ManagedV2
	// All addresses that affect swap/AUM execution are observed and bound twice:
	// once through their getters/links and once in their full original runtime.
	common := map[string][]byte{"_vault": managedV2AddressWord(v.ExpectedVault), "vault": managedV2AddressWord(v.ExpectedVault), "_protocolFeeProvider": managedV2AddressWord(c.ExpectedFeeProvider), "_protocolFeesCollector": managedV2AddressWord(c.ExpectedProtocolCollector), "_weightedMath": managedV2AddressWord(links["ExternalWeightedMath"]), "_recoveryModeHelper": managedV2AddressWord(links["RecoveryModeHelper"]), "_HASHED_VERSION": managedV2Hash("1"), "_TYPE_HASH": managedV2Hash("EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)")}
	match := func(name, address string, extra map[string][]byte) error {
		bindings := map[string][]byte{}
		for k, v := range common {
			bindings[k] = v
		}
		for k, v := range extra {
			bindings[k] = v
		}
		return managedV2Match(name, address, codes[address], bindings, links)
	}
	name := r.dynamicString(p.ID, "name()")
	id, _ := hex.DecodeString(v.PoolID[2:])
	if e := match("ManagedPool", p.ID, map[string][]byte{"_owner": managedV2AddressWord(c.ExpectedManager), "_poolId": id, "_feeIds": managedV2Word(feeIDs), "_actionIdDisambiguator": managedV2AddressWord(c.ExpectedFactory), "_pauseWindowEndTime": pauseEnd, "_bufferPeriodEndTime": bufferEnd, "_HASHED_NAME": managedV2Hash(name)}); e != nil {
		return e
	}
	for _, d := range c.Dependencies {
		if e := match(managedV2Roles[d.Role], d.Address, nil); e != nil {
			return e
		}
	}
	for _, target := range []struct{ name, address string }{{"ProtocolFeesCollector", c.ExpectedProtocolCollector}, {"ProtocolFeePercentagesProvider", c.ExpectedFeeProvider}} {
		if e := match(target.name, target.address, map[string][]byte{"_actionIdDisambiguator": managedV2AddressWord(target.address)}); e != nil {
			return e
		}
	}
	weth := r.address(v.ExpectedVault, "WETH()", "")
	if e := match("Vault", v.ExpectedVault, map[string][]byte{"_actionIdDisambiguator": managedV2AddressWord(v.ExpectedVault), "_weth": managedV2AddressWord(weth), "_pauseWindowEndTime": vaultPauseEnd, "_bufferPeriodEndTime": vaultBufferEnd, "_HASHED_NAME": managedV2Hash("Balancer V2 Vault")}); e != nil {
		return e
	}
	// The factory stores the original linked creation bytecode in two ordinary
	// EIP-170 sized code contracts. Verify both pieces and every linked byte.
	partWords := r.words(r.call(c.ExpectedFactory, "getCreationCodeContracts()", "", 2), 2)
	parts := make([][]byte, 2)
	partAddresses := make([]string, 2)
	for i, w := range partWords {
		word := r.uintWord(w, 160)
		partAddresses[i] = "0x" + hex.EncodeToString(managedV2Word(word)[12:])
		var text string
		if r.err == nil {
			r.err = a.Client.Call(r.ctx, "eth_getCode", []any{partAddresses[i], r.anchor}, &text)
		}
		if r.err == nil {
			var e error
			parts[i], e = hex.DecodeString(safeManagedHex(text))
			if e != nil || len(parts[i]) == 0 || len(parts[i]) > 24576 {
				r.require(false, "CREATION_CODE_SIZE")
			}
		}
	}
	creation := managedV2Templates["ManagedPoolCreation"]
	expected, _ := hex.DecodeString(creation.Hex)
	for lib, offsets := range creation.Links {
		word := managedV2AddressWord(links[lib])
		if len(word) != 32 {
			return errors.New("MANAGED_V2_CREATION_LINK")
		}
		for _, at := range offsets {
			copy(expected[at:at+20], word[12:])
		}
	}
	r.require(bytes.Equal(append(append([]byte(nil), parts[0]...), parts[1]...), expected), "FACTORY_CREATION_CODE")
	// getPauseConfiguration returns remaining pause duration and the buffer only
	// while the factory window is open. The original absolute end is uint64 and
	// its remaining duration must exactly reproduce that getter at this header.
	factoryCode := codes[c.ExpectedFactory]
	end, e := managedV2Immutable("ManagedPoolFactory", "_poolsPauseWindowEndTime", factoryCode)
	if e != nil {
		return e
	}
	duration, e := managedV2Immutable("ManagedPoolFactory", "_bufferPeriodDuration", factoryCode)
	if e != nil {
		return e
	}
	endN := r.uintWord(end, 64).Uint64()
	durationN := r.uintWord(duration, 64).Uint64()
	pause := r.words(r.call(c.ExpectedFactory, "getPauseConfiguration()", "", 2), 2)
	remaining, buffer := uint64(0), uint64(0)
	// Source time is retained in the managed state; passed separately below via
	// reader timestamp so no block number or latest tag can substitute for it.
	sourceTime := r.timestamp
	if endN > sourceTime {
		remaining = endN - sourceTime
		buffer = durationN
	}
	r.require(r.uintWord(pause[0], 64).Uint64() == remaining && r.uintWord(pause[1], 64).Uint64() == buffer, "FACTORY_PAUSE_CONFIG")
	poolPause := r.uintWord(pauseEnd, 64).Uint64()
	poolBuffer := r.uintWord(bufferEnd, 64).Uint64()
	if endN > sourceTime {
		r.require(poolPause == endN && poolBuffer >= poolPause && poolBuffer-poolPause == durationN, "POOL_FACTORY_WINDOWS")
	} else {
		r.require(poolPause <= sourceTime && poolBuffer >= poolPause, "POOL_FACTORY_WINDOWS")
	}
	if r.err != nil {
		return r.err
	}
	return match("ManagedPoolFactory", c.ExpectedFactory, map[string][]byte{"_actionIdDisambiguator": managedV2AddressWord(c.ExpectedFactory), "_creationCodeContractA": managedV2AddressWord(partAddresses[0]), "_creationCodeContractB": managedV2AddressWord(partAddresses[1]), "_creationCodeSizeA": managedV2Word(big.NewInt(int64(len(parts[0])))), "_creationCodeSizeB": managedV2Word(big.NewInt(int64(len(parts[1])))), "_poolsPauseWindowEndTime": end, "_bufferPeriodDuration": duration})
}

func validateManagedV2Header(b RawBlock) error {
	if e := validateV2CompleteHeader(b); e != nil {
		return fmt.Errorf("MANAGED_V2_HEADER: %w", e)
	}
	return nil
}
