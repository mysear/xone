package evm

import (
	"bytes"
	"context"
	"encoding/binary"
	"errors"
	"math/big"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/rlp"
)

// OP Mainnet OptimismPortalProxy and the version-zero deposit event.
var opPortal = common.HexToAddress("0xbEb5Fc579115071764c7423A4f12eDde41f106Ed")
var opDepositEvent = crypto.Keccak256Hash([]byte("TransactionDeposited(address,address,uint256,bytes)"))

func portalDeposit(log Log, block common.Hash, index uint64) (string, error) {
	if len(log.Topics) != 4 || common.HexToHash(log.Topics[3]) != (common.Hash{}) {
		return "", errors.New("OP_PORTAL_DEPOSIT_VERSION")
	}
	from, to := common.HexToHash(log.Topics[1]), common.HexToHash(log.Topics[2])
	if !allZero(from[:12]) || !allZero(to[:12]) {
		return "", errors.New("OP_PORTAL_DEPOSIT_ADDRESS")
	}
	data := common.FromHex(log.Data)
	if len(data) < 64 || !allZero(data[:31]) || data[31] != 32 || !allZero(data[32:56]) {
		return "", errors.New("OP_PORTAL_DEPOSIT_ABI")
	}
	size := binary.BigEndian.Uint64(data[56:64])
	data = data[64:]
	if size < 73 || size > uint64(len(data)) || uint64(len(data)) != (size+31)/32*32 || !allZero(data[size:]) {
		return "", errors.New("OP_PORTAL_DEPOSIT_SIZE")
	}
	opaque := data[:size]
	var preimage [64]byte
	copy(preimage[:32], block[:])
	binary.BigEndian.PutUint64(preimage[56:], index)
	id := crypto.Keccak256(preimage[:])
	clear(preimage[:32])
	copy(preimage[32:], id)
	deposit := opDepositWire{SourceHash: crypto.Keccak256Hash(preimage[:]), From: common.BytesToAddress(from[:]), Mint: new(big.Int).SetBytes(opaque[:32]), Value: new(big.Int).SetBytes(opaque[32:64]), Gas: binary.BigEndian.Uint64(opaque[64:72]), Data: opaque[73:]}
	switch opaque[72] {
	case 0:
		deposit.To = to[12:]
	case 1:
	default:
		return "", errors.New("OP_PORTAL_DEPOSIT_CREATION")
	}
	raw, err := rlp.EncodeToBytes(deposit)
	if err != nil {
		return "", err
	}
	return "0x7e" + common.Bytes2Hex(raw), nil
}

func allZero(data []byte) bool { return len(bytes.Trim(data, "\x00")) == 0 }

// Receipt and log order determine the deposit source hash. Ignore receipt RPC
// array order, and use log indices only after whole-block validation.
func verifyPortalDeposits(ctx context.Context, l1 RawBlock, receipts []Receipt, body ChainBlockBodyProof) ([]string, error) {
	if err := VerifyReceiptRoot(ctx, l1, receipts); err != nil {
		return nil, err
	}
	ordered := make([]*Receipt, len(receipts))
	for i := range receipts {
		n, _ := canonicalIndex(receipts[i].TxIndex)
		ordered[n] = &receipts[i]
	}
	hashes := []string{}
	blockHash := common.HexToHash(l1.Hash)
	for _, receipt := range ordered {
		if err := ctx.Err(); err != nil {
			return nil, err
		}
		if receipt.Status != "0x1" {
			continue
		}
		for _, log := range receipt.Logs {
			if common.HexToAddress(log.Address) != opPortal || len(log.Topics) == 0 || common.HexToHash(log.Topics[0]) != opDepositEvent {
				continue
			}
			index, _ := canonicalIndex(log.Index)
			wire, err := portalDeposit(log, blockHash, index)
			if err != nil {
				return nil, err
			}
			pos := 1 + len(hashes)
			if pos >= len(body.RawTransactions) || body.RawTransactions[pos] != wire {
				return nil, errors.New("OP_PORTAL_DEPOSIT_CONTENT")
			}
			hashes = append(hashes, body.Block.Transactions[pos])
		}
	}
	for _, wire := range body.RawTransactions[1+len(hashes):] {
		if len(wire) >= 4 && wire[:4] == "0x7e" {
			return nil, errors.New("OP_PORTAL_EXTRA_DEPOSIT")
		}
	}
	return hashes, nil
}
