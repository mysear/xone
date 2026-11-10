package config

// NativeProofConfig bounds the complete native header/body/receipt proof of one
// source observation, including successor commitments on execution-window chains.
// Limits are explicit so enabling a codec cannot widen the source read budget.
type NativeProofConfig struct {
	MaxBytes        int    `json:"max_bytes"`
	MaxHeaders      int    `json:"max_headers"`
	MaxWindowBlocks int    `json:"max_window_blocks"`
	MaxTransactions int    `json:"max_transactions"`
	MaxLookahead    uint64 `json:"max_lookahead"`
}

func (p *NativeProofConfig) valid() bool {
	return p != nil && p.MaxBytes > 0 && p.MaxBytes <= 64<<20 &&
		p.MaxHeaders >= 1 && p.MaxHeaders <= 512 &&
		p.MaxWindowBlocks >= 1 && p.MaxWindowBlocks <= 128 &&
		p.MaxTransactions >= 1 && p.MaxTransactions <= 100000 &&
		p.MaxLookahead >= 1 && p.MaxLookahead <= 512
}
