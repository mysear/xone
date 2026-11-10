package evm

import (
	"bytes"
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"errors"
	"xone/chain-parser-go/internal/domain"
)

type slipImmutable struct {
	Offset int
	Role   string
}
type slipTemplate struct {
	RuntimeBytes  int
	BodyBytes     int
	MetadataBytes int
	SHA256        string
	Bindings      []slipImmutable
}

var slipstreamTemplates = map[string]slipTemplate{
	"implementation":  {RuntimeBytes: 24279, BodyBytes: 24226, MetadataBytes: 51, SHA256: "82642d5d25fe9bf09e9a7ef46673ca3922f4c47cbffddfc173ce4866aa65c2e4", Bindings: []slipImmutable{}},
	"factory":         {RuntimeBytes: 4958, BodyBytes: 4905, MetadataBytes: 51, SHA256: "0a403e2c20be57a38c85a3d298207b1b25d3ba849dcb36b43be50c7983bb7187", Bindings: []slipImmutable{{Offset: 1664, Role: "pool_implementation"}, {Offset: 1832, Role: "factory_registry"}, {Offset: 2856, Role: "factory_registry"}, {Offset: 2934, Role: "voter"}, {Offset: 2973, Role: "voter"}, {Offset: 3161, Role: "voter"}, {Offset: 4096, Role: "pool_implementation"}}},
	"custom_swap":     {RuntimeBytes: 1218, BodyBytes: 1165, MetadataBytes: 51, SHA256: "ddc9d6554ff8e4a364fb7965029dc7e7f000676c15650cbd336e6fe47209904c", Bindings: []slipImmutable{{Offset: 355, Role: "factory"}, {Offset: 545, Role: "factory"}, {Offset: 954, Role: "factory"}, {Offset: 1130, Role: "factory"}}},
	"custom_unstaked": {RuntimeBytes: 967, BodyBytes: 914, MetadataBytes: 51, SHA256: "7f7aa9b5f067b30673550db0232bfec1c4c21344f1f1fa48bae97189e38fcae7", Bindings: []slipImmutable{{Offset: 355, Role: "factory"}, {Offset: 546, Role: "factory"}, {Offset: 879, Role: "factory"}}},
	"dynamic_swap":    {RuntimeBytes: 9270, BodyBytes: 9217, MetadataBytes: 51, SHA256: "187b7edb56da74a9674a63daac04c68e4e2f5d5d497e6ee8a8695f4fcec3bcc6", Bindings: []slipImmutable{{Offset: 1515, Role: "factory"}, {Offset: 1714, Role: "factory"}, {Offset: 2114, Role: "factory"}, {Offset: 2521, Role: "factory"}, {Offset: 2723, Role: "factory"}, {Offset: 2868, Role: "factory"}, {Offset: 3067, Role: "factory"}, {Offset: 3399, Role: "factory"}, {Offset: 3754, Role: "factory"}, {Offset: 3953, Role: "factory"}, {Offset: 4346, Role: "factory"}, {Offset: 4545, Role: "factory"}, {Offset: 4906, Role: "factory"}, {Offset: 5220, Role: "factory"}, {Offset: 5494, Role: "factory"}, {Offset: 5811, Role: "factory"}, {Offset: 6093, Role: "factory"}, {Offset: 6433, Role: "factory"}, {Offset: 7180, Role: "factory"}, {Offset: 7223, Role: "factory"}, {Offset: 7422, Role: "factory"}}},
}

func validateSlipstreamRuntime(kind string, raw []byte, bindings map[string]string) (string, error) {
	if kind == "voter" {
		if len(bindings) != 0 || len(raw) != 16912 {
			return "", errors.New("SLIPSTREAM_VOTER_RUNTIME")
		}
		hash := sha256.Sum256(raw)
		if hex.EncodeToString(hash[:]) != domain.SlipstreamVoterSHA256 {
			return "", errors.New("SLIPSTREAM_VOTER_RUNTIME")
		}
		return hex.EncodeToString(hash[:]), nil
	}
	template, ok := slipstreamTemplates[kind]
	if !ok || len(raw) != template.RuntimeBytes || template.BodyBytes+template.MetadataBytes+2 != len(raw) || int(binary.BigEndian.Uint16(raw[len(raw)-2:])) != template.MetadataBytes {
		return "", errors.New("SLIPSTREAM_RUNTIME_TEMPLATE_SIZE")
	}
	required := map[string]bool{}
	for _, ref := range template.Bindings {
		required[ref.Role] = true
	}
	if len(required) != len(bindings) {
		return "", errors.New("SLIPSTREAM_TEMPLATE_BINDINGS")
	}
	body := append([]byte{}, raw[:template.BodyBytes]...)
	for _, ref := range template.Bindings {
		value, ok := bindings[ref.Role]
		if !ok || domain.ValidateEVMAddress(value) != nil || ref.Offset < 0 || ref.Offset+32 > len(body) {
			return "", errors.New("SLIPSTREAM_TEMPLATE_BINDINGS")
		}
		address, e := hex.DecodeString(value[2:])
		if e != nil {
			return "", e
		}
		word := append(make([]byte, 12), address...)
		if !bytes.Equal(body[ref.Offset:ref.Offset+32], word) {
			return "", errors.New("SLIPSTREAM_IMMUTABLE_BINDING")
		}
		clear(body[ref.Offset : ref.Offset+32])
	}
	sum := sha256.Sum256(body)
	if hex.EncodeToString(sum[:]) != template.SHA256 {
		return "", errors.New("SLIPSTREAM_EXECUTION_BODY_TEMPLATE")
	}
	return template.SHA256, nil
}
