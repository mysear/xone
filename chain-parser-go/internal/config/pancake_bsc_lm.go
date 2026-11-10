package config

import "xone/chain-parser-go/internal/domain"

type BscPredecessorConfig struct {
	Address    string `json:"address"`
	CodeSHA256 string `json:"code_sha256"`
}
type PancakeBscLmConfig struct {
	Ledger       PancakeLmConfig        `json:"ledger"`
	Predecessors []BscPredecessorConfig `json:"predecessors"`
}

func (p PoolConfig) validatePancakeBscLm(chain string) bool {
	if p.Protocol != domain.PancakeBscLmProtocol || p.PancakeBscLm == nil || len(p.PancakeBscLm.Predecessors) != 3 || p.PancakeLm != nil {
		return false
	}
	c := p.PancakeBscLm
	projected := p
	projected.PancakeBscLm = nil
	projected.Protocol = domain.PancakeLmProtocol
	projected.PancakeLm = &c.Ledger
	if !projected.validatePancakeLm(chain) {
		return false
	}
	distinct := map[string]bool{p.ID: true, p.Token0: true, p.Token1: true, c.Ledger.ExpectedFactory: true, c.Ledger.ExpectedLmPool: true, c.Ledger.ExpectedMasterChef: true}
	for _, x := range c.Predecessors {
		if domain.ValidateEVMIdentity(x.Address, x.CodeSHA256) != nil || x.Address == domain.SingletonZeroAddress || distinct[x.Address] {
			return false
		}
		distinct[x.Address] = true
	}
	return true
}
