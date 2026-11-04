package domain

import (
	"errors"
	"math/big"
	"strings"
)

// CurveNGState retains effective-to-storage accounting and both oracle packs.
// The versioned standard-token profile has fixed decimal rates and no callbacks.
type CurveNGState struct {
	StoredBalances      []Amount `json:"stored_balances"`
	AdminBalances       []Amount `json:"admin_balances"`
	LastPricesPacked    []Amount `json:"last_prices_packed"`
	LastDPacked         Amount   `json:"last_d_packed"`
	MALastTime          Amount   `json:"ma_last_time"`
	MAExpTime           uint64   `json:"ma_exp_time"`
	DMaTime             uint64   `json:"d_ma_time"`
	Timestamp           uint64   `json:"timestamp"`
	BlockNumber         uint64   `json:"block_number"`
	BlockHash           string   `json:"block_hash"`
	OffpegFeeMultiplier Amount   `json:"offpeg_fee_multiplier"`
}

func (p Pool) validateCurveNG() error {
	s := p.MultiAsset
	if s == nil || s.CurveNG == nil {
		return errors.New("CURVE_NG_STATE_REQUIRED")
	}
	g := s.CurveNG
	n := len(s.Assets)
	if n < 2 || n > 8 || len(s.Weights) != 0 || s.Vault != "" || s.VaultCodeSHA256 != "" || s.PoolID != "" || len(g.StoredBalances) != n || len(g.AdminBalances) != n || len(g.LastPricesPacked) != n-1 || g.MAExpTime == 0 || g.DMaTime == 0 || g.Timestamp == 0 || g.Timestamp > ^uint64(0)/1000 || !strings.HasPrefix(g.BlockHash, "0x") || len(g.BlockHash) != 66 || !validDigest(g.BlockHash[2:]) || strings.Trim(g.BlockHash[2:], "0") == "" {
		return errors.New("CURVE_NG_PARAMETERS")
	}
	amp, e := ParseAmount(string(s.Amplification))
	if e != nil || amp.Big().Cmp(big.NewInt(100)) < 0 || amp.Big().Cmp(big.NewInt(100000000)) >= 0 || p.FeeNumerator > 5000000000 || s.AdminFee != 5000000000 {
		return errors.New("CURVE_NG_FEE_OR_AMP")
	}
	offpeg, e := ParseAmount(string(g.OffpegFeeMultiplier))
	if e != nil {
		return e
	}
	product := new(big.Int).Mul(offpeg.Big(), new(big.Int).SetUint64(p.FeeNumerator))
	if product.BitLen() > 256 || product.Cmp(new(big.Int).Mul(big.NewInt(5000000000), big.NewInt(10000000000))) > 0 {
		return errors.New("CURVE_NG_OFFPEG_FEE")
	}
	for i := range s.Assets {
		stored, e := ParseAmount(string(g.StoredBalances[i]))
		if e != nil {
			return e
		}
		admin, e := ParseAmount(string(g.AdminBalances[i]))
		if e != nil {
			return e
		}
		expected := new(big.Int).Add(s.Assets[i].Balance.Big(), admin.Big())
		if expected.BitLen() > 256 || stored.Big().Cmp(expected) != 0 {
			return errors.New("CURVE_NG_BALANCE_ACCOUNTING")
		}
	}
	limit := new(big.Int).Lsh(big.NewInt(1), 128)
	mask := new(big.Int).Sub(new(big.Int).Set(limit), big.NewInt(1))
	unpack := func(a Amount) (*big.Int, *big.Int, error) {
		v, e := ParseAmount(string(a))
		if e != nil {
			return nil, nil, e
		}
		return new(big.Int).And(v.Big(), mask), new(big.Int).Rsh(v.Big(), 128), nil
	}
	for _, a := range g.LastPricesPacked {
		spot, ema, e := unpack(a)
		if e != nil {
			return e
		}
		cap := big.NewInt(2000000000000000000)
		if spot.Cmp(cap) > 0 || ema.Cmp(cap) > 0 {
			return errors.New("CURVE_NG_ORACLE_PRICE")
		}
	}
	if _, _, e = unpack(g.LastDPacked); e != nil {
		return e
	}
	a, b, e := unpack(g.MALastTime)
	if e != nil {
		return e
	}
	now := new(big.Int).SetUint64(g.Timestamp)
	if a.Cmp(now) > 0 || b.Cmp(now) > 0 {
		return errors.New("CURVE_NG_ORACLE_TIME")
	}
	return nil
}
