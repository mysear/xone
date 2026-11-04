package domain

import (
	"bytes"
	"encoding/hex"
	"errors"
	"math"
	"strconv"
	"strings"
)

const RaydiumCPMMProgram = "CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C"
const RaydiumProgramData = "DMawCQzbgNTmbzaESc7o6pvL1KAeetY8zA7jNpzntHhU"
const RaydiumTokenProgram = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
const SolanaClock = "SysvarC1ock11111111111111111111111111111111"

func ValidateSolanaPubkey(s string) error {
	b, e := decode58(s)
	if e != nil || encode58(b) != s || bytes.Equal(b, make([]byte, 32)) {
		return errors.New("SOLANA_PUBKEY_IDENTITY")
	}
	return nil
}

type RaydiumCPMMState struct {
	ProgramID              string `json:"program_id"`
	Programdata            string `json:"programdata"`
	ProgramCodeSHA256      string `json:"program_code_sha256"`
	DeployedSlot           uint64 `json:"deployed_slot"`
	TokenProgramCodeSHA256 string `json:"token_program_code_sha256"`
	Authority              string `json:"authority"`
	AuthorityBump          uint32 `json:"authority_bump"`
	AmmConfig              string `json:"amm_config"`
	Vault0                 string `json:"vault0"`
	Vault1                 string `json:"vault1"`
	RawVault0              uint64 `json:"raw_vault0"`
	RawVault1              uint64 `json:"raw_vault1"`
	ProtocolFeeRate        uint64 `json:"protocol_fee_rate"`
	FundFeeRate            uint64 `json:"fund_fee_rate"`
	CreatorFeeRate         uint64 `json:"creator_fee_rate"`
	CreatorFeeShareRate    uint64 `json:"creator_fee_share_rate"`
	CreatorFeeOn           uint32 `json:"creator_fee_on"`
	CreatorFeeEnabled      bool   `json:"creator_fee_enabled"`
	ProtocolFees0          uint64 `json:"protocol_fees0"`
	ProtocolFees1          uint64 `json:"protocol_fees1"`
	FundFees0              uint64 `json:"fund_fees0"`
	FundFees1              uint64 `json:"fund_fees1"`
	CreatorFees0           uint64 `json:"creator_fees0"`
	CreatorFees1           uint64 `json:"creator_fees1"`
	BankSlot               uint64 `json:"bank_slot"`
	BankUnixTimestamp      uint64 `json:"bank_unix_timestamp"`
	BankEpoch              uint64 `json:"bank_epoch"`
	OpenTime               uint64 `json:"open_time"`
	RecentEpoch            uint64 `json:"recent_epoch"`
	Mint0Decimals          uint32 `json:"mint0_decimals"`
	Mint1Decimals          uint32 `json:"mint1_decimals"`
}

func (s RaydiumCPMMState) NetReserves() (uint64, uint64, error) {
	net := func(raw, protocol, fund, creator uint64) (uint64, error) {
		if protocol > math.MaxUint64-fund || protocol+fund > math.MaxUint64-creator {
			return 0, errors.New("RAYDIUM_FEE_OVERFLOW")
		}
		fee := protocol + fund + creator
		if fee > raw {
			return 0, errors.New("RAYDIUM_RESERVE_UNDERFLOW")
		}
		return raw - fee, nil
	}
	a, e := net(s.RawVault0, s.ProtocolFees0, s.FundFees0, s.CreatorFees0)
	if e != nil {
		return 0, 0, e
	}
	b, e := net(s.RawVault1, s.ProtocolFees1, s.FundFees1, s.CreatorFees1)
	return a, b, e
}
func (s RaydiumCPMMState) validateWire() error {
	if s.ProgramID != RaydiumCPMMProgram || s.Programdata != RaydiumProgramData || s.RecentEpoch > s.BankEpoch || (s.Authority != "GpMZbSM2GgvTKHJirzeGfMFoaZ8UR2X7F4v8vHTvxFbL" || s.AuthorityBump != 253) || s.CreatorFeeOn > 2 || s.Mint0Decimals > 255 || s.Mint1Decimals > 255 || s.DeployedSlot > s.BankSlot || s.BankUnixTimestamp > math.MaxInt64 || s.OpenTime > s.BankUnixTimestamp {
		return errors.New("RAYDIUM_VERSION_OR_BANK")
	}
	for _, key := range []string{s.Programdata, s.Authority, s.AmmConfig, s.Vault0, s.Vault1} {
		if e := ValidateSolanaPubkey(key); e != nil {
			return e
		}
	}
	for _, hash := range []string{s.ProgramCodeSHA256, s.TokenProgramCodeSHA256} {
		b, e := hex.DecodeString(hash)
		if e != nil || len(b) != 32 || hash != strings.ToLower(hash) {
			return errors.New("RAYDIUM_CODE_IDENTITY")
		}
	}
	if s.ProtocolFeeRate > 1000000 || s.FundFeeRate > 1000000 || s.ProtocolFeeRate+s.FundFeeRate > 1000000 || s.CreatorFeeRate > 1000000 || s.CreatorFeeShareRate > 1000000 {
		return errors.New("RAYDIUM_FEE_RANGE")
	}
	_, _, e := s.NetReserves()
	return e
}
func (s RaydiumCPMMState) Validate(p Pool) error {
	if e := s.validateWire(); e != nil {
		return e
	}
	if !strings.HasPrefix(p.ChainKey, "solana:") || ValidateChain(p.ChainKey) != nil || p.Protocol != "raydium-cpmm-creator" || p.AdapterVersion != "raydium-cpmm/f4b9b263-spl-1" || p.TokenBehavior != "standard" || p.FeeDenominator != 1000000 || p.FeeNumerator >= 1000000 || p.V2 != nil || p.V3 != nil || p.Stable != nil || p.SingletonCL != nil {
		return errors.New("RAYDIUM_POOL_IDENTITY")
	}
	seen := map[string]bool{}
	for _, key := range []string{p.PoolID, p.Token0, p.Token1, s.ProgramID, s.Programdata, s.Authority, s.AmmConfig, s.Vault0, s.Vault1, RaydiumTokenProgram, SolanaClock} {
		if e := ValidateSolanaPubkey(key); e != nil {
			return e
		}
		if seen[key] {
			return errors.New("RAYDIUM_ACCOUNT_ALIAS")
		}
		seen[key] = true
	}
	a, _ := decode58(p.Token0)
	b, _ := decode58(p.Token1)
	if bytes.Compare(a, b) >= 0 {
		return errors.New("RAYDIUM_MINT_ORDER")
	}
	r0, r1, e := s.NetReserves()
	if e != nil {
		return e
	}
	if p.Reserve0.String() != raydiumAmount(r0).String() || p.Reserve1.String() != raydiumAmount(r1).String() {
		return errors.New("RAYDIUM_RESERVE_MISMATCH")
	}
	return nil
}
func raydiumAmount(n uint64) Amount { return Amount(strconv.FormatUint(n, 10)) }

func SolanaPubkeyOrder(a, b string) int {
	x, _ := decode58(a)
	y, _ := decode58(b)
	return bytes.Compare(x, y)
}
