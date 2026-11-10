package domain

import (
	"encoding/hex"
	"errors"
	"google.golang.org/protobuf/proto"
	pb "xone/common/protoc/golang/dexpb"
)

const PancakeBscLmProtocol = "pancake-v3-bsc-lm-v4"
const PancakeBscLmAdapter = "pancake-v3/bsc-p400-lm1000000-mc999-v4"

// Fixed exact source/compiler identities for the separate BSC generation.
const BscPoolTemplate = "128755d0e9816da897b0634fec16d89fb47a0c7c41bc2660a68b0c7352f84d3e"
const BscLmTemplate = "df34ef41e16729a12160b2b0e3410729141a521ede0301f52e141b8595014389"
const BscChefTemplate = "64d48f6819e38b7561e0e92a414795f28cbb75e09475d982246659c939365309"

const BscFactoryTemplate = "829e62aa0f1d3471118cabd7931dc0ba702f63ff04496c7d1a1832453970fd52"

var BscPredecessorTemplates = [3]string{"baac406328c78d73ac2077353a7e657864be21a0a26bad057fe84f18d50cd5b7", "1f251fdf65b944bcde07aec7c5bcfb8cbd2a5b55c4808d6ab99184a8ae3dbad2", "baf90c4558c993d9fefd1d102d42a92ee1bc8d0209badca100835cbe38ce2749"}

type PancakeBscLmFlag struct {
	Tick     int32 `json:"tick"`
	Migrated bool  `json:"migrated"`
}

func (s *PancakeBscLmFlag) UnmarshalJSON(raw []byte) error {
	type alias PancakeBscLmFlag
	var out alias
	if e := pancakeStrictJSON(raw, &out, []string{"tick", "migrated"}); e != nil {
		return e
	}
	*s = PancakeBscLmFlag(out)
	return nil
}

type PancakeBscLmPredecessor struct {
	Address        string             `json:"address"`
	CodeSHA256     string             `json:"code_sha256"`
	TemplateSHA256 string             `json:"template_sha256"`
	Ticks          []PancakeLmTick    `json:"ticks"`
	Flags          []PancakeBscLmFlag `json:"flags"`
}

func (s *PancakeBscLmPredecessor) UnmarshalJSON(raw []byte) error {
	type alias PancakeBscLmPredecessor
	var out alias
	if e := pancakeStrictJSON(raw, &out, []string{"address", "code_sha256", "template_sha256", "ticks", "flags"}); e != nil {
		return e
	}
	*s = PancakeBscLmPredecessor(out)
	return nil
}

type PancakeBscLmState struct {
	Ledger         PancakeLmState            `json:"ledger"`
	Initialization bool                      `json:"initialization"`
	Flags          []PancakeBscLmFlag        `json:"flags"`
	Predecessors   []PancakeBscLmPredecessor `json:"predecessors"`
}

func (s *PancakeBscLmState) UnmarshalJSON(raw []byte) error {
	type alias PancakeBscLmState
	var out alias
	if e := pancakeStrictJSON(raw, &out, []string{"ledger", "initialization", "flags", "predecessors"}); e != nil {
		return e
	}
	*s = PancakeBscLmState(out)
	return nil
}
func (s *PancakeBscLmState) EffectiveTicks() ([]PancakeLmTick, error) {
	if s == nil || len(s.Predecessors) != 3 || len(s.Flags) != len(s.Ledger.Lm.Ticks) {
		return nil, errors.New("BSC4_MIGRATION_SCOPE")
	}
	out := make([]PancakeLmTick, len(s.Flags))
	for i, flag := range s.Flags {
		local := s.Ledger.Lm.Ticks[i]
		if local.Tick != flag.Tick {
			return nil, errors.New("BSC4_MIGRATION_TICK_BINDING")
		}
		for generation, p := range s.Predecessors {
			if len(p.Ticks) != len(s.Flags) || (generation != 0 && len(p.Flags) != len(s.Flags)) || (generation == 0 && len(p.Flags) != 0) || p.Ticks[i].Tick != flag.Tick || (generation != 0 && p.Flags[i].Tick != flag.Tick) {
				return nil, errors.New("BSC4_PREDECESSOR_TICK_SCOPE")
			}
		}
		out[i] = local
		if !flag.Migrated {
			at := 0
			if s.Predecessors[2].Flags[i].Migrated {
				at = 2
			} else if s.Predecessors[1].Flags[i].Migrated {
				at = 1
			}
			out[i] = s.Predecessors[at].Ticks[i]
		}
	}
	return out, nil
}

// ArithmeticLedger is only an internal semantic projection, never canonical
// source. The field34 wrapper retains and validates actual code identities.
func (s *PancakeBscLmState) ArithmeticLedger() (*PancakeLmState, error) {
	ticks, e := s.EffectiveTicks()
	if e != nil {
		return nil, e
	}
	out := s.Ledger
	out.PoolTemplateSHA256 = PancakePoolTemplate
	out.FactoryTemplateSHA256 = PancakeFactoryTemplate
	out.Lm.TemplateSHA256 = PancakeLmTemplate
	out.MasterChef.TemplateSHA256 = PancakeChefTemplate
	out.Lm.Ticks = ticks
	return &out, nil
}
func (s *PancakeBscLmState) ValidateWire() error {
	if s == nil || s.Ledger.Core == nil || len(s.Predecessors) != 3 {
		return errors.New("BSC4_LEDGER_REQUIRED")
	}
	v := &s.Ledger
	if v.PoolTemplateSHA256 != BscPoolTemplate || v.Lm.TemplateSHA256 != BscLmTemplate || v.MasterChef.TemplateSHA256 != BscChefTemplate || v.FactoryTemplateSHA256 != BscFactoryTemplate {
		return errors.New("BSC4_CODE_TEMPLATE")
	}
	distinct := map[string]bool{v.Core.Factory: true, v.Lm.Address: true, v.Lm.MasterChef: true, v.Lm.Pool: true}
	for generation, p := range s.Predecessors {
		if pancakeAddress(p.Address) != nil || distinct[p.Address] || !pancakeDigest(p.CodeSHA256) || p.TemplateSHA256 != BscPredecessorTemplates[generation] {
			return errors.New("BSC4_PREDECESSOR_IDENTITY")
		}
		distinct[p.Address] = true
		for i := range p.Ticks {
			if e := p.Ticks[i].ValidateWire(); e != nil {
				return e
			}
		}
	}
	normalized, e := s.ArithmeticLedger()
	if e != nil {
		return e
	}
	if e = normalized.ValidateWire(); e != nil {
		return e
	}
	for i := range v.Lm.Ticks {
		if e := v.Lm.Ticks[i].ValidateWire(); e != nil {
			return e
		}
	}
	return nil
}
func (s *PancakeBscLmState) Validate(p Pool) error {
	if e := s.ValidateWire(); e != nil {
		return e
	}
	if p.Protocol != PancakeBscLmProtocol || p.AdapterVersion != PancakeBscLmAdapter {
		return errors.New("BSC4_PARENT_BINDING")
	}
	for _, previous := range s.Predecessors {
		if previous.Address == p.Token0 || previous.Address == p.Token1 {
			return errors.New("BSC4_PREDECESSOR_TOKEN_ALIAS")
		}
	}
	normalized, e := s.ArithmeticLedger()
	if e != nil {
		return e
	}
	p.PancakeBscLm = nil
	p.PancakeLm = normalized
	p.Protocol = PancakeLmProtocol
	p.AdapterVersion = PancakeLmAdapter
	return p.Validate()
}
func bscFlagsToProto(flags []PancakeBscLmFlag) []*pb.PancakeBscLmFlag {
	out := make([]*pb.PancakeBscLmFlag, 0, len(flags))
	for _, f := range flags {
		out = append(out, &pb.PancakeBscLmFlag{Tick: pancakePtr(f.Tick), Migrated: pancakePtr(f.Migrated)})
	}
	return out
}
func bscFlagsFromProto(flags []*pb.PancakeBscLmFlag) ([]PancakeBscLmFlag, error) {
	out := make([]PancakeBscLmFlag, 0, len(flags))
	for _, f := range flags {
		if f == nil || f.Tick == nil || f.Migrated == nil {
			return nil, errors.New("BSC4_FLAG_REQUIRED")
		}
		out = append(out, PancakeBscLmFlag{*f.Tick, *f.Migrated})
	}
	return out, nil
}
func PancakeBscLmStateToProto(s *PancakeBscLmState) (*pb.PancakeBscLmState, error) {
	if s == nil {
		return nil, nil
	}
	if e := s.ValidateWire(); e != nil {
		return nil, e
	}
	base, e := s.ArithmeticLedger()
	if e != nil {
		return nil, e
	}
	ledger, e := PancakeLmStateToProto(base)
	if e != nil {
		return nil, e
	}
	ledger.PoolTemplateSha256, _ = hex.DecodeString(s.Ledger.PoolTemplateSHA256)
	ledger.FactoryTemplateSha256, _ = hex.DecodeString(s.Ledger.FactoryTemplateSHA256)
	ledger.MasterChef.TemplateSha256, _ = hex.DecodeString(s.Ledger.MasterChef.TemplateSHA256)
	ledger.Lm.TemplateSha256, _ = hex.DecodeString(s.Ledger.Lm.TemplateSHA256)
	ledger.Lm.Ticks = nil
	for i := range s.Ledger.Lm.Ticks {
		t, e := PancakeLmTickToProto(&s.Ledger.Lm.Ticks[i])
		if e != nil {
			return nil, e
		}
		ledger.Lm.Ticks = append(ledger.Lm.Ticks, t)
	}
	out := &pb.PancakeBscLmState{Ledger: ledger, Initialization: pancakePtr(s.Initialization), Flags: bscFlagsToProto(s.Flags)}
	for _, p := range s.Predecessors {
		code, _ := hex.DecodeString(p.CodeSHA256)
		template, _ := hex.DecodeString(p.TemplateSHA256)
		row := &pb.PancakeBscLmPredecessor{Address: p.Address, CodeSha256: code, TemplateSha256: template, Flags: bscFlagsToProto(p.Flags)}
		for i := range p.Ticks {
			tick, e := PancakeLmTickToProto(&p.Ticks[i])
			if e != nil {
				return nil, e
			}
			row.Ticks = append(row.Ticks, tick)
		}
		out.Predecessors = append(out.Predecessors, row)
	}
	return out, nil
}
func PancakeBscLmStateFromProto(w *pb.PancakeBscLmState) (*PancakeBscLmState, error) {
	if w == nil {
		return nil, nil
	}
	if w.Ledger == nil || w.Ledger.Lm == nil || w.Ledger.MasterChef == nil || w.Initialization == nil {
		return nil, errors.New("BSC4_LEDGER_REQUIRED")
	}
	flags, e := bscFlagsFromProto(w.Flags)
	if e != nil {
		return nil, e
	}
	out := &PancakeBscLmState{Initialization: *w.Initialization, Flags: flags}
	for _, p := range w.Predecessors {
		if p == nil || len(p.CodeSha256) != 32 || len(p.TemplateSha256) != 32 {
			return nil, errors.New("BSC4_PREDECESSOR_REQUIRED")
		}
		flags, e := bscFlagsFromProto(p.Flags)
		if e != nil {
			return nil, e
		}
		row := PancakeBscLmPredecessor{Address: p.Address, CodeSHA256: hex.EncodeToString(p.CodeSha256), TemplateSHA256: hex.EncodeToString(p.TemplateSha256), Flags: flags}
		for _, t := range p.Ticks {
			tick, e := PancakeLmTickFromProto(t)
			if e != nil {
				return nil, e
			}
			if tick == nil {
				return nil, errors.New("BSC4_TICK_REQUIRED")
			}
			row.Ticks = append(row.Ticks, *tick)
		}
		out.Predecessors = append(out.Predecessors, row)
	}
	for _, t := range w.Ledger.Lm.Ticks {
		tick, e := PancakeLmTickFromProto(t)
		if e != nil {
			return nil, e
		}
		if tick == nil {
			return nil, errors.New("BSC4_TICK_REQUIRED")
		}
		out.Ledger.Lm.Ticks = append(out.Ledger.Lm.Ticks, *tick)
	}
	effective, e := out.EffectiveTicks()
	if e != nil {
		return nil, e
	}
	rawTicks := out.Ledger.Lm.Ticks
	// Do not mutate the incoming protobuf or let legacy validation reject a
	// physically empty, still-unmigrated local slot before checking its source.
	ledger := proto.Clone(w.Ledger).(*pb.PancakeLmState)
	lm := ledger.Lm
	chef := ledger.MasterChef
	poolTemplate := hex.EncodeToString(ledger.PoolTemplateSha256)
	factoryTemplate := hex.EncodeToString(ledger.FactoryTemplateSha256)
	ledger.FactoryTemplateSha256, _ = hex.DecodeString(PancakeFactoryTemplate)
	lmTemplate := hex.EncodeToString(lm.TemplateSha256)
	chefTemplate := hex.EncodeToString(chef.TemplateSha256)
	ledger.PoolTemplateSha256, _ = hex.DecodeString(PancakePoolTemplate)
	lm.TemplateSha256, _ = hex.DecodeString(PancakeLmTemplate)
	chef.TemplateSha256, _ = hex.DecodeString(PancakeChefTemplate)
	lm.Ticks = nil
	for i := range effective {
		tick, e := PancakeLmTickToProto(&effective[i])
		if e != nil {
			return nil, e
		}
		lm.Ticks = append(lm.Ticks, tick)
	}
	base, e := PancakeLmStateFromProto(ledger)
	if e != nil {
		return nil, e
	}
	out.Ledger = *base
	out.Ledger.PoolTemplateSHA256 = poolTemplate
	out.Ledger.FactoryTemplateSHA256 = factoryTemplate
	out.Ledger.Lm.TemplateSHA256 = lmTemplate
	out.Ledger.MasterChef.TemplateSHA256 = chefTemplate
	out.Ledger.Lm.Ticks = rawTicks
	if e = out.ValidateWire(); e != nil {
		return nil, e
	}
	return out, nil
}

func (s PancakeBscLmState) Clone() PancakeBscLmState {
	out := s
	out.Ledger = s.Ledger.Clone()
	out.Flags = append([]PancakeBscLmFlag{}, s.Flags...)
	out.Predecessors = append([]PancakeBscLmPredecessor{}, s.Predecessors...)
	for i, p := range s.Predecessors {
		out.Predecessors[i].Ticks = append([]PancakeLmTick{}, p.Ticks...)
		out.Predecessors[i].Flags = append([]PancakeBscLmFlag{}, p.Flags...)
	}
	return out
}
