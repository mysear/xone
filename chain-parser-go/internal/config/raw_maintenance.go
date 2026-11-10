package config

import (
	"errors"
	"strings"
)

// Journal hook (main thread): RawMaintenance *RawMaintenanceConfig
// `json:"raw_maintenance,omitempty"`. From Config.Validate call
// Journal.RawMaintenance.Validate(Journal.RawEvidence). NewRawMaintenance also
// validates the loaded value and requires a nonempty authenticated API token.
type RawMaintenanceConfig struct {
	SinkPath          string `json:"sink_path"`
	SinkMaxBytes      int64  `json:"sink_max_bytes_per_chain"`
	SinkMaxRecords    int    `json:"sink_max_records_per_chain"`
	IntervalMS        uint64 `json:"interval_ms"`
	TimeoutMS         uint64 `json:"timeout_ms"`
	ReplayTimeoutMS   uint64 `json:"replay_timeout_ms"`
	KeepMS            uint64 `json:"keep_ms"`
	TargetBytes       int64  `json:"target_bytes"`
	MaxScan           int    `json:"max_scan"`
	MaxArchive        int    `json:"max_archive"`
	MaxReclaims       int    `json:"max_reclaims"`
	MaxRawBytes       int64  `json:"max_raw_bytes"` // encoded evidence admission/read limit, including metadata and base64 HTTP bodies
	MaxDecodedBytes   int64  `json:"max_decoded_bytes"`
	MaxPageRecords    int    `json:"max_page_records"`
	MaxRequestBytes   int64  `json:"max_request_bytes"`
	MaxPinsPerCapture int    `json:"max_pins_per_capture"` // HTTP captures per group; 1..65536, byte-budget constrained
	DecoderVersion    string `json:"decoder_version"`
}

func (c *RawMaintenanceConfig) Validate(raw *RawEvidenceConfig) error {
	if c == nil {
		return nil
	}
	duration := func(v uint64) bool { return v > 0 && v <= 3600000 }
	if raw == nil || strings.TrimSpace(c.SinkPath) == "" || len(c.SinkPath) > 4096 || c.SinkMaxBytes < 4096 || c.SinkMaxRecords < 2 || c.SinkMaxRecords > 1000000 || !duration(c.IntervalMS) || !duration(c.TimeoutMS) || !duration(c.ReplayTimeoutMS) || c.KeepMS > 315360000000 || c.TargetBytes <= 0 || c.TargetBytes >= raw.MaxBytes || c.MaxScan < 1 || c.MaxScan > 100000 || c.MaxArchive < 1 || c.MaxArchive > c.MaxScan || c.MaxReclaims < 1 || c.MaxReclaims > c.MaxScan || c.MaxRawBytes <= 0 || c.MaxRawBytes > raw.MaxRecordBytes || c.MaxDecodedBytes <= 0 || c.MaxDecodedBytes > raw.MaxRecordBytes || c.MaxPageRecords < 1 || c.MaxPageRecords > 1000 || c.MaxRequestBytes < 256 || c.MaxRequestBytes > 65536 || c.MaxPinsPerCapture < 1 || c.MaxPinsPerCapture > 65536 || c.DecoderVersion != "staged-state/public-adapter-v1" {
		return errors.New("RAW_MAINTENANCE_CONFIG_INVALID")
	}
	return nil
}
