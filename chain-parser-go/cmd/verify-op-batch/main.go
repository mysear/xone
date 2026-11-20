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
	channel := flag.Bool("channel", false, "also verify a complete channel within this transaction")
	span := flag.String("span-blocks", "", "ordered OP Mainnet full block array to match to the complete span")
	flag.Parse()
	if !common.IsHexAddress(*sender) {
		return errors.New("OP_BATCH_SENDER")
	}
	proof, err := provenance.ReadOnlineBeaconProof(*file, *root)
	if err != nil {
		return err
	}
	var evidence evm.OPBatchEvidence
	if err = readJSON(*input, &evidence); err != nil {
		return err
	}
	ctx := context.Background()
	var report evm.OPBatchReport
	address := common.HexToAddress(*sender)
	switch {
	case *span != "":
		var blocks []json.RawMessage
		if err = readJSON(*span, &blocks); err != nil {
			return err
		}
		report, err = evm.VerifyOPBatchSpan(ctx, proof, address, evidence, blocks)
	case *channel:
		report, err = evm.VerifyOPBatchChannel(ctx, proof, address, evidence)
	default:
		report, err = evm.VerifyOPBatchBlobs(ctx, proof, address, evidence)
	}
	if err != nil {
		return err
	}
	return json.NewEncoder(os.Stdout).Encode(report)
}

func readJSON(path string, dst any) error {
	f, err := os.Open(path)
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
	return json.Unmarshal(raw, dst)
}

func main() {
	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
