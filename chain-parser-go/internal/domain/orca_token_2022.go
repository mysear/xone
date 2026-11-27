package domain

import (
	"bytes"
	"encoding/binary"
	"encoding/hex"
	"errors"
	"strings"
)

const OrcaToken2022Profile = "orca-whirlpools-adaptive-token2022"
const orcaToken2022Adapter = "orca-whirlpools/cc683ec9-token2022-adaptive-1"
const SPLToken2022Program = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb"
const SPLToken2022Programdata = "DoU57AYuPFu2QU514RktNPG22QhApEjnKxnBcu4BHDTY"

type OrcaToken2022Asset struct {
	MintData  string `json:"mint_data"`
	VaultData string `json:"vault_data"`
}

type OrcaToken2022State struct {
	Programdata       string              `json:"programdata"`
	DeployedSlot      uint64              `json:"deployed_slot"`
	ProgramCodeSHA256 string              `json:"program_code_sha256"`
	Token0            *OrcaToken2022Asset `json:"token0,omitempty"`
	Token1            *OrcaToken2022Asset `json:"token1,omitempty"`
}

func (t OrcaToken2022State) validate(s OrcaWhirlpoolState, p *Pool) error {
	digest, err := hex.DecodeString(t.ProgramCodeSHA256)
	if err != nil || len(digest) != 32 || t.ProgramCodeSHA256 != strings.ToLower(t.ProgramCodeSHA256) ||
		t.Programdata != SPLToken2022Programdata || t.DeployedSlot == 0 || t.DeployedSlot > s.BankSlot ||
		t.Token0 == nil && t.Token1 == nil {
		return errors.New("ORCA_TOKEN2022_DEPLOYMENT")
	}
	for i, asset := range [2]*OrcaToken2022Asset{t.Token0, t.Token1} {
		if asset == nil {
			continue
		}
		mint, mintErr := hex.DecodeString(asset.MintData)
		vault, vaultErr := hex.DecodeString(asset.VaultData)
		if mintErr != nil || vaultErr != nil || asset.MintData != strings.ToLower(asset.MintData) ||
			asset.VaultData != strings.ToLower(asset.VaultData) ||
			len(mint) != 278 || mint[45] != 1 || !bytes.Equal(mint[82:165], make([]byte, 83)) ||
			!bytes.Equal(mint[165:170], []byte{1, 1, 0, 108, 0}) ||
			binary.LittleEndian.Uint32(mint[:4]) > 1 || binary.LittleEndian.Uint32(mint[46:50]) > 1 ||
			binary.LittleEndian.Uint64(mint[242:250]) > binary.LittleEndian.Uint64(mint[260:268]) ||
			binary.LittleEndian.Uint16(mint[258:260]) > 10000 || binary.LittleEndian.Uint16(mint[276:278]) > 10000 ||
			uint32(mint[44]) != [2]uint32{s.Mint0Decimals, s.Mint1Decimals}[i] {
			return errors.New("ORCA_TOKEN2022_MINT_LAYOUT")
		}
		if !validOrcaToken2022Vault(vault) || vault[108] != 1 ||
			binary.LittleEndian.Uint64(vault[64:72]) != [2]uint64{s.RawVault0, s.RawVault1}[i] ||
			binary.LittleEndian.Uint32(vault[72:76]) != 0 || binary.LittleEndian.Uint32(vault[109:113]) != 0 ||
			!bytes.Equal(vault[121:133], make([]byte, 12)) {
			return errors.New("ORCA_TOKEN2022_VAULT_LAYOUT")
		}
		if p != nil && (encode58(vault[:32]) != [2]string{p.Token0, p.Token1}[i] || encode58(vault[32:64]) != p.PoolID) {
			return errors.New("ORCA_TOKEN2022_VAULT_BINDING")
		}
	}
	return nil
}

func validOrcaToken2022Vault(vault []byte) bool {
	if (len(vault) != 178 && len(vault) != 182) || vault[165] != 2 {
		return false
	}
	fee := []byte{2, 0, 8, 0}
	immutable := []byte{7, 0, 0, 0}
	if bytes.Equal(vault[166:170], fee) {
		return len(vault) == 178 || bytes.Equal(vault[178:182], immutable)
	}
	return len(vault) == 182 && bytes.Equal(vault[166:170], immutable) && bytes.Equal(vault[170:174], fee)
}
