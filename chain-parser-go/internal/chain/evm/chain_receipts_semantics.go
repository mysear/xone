package evm

import (
	"crypto/sha256"
	"encoding/json"
	"errors"
	"fmt"
	"strconv"
)

type NativeSemanticsDescriptor struct {
	ID                    string            `json:"id,omitempty"`
	ChainKey              string            `json:"chain_key"`
	GenesisHash           string            `json:"genesis_hash"`
	ActivationID          string            `json:"activation_id"`
	ActivationHeight      *uint64           `json:"activation_height,omitempty"`
	ActivationTime        *uint64           `json:"activation_time,omitempty"`
	HeaderEncodingID      string            `json:"header_encoding_id"`
	ReceiptEncodingID     string            `json:"receipt_encoding_id"`
	TransactionIdentityID string            `json:"transaction_identity_id"`
	ReceiptAuthority      string            `json:"receipt_authority"`
	ReceiptCoverageScope  string            `json:"receipt_coverage_scope"`
	AccountEncodingID     string            `json:"account_encoding_id"`
	ArbOSVersion          *uint64           `json:"arbos_version,omitempty"`
	SourcePins            map[string]string `json:"source_pins"`
	Admitted              bool              `json:"admitted"`
}

// ResolveNativeSemantics accepts a verified native header commitment. Its
// deterministic ID includes chain fingerprint, active fork, header/receipt
// authority, account layout, ArbOS format version and fixed source commits.
// HistoryDatasetID must bind this ID instead of a universal Ethereum label.
func ResolveNativeSemantics(chainKey string, header ChainHeaderCommitment) (NativeSemanticsDescriptor, error) {
	var out NativeSemanticsDescriptor
	genesis, err := knownBodyGenesis(chainKey)
	if err != nil {
		return out, err
	}
	height, err := canonicalIndex(header.Block.Number)
	if err != nil {
		return out, err
	}
	timestamp, err := canonicalIndex(header.Block.Timestamp)
	if err != nil {
		return out, err
	}
	if header.HeaderEncoding == "" {
		return out, errors.New("NATIVE_SEMANTICS_HEADER_REQUIRED")
	}
	out = NativeSemanticsDescriptor{ChainKey: chainKey, GenesisHash: genesis, HeaderEncodingID: header.HeaderEncoding, ReceiptEncodingID: "eip2718-status-or-poststate-rlp/0..4", TransactionIdentityID: "keccak-encoded-transaction/header-body-trie", ReceiptAuthority: "own-header-receipts-root", ReceiptCoverageScope: "all-canonical-evm-receipts", AccountEncodingID: "ethereum-account-rlp/v1", SourcePins: map[string]string{}, Admitted: true}
	setHeight := func(name string, at uint64) {
		out.ActivationID = name
		value := at
		out.ActivationHeight = &value
		out.ActivationTime = nil
	}
	setTime := func(name string, at uint64) {
		out.ActivationID = name
		value := at
		out.ActivationTime = &value
		out.ActivationHeight = nil
	}
	switch chainKey {
	case "evm:1":
		out.SourcePins["ethereum/go-ethereum"] = "a01129526c55e4f4f42092160776390958c2a662"
		setHeight("ethereum/frontier", 0)
		if height >= 4370000 {
			setHeight("ethereum/byzantium-status-receipts", 4370000)
		}
		if height >= 12965000 {
			setHeight("ethereum/london", 12965000)
		}
		for _, fork := range []nativeTimeFork{{"ethereum/shanghai", 1681338455}, {"ethereum/cancun", 1710338135}, {"ethereum/prague", 1746612311}, {"ethereum/osaka", 1764798551}} {
			if timestamp >= fork.Time {
				setTime(fork.Name, fork.Time)
			}
		}
	case "evm:10", "evm:8453":
		out.SourcePins["ethereum-optimism/op-geth"] = "79e9d244748a1201fe686ca8a882b5fe7e173348"
		if chainKey == "evm:10" {
			out.SourcePins["ethereum-optimism/superchain-registry"] = "3273a28575300b76cf82f282d34c2221f81cbefd"
		} else {
			out.SourcePins["ethereum-optimism/superchain-registry/base"] = "5b055be4d294ce43814a44c8839d89b2436dc8aa"
		}
		genesisTime := uint64(1686068903)
		label := "op-mainnet"
		if chainKey == "evm:8453" {
			genesisTime = 1686789347
			label = "base-mainnet"
		}
		setTime(label+"/bedrock-regolith", genesisTime)
		if timestamp < genesisTime {
			out.Admitted = false
			setHeight(label+"/pre-bedrock-not-admitted", 0)
		}
		out.ReceiptEncodingID = "op-deposit-regolith-nonce-uncommitted/eip2718/v1"
		forks := []nativeTimeFork{{"canyon", 1704992401}, {"delta", 1708560000}, {"ecotone", 1710374401}, {"fjord", 1720627201}, {"granite", 1726070401}, {"holocene", 1736445601}, {"isthmus", 1746806401}, {"jovian", 1764691201}}
		if chainKey == "evm:10" {
			forks = append(forks, nativeTimeFork{"karst", 1783526401})
		}
		for _, fork := range forks {
			if timestamp >= fork.Time {
				setTime(label+"/"+fork.Name, fork.Time)
			}
		}
		if timestamp >= 1704992401 {
			out.ReceiptEncodingID = "op-canyon-deposit-nonce+version1/eip2718/v1"
		}
	case "evm:42161":
		out.SourcePins["OffchainLabs/go-ethereum"] = "260bc5dcbd5a16925d7e26145577b2612f30f428"
		out.SourcePins["OffchainLabs/nitro"] = "ccc5c828d9ea22f161efc784bac4d4a6559b7e59"
		out.ReceiptEncodingID = "nitro-native64,65,66,68,69,6a+legacy78/eip2718/v1"
		out.TransactionIdentityID = "nitro-keccak-native/classic-hashoverride-not-admitted"
		setHeight("arbitrum-one/nitro-genesis", 22207817)
		if height < 22207817 {
			out.Admitted = false
			setHeight("arbitrum-one/classic-authority-required", 0)
		} else if header.Nitro != nil {
			v := header.Nitro.ArbOSVersion
			out.ArbOSVersion = &v
			out.ActivationID = "arbitrum-one/nitro/arbos-" + strconv.FormatUint(v, 10)
		} else if height > 22207817 {
			return out, errors.New("NATIVE_SEMANTICS_NITRO_HEADER_REQUIRED")
		}
	case "evm:56":
		out.SourcePins["bnb-chain/bsc"] = "54afaff4da31ac9e49ba646a4f65104edac5a2f3"
		out.ReceiptEncodingID = "parlia-standard-rlp/native-zero-gas-cumulative-nondecreasing/v1"
		setHeight("bsc/parlia-genesis", 0)
		if height >= 31302048 {
			setHeight("bsc/hertz", 31302048)
		}
		if height >= 34140700 {
			setHeight("bsc/hertzfix", 34140700)
		}
		for _, fork := range []nativeTimeFork{{"kepler", 1705996800}, {"feynman", 1713419340}, {"haber", 1718863500}, {"haberfix", 1727316120}, {"bohr", 1727317200}, {"pascal", 1742436600}, {"lorentz", 1745903100}, {"maxwell", 1751250600}, {"fermi", 1768357800}, {"mendel", 1777343400}, {"pasteur", 1787625000}, {"jenner", 1796697000}} {
			if timestamp >= fork.Time {
				setTime("bsc/"+fork.Name, fork.Time)
			}
		}
	case "evm:137":
		out.SourcePins["0xPolygon/bor"] = "f8fc9081e424cea8ec0b6ec44d2e66c137970e8b"
		if height < 80084800 {
			setHeight("polygon/bor-pre-madhugiri", 0)
			out.ReceiptEncodingID = "bor-canonical-rlp/system-suffix-outside-root/v1"
			out.ReceiptCoverageScope = "canonical-transactions/system-isolated"
		} else {
			setHeight("polygon/bor-madhugiri", 80084800)
			out.ReceiptEncodingID = "bor-state-sync-7f-root-member/eip2718/v1"
		}
	case "evm:43114":
		out.SourcePins["ava-labs/coreth"] = "9d4ce7bf22db6a9e5d37cf26d643753e2d768208"
		out.SourcePins["ava-labs/avalanchego/v1.15.3"] = "983815fd993acf846a27749b598f0fea4d876a75"
		out.SourcePins["ava-labs/libevm"] = "d398bc4dbe412582a1eef24dee3262cab510777c"
		out.AccountEncodingID = "coreth-account-held-xchain-rlp/v1"
		out.ReceiptEncodingID = "coreth-eip2718-types0..3/v1"
		setHeight("avalanche/coreth-genesis", 0)
		for _, fork := range corethNativeTimeForks {
			if timestamp >= fork.Time {
				setTime("avalanche/"+fork.Name, fork.Time)
			}
		}
		if timestamp >= avalancheHeliconTime {
			if header.SettledHeight == nil {
				return out, errors.New("NATIVE_SEMANTICS_SAE_HEADER_REQUIRED")
			}
			out.ReceiptAuthority = "successor-sae-settling-window"
			out.ReceiptCoverageScope = "complete-settled-window/target-evm-receipts"
		}
	}
	encoded, err := json.Marshal(out)
	if err != nil {
		return out, err
	}
	digest := sha256.Sum256(encoded)
	out.ID = fmt.Sprintf("native-commitments/v2/%s/%x", chainKey, digest)
	return out, nil
}

type nativeTimeFork struct {
	Name string
	Time uint64
}

var corethNativeTimeForks = []nativeTimeFork{
	{"apricot-phase1", 1617199200}, {"apricot-phase2", 1620644400}, {"apricot-phase3", 1629813600}, {"apricot-phase4", 1632344400}, {"apricot-phase5", 1638468000},
	{"apricot-pre6", 1662341400}, {"apricot-phase6", 1662494400}, {"apricot-post6", 1662519600}, {"banff", 1666108800}, {"cortina", 1682434800}, {"durango", 1709740800}, {"etna", 1734368400}, {"fortuna", 1744124400}, {"granite", 1763568000}, {"helicon", 1790089200},
}

func attachNativeSemantics(out *VerifiedReceiptCoverage, header ChainHeaderCommitment) error {
	sem, err := ResolveNativeSemantics(out.ChainKey, header)
	if err != nil {
		return err
	}
	if !sem.Admitted {
		return errors.New("NATIVE_SOURCE_ACTIVATION_NOT_ADMITTED")
	}
	out.NativeSemanticsID = sem.ID
	out.ActivationID = sem.ActivationID
	out.NativeSemantics = &sem
	return nil
}
