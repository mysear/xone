// verify-op-origin verifies an archived OP block's L1 origin under a caller-pinned beacon checkpoint.
package main

import (
	"context"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"os"

	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/provenance"
)

func run() error {
	root := flag.String("beacon-root", "", "trusted Ethereum mainnet beacon checkpoint root")
	file := flag.String("beacon-proof", "", "archived bootstrap, finality and committee updates JSON")
	input := flag.String("evidence", "", "OP execution block and descending Ethereum headers JSON")
	flag.Parse()
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
		return errors.New("OP_ORIGIN_BYTE_LIMIT")
	}
	var evidence evm.OPOriginEvidence
	if err = json.Unmarshal(raw, &evidence); err != nil {
		return err
	}
	report, err := evm.VerifyOPOrigin(context.Background(), proof, evidence)
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
