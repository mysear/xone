// singleton-receipt-node exposes a local Geth chain for transaction-index receipt tests.
package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"math/big"
	"os"
	"strconv"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core"
	"github.com/ethereum/go-ethereum/eth"
	"github.com/ethereum/go-ethereum/eth/catalyst"
	"github.com/ethereum/go-ethereum/eth/ethconfig"
	"github.com/ethereum/go-ethereum/eth/tracers"
	_ "github.com/ethereum/go-ethereum/eth/tracers/native"
	"github.com/ethereum/go-ethereum/node"
	"github.com/ethereum/go-ethereum/p2p"
	"github.com/ethereum/go-ethereum/params"
)

func run() error {
	if len(os.Args) != 3 {
		return fmt.Errorf("usage: singleton-receipt-node GENESIS PORT")
	}
	port, err := strconv.Atoi(os.Args[2])
	if err != nil || port < 1 || port > 65535 {
		return fmt.Errorf("invalid port %q", os.Args[2])
	}
	data, err := os.ReadFile(os.Args[1])
	if err != nil {
		return err
	}
	var genesis core.Genesis
	if err := json.Unmarshal(data, &genesis); err != nil {
		return err
	}
	genesis.Config.BlobScheduleConfig = &params.BlobScheduleConfig{Cancun: params.DefaultCancunBlobConfig}
	config := node.DefaultConfig
	config.DataDir = ""
	config.HTTPHost = "127.0.0.1"
	config.HTTPPort = port
	config.HTTPModules = []string{"eth", "debug"}
	config.IPCPath = ""
	config.P2P = p2p.Config{NoDiscovery: true}
	stack, err := node.New(&config)
	if err != nil {
		return err
	}
	defer stack.Close()
	ethConfig := ethconfig.Defaults
	ethConfig.Genesis = &genesis
	ethConfig.SyncMode = ethconfig.FullSync
	ethConfig.Miner.GasCeil = genesis.GasLimit
	ethConfig.Miner.GasPrice = big.NewInt(1)
	ethConfig.TxPool.PriceLimit = 1
	ethConfig.TxPool.NoLocals = true
	backend, err := eth.New(stack, &ethConfig)
	if err != nil {
		return err
	}
	stack.RegisterAPIs(tracers.APIs(backend.APIBackend))
	if err := stack.Start(); err != nil {
		return err
	}
	beacon, err := catalyst.NewSimulatedBeacon(0, common.Address{}, backend)
	if err != nil {
		return err
	}
	defer beacon.Stop()
	if err := beacon.Fork(backend.BlockChain().GetCanonicalHash(0)); err != nil {
		return err
	}
	reader := bufio.NewScanner(os.Stdin)
	for reader.Scan() {
		if reader.Text() != "mine" {
			return fmt.Errorf("invalid command %q", reader.Text())
		}
		fmt.Println(beacon.Commit())
	}
	return reader.Err()
}

func main() {
	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
