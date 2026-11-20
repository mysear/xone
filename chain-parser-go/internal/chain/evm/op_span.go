package evm

import (
	"bytes"
	"context"
	"encoding/binary"
	"encoding/json"
	"errors"
	"math/big"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/rlp"
)

// The report identifies the observed L2 bodies whose sequencer inputs match the span.
// Execution outputs and the contents of deposits require separate derivation.
type OPSpanReport struct {
	origins           []OPL1Origin
	VerifiedL1Origins int `json:"verified_l1_origins,omitempty"`

	FirstBlock       uint64 `json:"first_l2_number"`
	LastBlock        uint64 `json:"last_l2_number"`
	LastHash         string `json:"last_l2_hash"`
	UserTransactions int    `json:"user_transactions"`
}

type opSpanColumns struct {
	count, legacy                             int
	creates, parity, protected                big.Int
	signatures, recipients, data, nonces, gas []byte
}

func (c *opSpanColumns) add(raw []byte, timestamp uint64) error {
	var tx types.Transaction
	if err := tx.UnmarshalBinary(raw); err != nil {
		return err
	}
	var fields []any
	v, r, s := tx.RawSignatureValues()
	if !v.IsUint64() || tx.Protected() && tx.ChainId().Cmp(big.NewInt(10)) != 0 {
		return errors.New("OP_SPAN_SIGNATURE")
	}
	y := v.Uint64()
	switch tx.Type() {
	case types.LegacyTxType:
		base := uint64(27)
		if tx.Protected() {
			base = 55 // EIP-155 for chain ID 10
			c.protected.SetBit(&c.protected, c.legacy, 1)
		}
		y -= base
		c.legacy++
		fields = []any{tx.Value(), tx.GasPrice(), tx.Data()}
	case types.AccessListTxType:
		fields = []any{tx.Value(), tx.GasPrice(), tx.Data(), tx.AccessList()}
	case types.DynamicFeeTxType, types.SetCodeTxType:
		fields = []any{tx.Value(), tx.GasTipCap(), tx.GasFeeCap(), tx.Data(), tx.AccessList()}
		if tx.Type() == types.SetCodeTxType {
			if timestamp <= opIsthmusTime {
				return errors.New("OP_SPAN_TX_TYPE")
			}
			fields = append(fields, tx.SetCodeAuthorizations())
		}
	default:
		return errors.New("OP_SPAN_TX_TYPE")
	}
	if y > 1 || !crypto.ValidateSignatureValues(byte(y), r, s, true) {
		return errors.New("OP_SPAN_SIGNATURE")
	}
	if tx.To() == nil {
		c.creates.SetBit(&c.creates, c.count, 1)
	} else {
		c.recipients = append(c.recipients, tx.To().Bytes()...)
	}
	c.parity.SetBit(&c.parity, c.count, uint(y))
	var rs [64]byte
	r.FillBytes(rs[:32])
	s.FillBytes(rs[32:])
	c.signatures = append(c.signatures, rs[:]...)
	data, err := rlp.EncodeToBytes(fields)
	if err != nil {
		return err
	}
	if tx.Type() != types.LegacyTxType {
		c.data = append(c.data, tx.Type())
	}
	c.data = append(c.data, data...)
	c.nonces = binary.AppendUvarint(c.nonces, tx.Nonce())
	c.gas = binary.AppendUvarint(c.gas, tx.Gas())
	c.count++
	return nil
}

func spanBits(bits *big.Int, count int) []byte { return bits.FillBytes(make([]byte, (count+7)/8)) }

func verifyOPSpan(ctx context.Context, batch []byte, blocks []json.RawMessage) (*OPSpanReport, error) {
	const genesisTime = 1686068903
	const genesisHeight = 105235063
	const blockTime = 2
	if len(blocks) == 0 || len(blocks) > 1000000 {
		return nil, errors.New("OP_SPAN_BLOCK_COUNT")
	}
	remaining := 32 << 20
	var columns opSpanColumns
	var originBits big.Int
	var prev OPL1Origin
	var relative uint64
	var parent, counts []byte
	var report OPSpanReport
	for i, raw := range blocks {
		remaining -= len(raw)
		if remaining < 0 {
			return nil, errors.New("OP_SPAN_BYTE_LIMIT")
		}
		body, err := VerifyChainBlockJSON(ctx, "evm:10", raw, 32<<20)
		if err != nil {
			return nil, err
		}
		b := body.Block
		t, err := canonicalIndex(b.Timestamp)
		if err != nil || t < genesisTime || (t-genesisTime)%blockTime != 0 {
			return nil, errors.New("OP_SPAN_BLOCK_TIME")
		}
		h, err := canonicalIndex(b.Number)
		if err != nil || h != (t-genesisTime)/blockTime+genesisHeight {
			return nil, errors.New("OP_SPAN_BLOCK_HEIGHT")
		}
		if len(body.RawTransactions) == 0 {
			return nil, errors.New("OP_SPAN_ATTRIBUTES")
		}
		_, origin, err := parseOPOrigin(common.FromHex(body.RawTransactions[0]), t)
		if err != nil {
			return nil, err
		}
		if i == 0 {
			report.FirstBlock = h
			relative = t - genesisTime
			parent = common.FromHex(b.ParentHash)[:20]
		} else {
			if b.ParentHash != report.LastHash || h != report.LastBlock+1 {
				return nil, errors.New("OP_SPAN_BLOCK_LINK")
			}
			if origin.Number == prev.Number {
				if origin.Hash != prev.Hash || origin.Timestamp != prev.Timestamp || origin.Sequence == 0 || origin.Sequence-1 != prev.Sequence {
					return nil, errors.New("OP_SPAN_ORIGIN_SEQUENCE")
				}
			} else if origin.Number <= prev.Number || origin.Number-prev.Number != 1 || origin.Sequence != 0 || origin.Timestamp <= prev.Timestamp || origin.Hash == prev.Hash {
				return nil, errors.New("OP_SPAN_ORIGIN_TRANSITION")
			}
		}
		if i == 0 || origin.Number != prev.Number {
			report.origins = append(report.origins, origin)
		}
		if origin.Sequence == 0 {
			originBits.SetBit(&originBits, i, 1)
		}
		start := columns.count
		for _, tx := range body.RawTransactions[1:] {
			if err := ctx.Err(); err != nil {
				return nil, err
			}
			data := common.FromHex(tx)
			if len(data) > 0 && data[0] == 0x7e {
				if columns.count != start {
					return nil, errors.New("OP_SPAN_DEPOSIT_ORDER")
				}
				continue
			}
			if err := columns.add(data, t); err != nil {
				return nil, err
			}
		}
		counts = binary.AppendUvarint(counts, uint64(columns.count-start))
		prev = origin
		report.LastBlock = h
		report.LastHash = b.Hash
	}
	// Compare in wire order without allocating a second complete batch.
	prefix := binary.AppendUvarint([]byte{1}, relative)
	prefix = binary.AppendUvarint(prefix, prev.Number)
	prefix = append(prefix, parent...)
	prefix = append(prefix, common.FromHex(prev.Hash)[:20]...)
	prefix = binary.AppendUvarint(prefix, uint64(len(blocks)))
	for _, col := range [][]byte{prefix, spanBits(&originBits, len(blocks)), counts, spanBits(&columns.creates, columns.count), spanBits(&columns.parity, columns.count), columns.signatures, columns.recipients, columns.data, columns.nonces, columns.gas, spanBits(&columns.protected, columns.legacy)} {
		var ok bool
		batch, ok = bytes.CutPrefix(batch, col)
		if !ok {
			return nil, errors.New("OP_SPAN_CONTENT_MISMATCH")
		}
	}
	if len(batch) != 0 {
		return nil, errors.New("OP_SPAN_TRAILING_BYTES")
	}
	report.UserTransactions = columns.count
	return &report, ctx.Err()
}
