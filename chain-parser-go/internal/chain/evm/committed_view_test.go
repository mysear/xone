package evm

import (
	"context"
	"errors"
	"strings"
	"testing"

	"github.com/ethereum/go-ethereum/common"
)

func viewInput() []byte { return append(common.FromHex("0x70a08231"), make([]byte, 32)...) }
func TestCommittedViewStopsAtUnprovedStorageBeforeZeroCanAffectControlFlow(t *testing.T) {
	// Read slot1; zero chooses slot3, nonzero chooses slot2. No first-pass
	// default may choose a branch before slot1 is proved.
	code := common.FromHex("0x60015415600d576002546011565b6003545b60005260206000f3")
	slots := map[common.Hash]common.Hash{}
	output, missing, err := executeCommittedView(context.Background(), code, common.HexToAddress("0x1234"), viewInput(), slots)
	if output != nil || missing == nil || *missing != common.HexToHash("0x1") || err == nil {
		t.Fatal(output, missing, err)
	}
	slots[*missing] = common.HexToHash("0x1")
	output, missing, err = executeCommittedView(context.Background(), code, common.HexToAddress("0x1234"), viewInput(), slots)
	if output != nil || missing == nil || *missing != common.HexToHash("0x2") || err == nil {
		t.Fatal("unproved zero changed branch", output, missing, err)
	}
	slots[*missing] = common.HexToHash("0x42")
	output, missing, err = executeCommittedView(context.Background(), code, common.HexToAddress("0x1234"), viewInput(), slots)
	if err != nil || missing != nil || common.BytesToHash(output) != common.HexToHash("0x42") {
		t.Fatal(output, missing, err)
	}
	slots[common.HexToHash("0x1")] = common.Hash{}
	slots[common.HexToHash("0x3")] = common.HexToHash("0x7")
	output, missing, err = executeCommittedView(context.Background(), code, common.HexToAddress("0x1234"), viewInput(), slots)
	if err != nil || missing != nil || common.BytesToHash(output) != common.HexToHash("0x7") {
		t.Fatal("proved zero branch failed", output, missing, err)
	}
}
func TestCommittedViewRejectsNonlocalEnvironmentWritesAndUnboundedExecution(t *testing.T) {
	for name, code := range map[string]string{
		"gas": "0x5a60005260206000f3", "block": "0x4360005260206000f3", "time": "0x4260005260206000f3",
		"balance": "0x60003160005260206000f3", "codehash": "0x60003f60005260206000f3",
		"write": "0x600160005560006000f3", "call": "0x6000600060006000600060006000f160005260206000f3",
		"delegatecall": "0x600060006000600060006000f460005260206000f3", "push0": "0x5f60005260206000f3",
		"loop": "0x5b600056", "huge-memory": "0x7f" + strings.Repeat("ff", 32) + "5160005260206000f3",
		"short-return": "0x60016000f3", "revert": "0x60006000fd",
	} {
		t.Run(name, func(t *testing.T) {
			out, missing, err := executeCommittedView(context.Background(), common.FromHex(code), common.HexToAddress("0x1234"), viewInput(), nil)
			if err == nil || out != nil || missing != nil {
				t.Fatal(out, missing, err)
			}
		})
	}
}
func TestCommittedViewBoundsInputsAndPreservesCancellation(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if out, missing, err := executeCommittedView(ctx, []byte{0}, common.Address{}, viewInput(), nil); !errors.Is(err, context.Canceled) || out != nil || missing != nil {
		t.Fatal(out, missing, err)
	}
	slots := map[common.Hash]common.Hash{}
	for n := 0; n < 9; n++ {
		slots[common.BytesToHash([]byte{byte(n)})] = common.Hash{}
	}
	if _, _, err := executeCommittedView(context.Background(), []byte{0}, common.Address{}, viewInput(), slots); err == nil {
		t.Fatal("too many slots accepted")
	}
	for _, code := range [][]byte{nil, make([]byte, 24577)} {
		if _, _, err := executeCommittedView(context.Background(), code, common.Address{}, viewInput(), nil); err == nil {
			t.Fatal("code limit")
		}
	}
	if _, _, err := executeCommittedView(context.Background(), []byte{0}, common.Address{}, nil, nil); err == nil {
		t.Fatal("input limit")
	}
}
