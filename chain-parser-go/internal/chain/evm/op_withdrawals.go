package evm

import (
	"bytes"
	"encoding/json"
	"errors"

	"github.com/ethereum/go-ethereum/core/types"
)

const opMessagePasserAddress = "0x4200000000000000000000000000000000000016"

// Called with the block read session locked and the replayed state anchor set.
func (batch *accountProofBatch) verifyOPMessagePasserRoot(root string) (int, error) {
	session := batch.block.nativeSession
	limits, err := session.available()
	if err != nil {
		return 0, err
	}
	budget := receiptProofBudget{remaining: limits.MaxBytes}
	plan, err := planContractStorage(batch.ctx, ContractStorageSource{ChainKey: batch.adapter.Chain.Key, GenesisHash: batch.adapter.Chain.Genesis}, *batch.anchor, nil, &budget)
	if err != nil {
		return 0, err
	}
	planBytes := limits.MaxBytes - budget.remaining
	limits.MaxBytes = budget.remaining
	account, used, err := batch.adapter.readNativeContractAccountProof(batch.ctx, *batch.anchor, opMessagePasserAddress, plan, limits, &batch.endpoint)
	if err != nil {
		return 0, err
	}
	if err = checkOPMessagePasserRoot(account.Account, root); err != nil {
		return 0, err
	}
	return used + planBytes, session.consume(used + planBytes)
}

func checkOPMessagePasserRoot(account VerifiedAccountCommitment, root string) error {
	if !account.Exists || account.Address != opMessagePasserAddress || account.provenStorageRoot == nil || account.provenStorageRoot.Hex() != root {
		return errors.New("OP_ISTHMUS_MESSAGE_PASSER_ROOT")
	}
	return nil
}

// OP Mainnet's Canyon payload has an empty withdrawals list. From Isthmus
// the header commits to the message passer's storage root instead of the
// empty list's trie. Checking that storage root requires separate state proof.
func verifyOPMainnetWithdrawalFields(header *types.Header, list json.RawMessage) error {
	const canyon = 1704992401
	if header.Time < canyon {
		if header.WithdrawalsHash != nil || (len(list) != 0 && !bytes.Equal(list, []byte("null"))) {
			return errors.New("OP_WITHDRAWALS_BEFORE_CANYON")
		}
	} else {
		if header.WithdrawalsHash == nil || len(list) < 2 || list[0] != '[' || list[len(list)-1] != ']' || len(bytes.TrimSpace(list[1:len(list)-1])) != 0 {
			return errors.New("OP_WITHDRAWALS_EMPTY_LIST_REQUIRED")
		}
		if header.Time < opIsthmusTime && *header.WithdrawalsHash != types.EmptyWithdrawalsHash {
			return errors.New("OP_CANYON_WITHDRAWALS_ROOT")
		}
	}
	if header.Time >= opIsthmusTime {
		if header.RequestsHash == nil || *header.RequestsHash != types.EmptyRequestsHash {
			return errors.New("OP_ISTHMUS_REQUESTS_HASH")
		}
	} else if header.RequestsHash != nil {
		return errors.New("OP_REQUESTS_BEFORE_ISTHMUS")
	}
	return nil
}
