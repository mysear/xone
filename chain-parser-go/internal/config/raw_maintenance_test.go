package config

import (
	"testing"
)

func TestRawMaintenanceConfigRequiresExplicitBoundedSinkAndDecoder(t *testing.T) {
	raw := &RawEvidenceConfig{MaxBytes: 64 << 20, MaxRecords: 1000, MaxRecordBytes: 1 << 20}
	c := RawMaintenanceConfig{SinkPath: "archive", SinkMaxBytes: 64 << 20, SinkMaxRecords: 1000, IntervalMS: 100, TimeoutMS: 1000, ReplayTimeoutMS: 2000, KeepMS: 1000, TargetBytes: 32 << 20, MaxScan: 20, MaxArchive: 4, MaxReclaims: 4, MaxRawBytes: 1 << 20, MaxDecodedBytes: 1 << 20, MaxPageRecords: 20, MaxRequestBytes: 8192, MaxPinsPerCapture: 64, DecoderVersion: "staged-state/public-adapter-v1"}
	if err := c.Validate(raw); err != nil {
		t.Fatal(err)
	}
	for _, mode := range []string{"sink", "quota", "timeout", "decoder", "memory", "pins", "raw"} {
		t.Run(mode, func(t *testing.T) {
			bad := c
			r := raw
			switch mode {
			case "sink":
				bad.SinkPath = ""
			case "quota":
				bad.SinkMaxBytes = 0
			case "timeout":
				bad.ReplayTimeoutMS = 0
			case "decoder":
				bad.DecoderVersion = "/tmp/script"
			case "memory":
				bad.MaxDecodedBytes = 2 << 20
			case "pins":
				bad.MaxPinsPerCapture = 0
			case "raw":
				r = nil
			}
			if err := bad.Validate(r); err == nil {
				t.Fatal("unsafe configuration accepted")
			}
		})
	}
}

func TestRawMaintenanceAllowsMainnetSizedBoundedCaptureGroups(t *testing.T) {
	raw := &RawEvidenceConfig{MaxBytes: 1 << 30, MaxRecords: 100000, MaxRecordBytes: 64 << 20}
	c := RawMaintenanceConfig{SinkPath: "archive", SinkMaxBytes: 1 << 30, SinkMaxRecords: 100000, IntervalMS: 1000, TimeoutMS: 10000, ReplayTimeoutMS: 20000, TargetBytes: 128 << 20, MaxScan: 100, MaxArchive: 10, MaxReclaims: 10, MaxRawBytes: 64 << 20, MaxDecodedBytes: 16 << 20, MaxPageRecords: 100, MaxRequestBytes: 8192, MaxPinsPerCapture: 20000, DecoderVersion: "staged-state/public-adapter-v1"}
	if err := c.Validate(raw); err != nil {
		t.Fatal("hidden small RPC cap", err)
	}
	c.MaxPinsPerCapture = 1000000
	if err := c.Validate(raw); err == nil {
		t.Fatal("unbounded group accepted")
	}
}
