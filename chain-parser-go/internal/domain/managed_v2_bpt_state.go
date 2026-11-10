package domain

import (
	"errors"
	pb "xone/common/protoc/golang/dexpb"
)

const ManagedV2BptProtocol = "balancer-v2-managed-bpt"
const ManagedV2BptAdapterVersion = "balancer-v2/20230411-managed-bpt-1"

type ManagedV2AllowlistMember struct {
	Address string `json:"address"`
	Allowed bool   `json:"allowed"`
}

func (s *ManagedV2AllowlistMember) UnmarshalJSON(raw []byte) error {
	type plain ManagedV2AllowlistMember
	var v plain
	if e := decodeManagedV2JSON(raw, &v, true); e != nil {
		return e
	}
	*s = ManagedV2AllowlistMember(v)
	return nil
}

type ManagedV2BptState struct {
	JoinExitEnabled    bool                       `json:"join_exit_enabled"`
	MinimumBPTLocked   Amount                     `json:"minimum_bpt_locked"`
	BPTLastChangeBlock uint64                     `json:"bpt_last_change_block"`
	AllowlistMembers   []ManagedV2AllowlistMember `json:"allowlist_members"`
}

func (s *ManagedV2BptState) UnmarshalJSON(raw []byte) error {
	type plain ManagedV2BptState
	var v plain
	if e := decodeManagedV2JSON(raw, &v, true); e != nil {
		return e
	}
	*s = ManagedV2BptState(v)
	return nil
}
func (s ManagedV2BptState) Validate() error {
	if s.MinimumBPTLocked != "1000000" || s.BPTLastChangeBlock > 1<<32-1 || s.AllowlistMembers == nil {
		return errors.New("MANAGED_V2_BPT_CONTROL")
	}
	previous := ""
	for _, m := range s.AllowlistMembers {
		if ValidateEVMAddress(m.Address) != nil || previous >= m.Address {
			return errors.New("MANAGED_V2_BPT_MEMBER_ORDER")
		}
		previous = m.Address
	}
	return nil
}
func (s ManagedV2BptState) MemberAllowed(address string) (bool, error) {
	if e := s.Validate(); e != nil {
		return false, e
	}
	if e := ValidateEVMAddress(address); e != nil {
		return false, e
	}
	for _, m := range s.AllowlistMembers {
		if m.Address == address {
			return m.Allowed, nil
		}
	}
	return false, errors.New("MANAGED_V2_BPT_MEMBER_UNKNOWN")
}
func (s ManagedV2BptState) toProto() (*pb.ManagedV2BptState, error) {
	if e := s.Validate(); e != nil {
		return nil, e
	}
	raw, e := s.MinimumBPTLocked.Bytes()
	if e != nil {
		return nil, e
	}
	out := &pb.ManagedV2BptState{JoinExitEnabled: &s.JoinExitEnabled, MinimumBptLocked: &pb.Amount{Uint256Be: raw}, BptLastChangeBlock: &s.BPTLastChangeBlock}
	for _, m := range s.AllowlistMembers {
		out.AllowlistMembers = append(out.AllowlistMembers, &pb.ManagedV2AllowlistMember{Address: m.Address, Allowed: &m.Allowed})
	}
	return out, nil
}
func ManagedV2BptStateFromProto(w *pb.ManagedV2BptState) (*ManagedV2BptState, error) {
	if w == nil {
		return nil, nil
	}
	if w.JoinExitEnabled == nil || w.BptLastChangeBlock == nil || w.MinimumBptLocked == nil {
		return nil, errors.New("MANAGED_V2_BPT_FIELD_REQUIRED")
	}
	a, e := AmountFromBytes(w.MinimumBptLocked.Uint256Be)
	if e != nil {
		return nil, e
	}
	s := &ManagedV2BptState{JoinExitEnabled: *w.JoinExitEnabled, MinimumBPTLocked: a, BPTLastChangeBlock: *w.BptLastChangeBlock, AllowlistMembers: []ManagedV2AllowlistMember{}}
	for _, m := range w.AllowlistMembers {
		if m == nil || m.Allowed == nil {
			return nil, errors.New("MANAGED_V2_BPT_FIELD_REQUIRED")
		}
		s.AllowlistMembers = append(s.AllowlistMembers, ManagedV2AllowlistMember{Address: m.Address, Allowed: *m.Allowed})
	}
	if e := s.Validate(); e != nil {
		return nil, e
	}
	return s, nil
}
