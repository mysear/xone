// verify-op-batch verifies a finalized Ethereum blob transaction and its OP batch data.
package main

import (
	"context"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"github.com/ethereum/go-ethereum/common"
	"io"
	"os"

	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/provenance"
)

func run() error {
	root := flag.String("beacon-root", "", "trusted Ethereum mainnet beacon checkpoint root")
	file := flag.String("beacon-proof", "", "archived bootstrap, finality and committee updates JSON")
	sender := flag.String("sender", "", "expected OP Mainnet batch sender for this L1 block")
	input := flag.String("evidence", "", "Ethereum block, OP inbox transaction hash and block-ordered blobs JSON")
	flag.Parse()
	if !common.IsHexAddress(*sender) {
		return errors.New("OP_BATCH_SENDER")
	}
	proof, err := provenance.ReadOnlineBeaconProof(*file, *root)
	if err != nil {
		return err
	}
	f, err := os.Open(*input)
	if err != nil {
		return err
	}
	defer f.Close()
	const limit = 32 << 20
	raw, err := io.ReadAll(io.LimitReader(f, limit+1))
	if err != nil {
		return err
	}
	if len(raw) > limit {
		return errors.New("OP_BATCH_BYTE_LIMIT")
	}
	var evidence evm.OPBatchEvidence
	if err = json.Unmarshal(raw, &evidence); err != nil {
		return err
	}
	report, err := evm.VerifyOPBatchBlobs(context.Background(), proof, common.HexToAddress(*sender), evidence)
	if err != nil {
		return err
	}
	return json.NewEncoder(os.Stdout).Encode(report)
}

func main() {
	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
