package config

import (
	"encoding/json"
	"strings"
	"testing"
)

func TestRawEvidenceBudgetIsIndependentExplicitAndBounded(t *testing.T) {
	cfg, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	c := cfg.Config
	valid := RawEvidenceConfig{MaxBytes: 256 * 1024 * 1024, MaxRecords: 10000, MaxRecordBytes: int64(c.Journal.MaxBytes)*4 + 16384}
	c.Journal.RawEvidence = &valid
	c.Journal.Kafka.MaxTailRecords = 3
	if e = c.Validate("chain-parser-go"); e != nil {
		t.Fatal(e)
	}
	for name, change := range map[string]func(*RawEvidenceConfig){"bytes": func(v *RawEvidenceConfig) { v.MaxBytes = 0 }, "records": func(v *RawEvidenceConfig) { v.MaxRecords = 0 }, "record-response": func(v *RawEvidenceConfig) { v.MaxRecordBytes = 1 }, "record-total": func(v *RawEvidenceConfig) { v.MaxBytes = v.MaxRecordBytes - 1 }, "count-unbounded": func(v *RawEvidenceConfig) { v.MaxRecords = 1000001 }, "total-unbounded": func(v *RawEvidenceConfig) { v.MaxBytes = 1 << 41 }} {
		t.Run(name, func(t *testing.T) {
			bad := valid
			change(&bad)
			c.Journal.RawEvidence = &bad
			if c.Validate("chain-parser-go") == nil {
				t.Fatal("invalid raw budget admitted")
			}
		})
	}
	c.Journal.RawEvidence = nil
	raw, e := json.Marshal(c.Journal)
	if e != nil || strings.Contains(string(raw), "\"raw_evidence\"") {
		t.Fatal("nil policy changed legacy schema", e)
	}
}
