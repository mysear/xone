package domain

import (
	"bytes"
	"encoding/hex"
	"encoding/json"
	"errors"
	"strings"
	pb "xone/common/protoc/golang/dexpb"
)

const V2CompleteProtocol = "uniswap-v2-complete"
const V2CompleteAdapterVersion = "uniswap-v2/core-1.0.1-full-swap-1"
const V2CompletePairCodeSHA256 = "8b5db55fa9ab3b9527508d4abe0b39eb588bf310270c8e04b3f38214e8ba63b4"
const V2CompleteFactoryCodeSHA256 = "3abc53f12a9cb8ae37ebfada9efc261c1ab4c2759d161e341a49bf67df3f8321"

// V2SwapState contains the complete Pair.swap write set and its raw-token inputs.
// The bank remains the immutable source anchor, including on derived plan states.
type V2SwapState struct {
	RawBalance0          Amount `json:"raw_balance0"`
	RawBalance1          Amount `json:"raw_balance1"`
	BlockTimestampLast   uint32 `json:"block_timestamp_last"`
	Price0CumulativeLast Amount `json:"price0_cumulative_last"`
	Price1CumulativeLast Amount `json:"price1_cumulative_last"`
	Unlocked             bool   `json:"unlocked"`
	FactoryCodeSHA256    string `json:"factory_code_sha256"`
	BankBlockNumber      uint64 `json:"bank_block_number"`
	BankBlockHash        string `json:"bank_block_hash"`
	BankUnixTimestamp    uint64 `json:"bank_unix_timestamp"`
}

// Required zero-valued storage/time fields must not silently become unknown.
func (s *V2SwapState) UnmarshalJSON(raw []byte) error {
	decoder := json.NewDecoder(bytes.NewReader(raw))
	start, e := decoder.Token()
	if e != nil || start != json.Delim('{') {
		return errors.New("V2_SWAP_JSON_OBJECT")
	}
	required := []string{"raw_balance0", "raw_balance1", "block_timestamp_last", "price0_cumulative_last", "price1_cumulative_last", "unlocked", "factory_code_sha256", "bank_block_number", "bank_block_hash", "bank_unix_timestamp"}
	allowed := make(map[string]bool, len(required))
	for _, name := range required {
		allowed[name] = true
	}
	fields := map[string]json.RawMessage{}
	for decoder.More() {
		key, e := decoder.Token()
		if e != nil {
			return e
		}
		name, ok := key.(string)
		if !ok {
			return errors.New("V2_SWAP_JSON_FIELD")
		}
		if !allowed[name] {
			return errors.New("V2_SWAP_UNKNOWN_FIELD")
		}
		if _, duplicate := fields[name]; duplicate {
			return errors.New("V2_SWAP_DUPLICATE_FIELD")
		}
		var value json.RawMessage
		if e = decoder.Decode(&value); e != nil {
			return e
		}
		fields[name] = value
	}
	if _, e = decoder.Token(); e != nil {
		return e
	}
	for _, name := range required {
		value, ok := fields[name]
		if !ok || bytes.Equal(bytes.TrimSpace(value), []byte("null")) {
			return errors.New("V2_SWAP_FIELD_REQUIRED")
		}
	}
	type plain V2SwapState
	var value plain
	decoder = json.NewDecoder(bytes.NewReader(raw))
	decoder.DisallowUnknownFields()
	if e = decoder.Decode(&value); e != nil {
		return e
	}
	*s = V2SwapState(value)
	return nil
}
func (s V2SwapState) Validate() error {
	for _, a := range []Amount{s.RawBalance0, s.RawBalance1, s.Price0CumulativeLast, s.Price1CumulativeLast} {
		if _, e := ParseAmount(a.String()); e != nil {
			return errors.New("V2_SWAP_AMOUNT_WIDTH")
		}
	}
	if !s.Unlocked {
		return errors.New("V2_SWAP_LOCKED")
	}
	if s.FactoryCodeSHA256 != V2CompleteFactoryCodeSHA256 {
		return errors.New("V2_SWAP_FACTORY_CODE")
	}
	if len(s.BankBlockHash) != 66 || !strings.HasPrefix(s.BankBlockHash, "0x") || s.BankBlockHash != strings.ToLower(s.BankBlockHash) || strings.Trim(s.BankBlockHash[2:], "0") == "" {
		return errors.New("V2_SWAP_BANK_HASH")
	}
	if b, e := hex.DecodeString(s.BankBlockHash[2:]); e != nil || len(b) != 32 {
		return errors.New("V2_SWAP_BANK_HASH")
	}
	if s.BankUnixTimestamp > ^uint64(0)/1000 {
		return errors.New("V2_SWAP_BANK_TIME")
	}
	return nil
}
func validateCompleteV2Pool(p Pool) error {
	if p.Protocol != V2CompleteProtocol {
		if p.V2 != nil && p.V2.Swap != nil {
			return errors.New("V2_SWAP_PROFILE_MISMATCH")
		}
		return nil
	}
	if p.V2 == nil || p.V2.Swap == nil {
		return errors.New("V2_SWAP_STATE_REQUIRED")
	}
	if p.V2.CodeSHA256 != V2CompletePairCodeSHA256 {
		return errors.New("V2_SWAP_PAIR_CODE")
	}
	if p.TokenBehavior != "standard" {
		return errors.New("V2_SWAP_TOKEN_BEHAVIOR")
	}
	for _, a := range []string{p.PoolID, p.Token0, p.Token1, p.V2.Factory} {
		if ValidateEVMAddress(a) != nil || strings.Trim(strings.TrimPrefix(a, "0x"), "0") == "" {
			return errors.New("V2_SWAP_ADDRESS")
		}
	}
	if p.Token0 >= p.Token1 {
		return errors.New("V2_SWAP_TOKEN_ORDER")
	}
	for _, r := range []Amount{p.Reserve0, p.Reserve1} {
		v, e := ParseAmount(r.String())
		if e != nil || v.Big().BitLen() > 112 {
			return errors.New("V2_SWAP_RESERVE_WIDTH")
		}
	}
	return p.V2.Swap.Validate()
}
func V2SwapToProto(s *V2SwapState) (*pb.V2SwapState, error) {
	if s == nil {
		return nil, nil
	}
	if e := s.Validate(); e != nil {
		return nil, e
	}
	amounts := make([]*pb.Amount, 4)
	for i, a := range []Amount{s.RawBalance0, s.RawBalance1, s.Price0CumulativeLast, s.Price1CumulativeLast} {
		b, e := a.Bytes()
		if e != nil {
			return nil, e
		}
		amounts[i] = &pb.Amount{Uint256Be: b}
	}
	code, _ := hex.DecodeString(s.FactoryCodeSHA256)
	bank, _ := hex.DecodeString(s.BankBlockHash[2:])
	last, number, timestamp := s.BlockTimestampLast, s.BankBlockNumber, s.BankUnixTimestamp
	return &pb.V2SwapState{RawBalance0: amounts[0], RawBalance1: amounts[1], BlockTimestampLast: &last, Price0CumulativeLast: amounts[2], Price1CumulativeLast: amounts[3], Unlocked: s.Unlocked, FactoryCodeSha256: code, BankBlockNumber: &number, BankBlockHash: bank, BankUnixTimestamp: &timestamp}, nil
}
func V2SwapFromProto(s *pb.V2SwapState) (*V2SwapState, error) {
	if s == nil {
		return nil, nil
	}
	if s.BlockTimestampLast == nil || s.BankBlockNumber == nil || s.BankUnixTimestamp == nil {
		return nil, errors.New("V2_SWAP_FIELD_REQUIRED")
	}
	if len(s.FactoryCodeSha256) != 32 || len(s.BankBlockHash) != 32 {
		return nil, errors.New("V2_SWAP_WIRE_WIDTH")
	}
	amounts := make([]Amount, 4)
	for i, a := range []*pb.Amount{s.RawBalance0, s.RawBalance1, s.Price0CumulativeLast, s.Price1CumulativeLast} {
		if a == nil {
			return nil, errors.New("V2_SWAP_AMOUNT_REQUIRED")
		}
		v, e := AmountFromBytes(a.Uint256Be)
		if e != nil {
			return nil, e
		}
		amounts[i] = v
	}
	out := &V2SwapState{RawBalance0: amounts[0], RawBalance1: amounts[1], BlockTimestampLast: *s.BlockTimestampLast, Price0CumulativeLast: amounts[2], Price1CumulativeLast: amounts[3], Unlocked: s.Unlocked, FactoryCodeSHA256: hex.EncodeToString(s.FactoryCodeSha256), BankBlockNumber: *s.BankBlockNumber, BankBlockHash: "0x" + hex.EncodeToString(s.BankBlockHash), BankUnixTimestamp: *s.BankUnixTimestamp}
	return out, out.Validate()
}
