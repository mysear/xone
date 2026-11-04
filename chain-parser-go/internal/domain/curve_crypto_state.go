package domain

import (
	"encoding/hex"
	"errors"
	"math/big"
	"strings"
)

const CurveCryptoLPCodeSHA256 = "d3f43d24527d6a8ed0fa9f24d110ad7cc9325f40da335ed327e8e0e002a8694d"

// CurveCryptoRamp records the initial endpoint of the packed A/gamma schedule.
type CurveCryptoRamp struct {
	InitialAmp   Amount `json:"initial_amp"`
	InitialGamma Amount `json:"initial_gamma"`
	InitialTime  uint64 `json:"initial_time"`
}

// CurveCryptoState is the complete pricing and LP ledger for the pinned two-coin
// Crypto model. Amplification and Gamma retain the future endpoints.
type CurveCryptoState struct {
	Ramp                 *CurveCryptoRamp `json:"ramp,omitempty"`
	Gamma                Amount           `json:"gamma"`
	OutFee               Amount           `json:"out_fee"`
	FeeGamma             Amount           `json:"fee_gamma"`
	AllowedExtraProfit   Amount           `json:"allowed_extra_profit"`
	AdjustmentStep       Amount           `json:"adjustment_step"`
	Invariant            Amount           `json:"invariant"`
	PriceScale           Amount           `json:"price_scale"`
	PriceOracle          Amount           `json:"price_oracle"`
	LastPrices           Amount           `json:"last_prices"`
	XCPProfit            Amount           `json:"xcp_profit"`
	XCPProfitA           Amount           `json:"xcp_profit_a"`
	VirtualPrice         Amount           `json:"virtual_price"`
	TotalSupply          Amount           `json:"total_supply"`
	AdminReceiverBalance Amount           `json:"admin_receiver_balance"`
	MAHalfTime           uint64           `json:"ma_half_time"`
	LastPricesTimestamp  uint64           `json:"last_prices_timestamp"`
	NotAdjusted          bool             `json:"not_adjusted"`
	FutureAGammaTime     uint64           `json:"future_a_gamma_time"`
	LPToken              string           `json:"lp_token"`
	AdminReceiver        string           `json:"admin_receiver"`
	Timestamp            uint64           `json:"timestamp"`
	BlockNumber          uint64           `json:"block_number"`
	LPTokenCodeSHA256    string           `json:"lp_token_code_sha256"`
	RawBalances          []Amount         `json:"raw_balances"`
	BlockHash            string           `json:"block_hash"`
}

func (p Pool) validateCurveCrypto() error {
	s := p.MultiAsset
	if s == nil || s.CurveCrypto == nil {
		return errors.New("CURVE_CRYPTO_STATE_REQUIRED")
	}
	c := s.CurveCrypto
	if p.Protocol == "curve-crypto2-ramp" {
		r := c.Ramp
		if r == nil {
			return errors.New("CURVE_CRYPTO_RAMP_REQUIRED")
		}
		if r.InitialTime > c.Timestamp || r.InitialTime > ^uint64(0)/1000 || c.FutureAGammaTime > 1 && (r.InitialTime > c.FutureAGammaTime || c.FutureAGammaTime > ^uint64(0)/1000) {
			return errors.New("CURVE_CRYPTO_RAMP_TIME")
		}
		for _, a := range []Amount{r.InitialAmp, r.InitialGamma} {
			if _, e := ParseAmount(string(a)); e != nil {
				return e
			}
			if a.Big().BitLen() > 128 {
				return errors.New("CURVE_CRYPTO_RAMP_PARAMETER_DOMAIN")
			}
		}
		if r.InitialAmp.Big().Cmp(big.NewInt(4000)) < 0 || r.InitialAmp.Big().Cmp(big.NewInt(4000000000)) > 0 || r.InitialGamma.Big().Cmp(big.NewInt(10000000000)) < 0 || r.InitialGamma.Big().Cmp(big.NewInt(20000000000000000)) > 0 {
			return errors.New("CURVE_CRYPTO_RAMP_PARAMETER_DOMAIN")
		}
	}
	if s.CurveNG != nil || s.CurveMeta != nil || s.ManagedFees != nil || len(s.Assets) != 2 || len(s.Weights) != 0 || s.Vault != "" || s.VaultCodeSHA256 != "" || s.PoolID != "" || len(c.RawBalances) != 2 || c.Timestamp == 0 || c.Timestamp > ^uint64(0)/1000 || c.LastPricesTimestamp > c.Timestamp || p.Protocol == "curve-crypto2-full" && (c.FutureAGammaTime > 1 || c.Ramp != nil) {
		return errors.New("CURVE_CRYPTO_PARAMETERS")
	}
	if len(c.BlockHash) != 66 || !strings.HasPrefix(c.BlockHash, "0x") || !validDigest(c.BlockHash[2:]) || strings.Trim(c.BlockHash[2:], "0") == "" {
		return errors.New("CURVE_CRYPTO_BLOCK_HASH")
	}
	if ValidateEVMAddress(c.LPToken) != nil || strings.TrimPrefix(c.LPToken, "0x") == strings.Repeat("0", 40) || c.LPToken == p.PoolID || c.LPToken == s.Assets[0].Token || c.LPToken == s.Assets[1].Token || c.LPTokenCodeSHA256 != CurveCryptoLPCodeSHA256 || ValidateEVMAddress(c.AdminReceiver) != nil || c.AdminReceiver == p.PoolID || c.AdminReceiver == c.LPToken {
		return errors.New("CURVE_CRYPTO_LP_IDENTITY")
	}
	for _, a := range []Amount{c.Gamma, c.OutFee, c.FeeGamma, c.AllowedExtraProfit, c.AdjustmentStep, c.Invariant, c.PriceScale, c.PriceOracle, c.LastPrices, c.XCPProfit, c.XCPProfitA, c.VirtualPrice, c.TotalSupply, c.AdminReceiverBalance, s.Amplification} {
		if _, e := ParseAmount(string(a)); e != nil {
			return e
		}
	}
	wad := big.NewInt(1000000000000000000)
	if s.Amplification.Big().Cmp(big.NewInt(4000)) < 0 || s.Amplification.Big().Cmp(big.NewInt(4000000000)) > 0 || c.Gamma.Big().Cmp(big.NewInt(10000000000)) < 0 || c.Gamma.Big().Cmp(big.NewInt(20000000000000000)) > 0 || c.OutFee.Big().Cmp(big.NewInt(500000)) < 0 || c.OutFee.Big().Cmp(big.NewInt(10000000000)) > 0 || c.OutFee.Big().Cmp(new(big.Int).SetUint64(p.FeeNumerator)) < 0 || s.AdminFee > 10000000000 || c.FeeGamma.Big().Sign() == 0 || c.FeeGamma.Big().Cmp(wad) >= 0 || c.AllowedExtraProfit.Big().Cmp(wad) > 0 || c.AdjustmentStep.Big().Cmp(wad) > 0 || c.MAHalfTime == 0 || c.MAHalfTime >= 604800 {
		return errors.New("CURVE_CRYPTO_PARAMETER_DOMAIN")
	}
	for _, a := range []Amount{c.Invariant, c.PriceScale, c.PriceOracle, c.LastPrices, c.VirtualPrice, c.TotalSupply} {
		if a.Big().Sign() == 0 {
			return errors.New("CURVE_CRYPTO_PRICING_DOMAIN")
		}
	}
	if c.AdminReceiverBalance.Big().Cmp(c.TotalSupply.Big()) > 0 || c.AdminReceiver == "0x"+strings.Repeat("0", 40) && c.AdminReceiverBalance != "0" {
		return errors.New("CURVE_CRYPTO_LP_LEDGER")
	}
	for i, a := range c.RawBalances {
		if _, e := ParseAmount(string(a)); e != nil {
			return e
		}
		if a.Big().Cmp(s.Assets[i].Balance.Big()) < 0 {
			return errors.New("CURVE_CRYPTO_RAW_BALANCE")
		}
	}
	return nil
}

// Keep strict digest conversion even when a caller serializes without validating a pool.
func cryptoBytes(s string) ([]byte, error) {
	b, e := hex.DecodeString(strings.TrimPrefix(s, "0x"))
	if e != nil || len(b) != 32 {
		return nil, errors.New("CURVE_CRYPTO_BYTES_WIDTH")
	}
	return b, nil
}

// Admin LP mints may donate to another pool. That would require cross-pool raw
// balance propagation; reject that dependency until the execution model owns it.
func ValidatePoolDependencies(pools []Pool) error {
	if e := ValidateManagedV2PoolDependencies(pools); e != nil {
		return e
	}
	if e := ValidateInfinityBinDependencies(pools); e != nil {
		return e
	}
	if e := ValidateCurveMetaDependencies(pools); e != nil {
		return e
	}
	hasCrypto := false
	for _, p := range pools {
		hasCrypto = hasCrypto || p.MultiAsset != nil && p.MultiAsset.CurveCrypto != nil && p.MultiAsset.AdminFee != 0
	}
	if !hasCrypto {
		return nil
	}
	byID := map[string]Pool{}
	for _, p := range pools {
		byID[p.PoolID] = p
	}
	for _, p := range pools {
		if p.MultiAsset == nil || p.MultiAsset.CurveCrypto == nil {
			continue
		}
		c := p.MultiAsset.CurveCrypto
		if p.MultiAsset.AdminFee == 0 || c.AdminReceiver == "0x"+strings.Repeat("0", 40) {
			continue
		}
		receiver, ok := byID[c.AdminReceiver]
		if !ok {
			continue
		}
		holds := receiver.Token0 == c.LPToken || receiver.Token1 == c.LPToken
		if receiver.MultiAsset != nil {
			for _, a := range receiver.MultiAsset.Assets {
				holds = holds || a.Token == c.LPToken
			}
		}
		if holds {
			return errors.New("CURVE_CRYPTO_MUTABLE_LP_DEPENDENCY")
		}
	}
	return nil
}
