package provenance

import (
	"compress/gzip"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

func readForkCapture(t *testing.T, file string, target any) {
	t.Helper()
	f, err := os.Open(filepath.Join("../../../integration/test/source/beacon-fork-handover", file))
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	gz, err := gzip.NewReader(f)
	if err != nil {
		t.Fatal(err)
	}
	defer gz.Close()
	if err = json.NewDecoder(gz).Decode(target); err != nil {
		t.Fatal(err)
	}
}

func TestHistoricalDenebBootstrapCommitteeMismatch(t *testing.T) {
	var raw json.RawMessage
	readForkCapture(t, "deneb-bootstrap.json.gz", &raw)
	_, _, err := verifyCheckpoint("0xcda3a7b63f1a5c787e9ad8ae955e8fceff45b54e58d890cd6eb0d80486782598", raw)
	if err == nil || err.Error() != "PROVENANCE_BEACON_COMMITTEE_BRANCH" {
		t.Fatal(err)
	}
}

func TestHistoricalMainnetElectraFuluHandover(t *testing.T) {
	const root = "0xd203fe26634fb6d972b70fdc29d9684963183549cab12cae1ce60aea9aa2533c"
	var input OnlineBeaconProof
	readForkCapture(t, "electra-fulu.json.gz", &input)
	proof, err := VerifyExecutionFinality(root, input.Bootstrap, input.Finality, input.Updates)
	if err != nil {
		t.Fatal(err)
	}
	n, h := proof.ExecutionBlock()
	if n != 23942645 || h != "0x4417a28d346e978a8e77128d3596a355c20d87ca18ebd126276933895cace473" || len(proof.transitions) != 1 || proof.transitions[0].Period != 1606 {
		t.Fatal(n, h, proof.transitions)
	}
	var update map[string]json.RawMessage
	if err = json.Unmarshal(input.Finality, &update); err != nil {
		t.Fatal(err)
	}
	update["version"], _ = json.Marshal(proof.checkpoint.Fork)
	wrong, err := json.Marshal(update)
	if err != nil {
		t.Fatal(err)
	}
	if _, err = VerifyExecutionFinality(root, input.Bootstrap, wrong, input.Updates); err == nil || err.Error() != "PROVENANCE_BEACON_UPDATE_FORK" {
		t.Fatal("wrong attested fork", err)
	}
	if _, err = VerifyExecutionFinality(root, input.Bootstrap, input.Finality, nil); err == nil || err.Error() != "PROVENANCE_BEACON_UPDATE_SLOTS" {
		t.Fatal("missing handover", err)
	}
	// The signed Fulu update also includes a next-committee branch. Exercise the
	// transition verifier across the fork independently of any period-1608 head.
	pin, _, err := verifyCheckpoint(root, input.Bootstrap)
	if err != nil {
		t.Fatal(err)
	}
	steps, err := parseCommitteeUpdates(input.Updates)
	if err != nil {
		t.Fatal(err)
	}
	var next committeeUpdate
	if err = json.Unmarshal(input.Finality, &next); err != nil {
		t.Fatal(err)
	}
	if _, err = verifyTransition(pin, 1607, steps[0].Data.NextCommittee, next); err != nil {
		t.Fatal(err)
	}
	next.Data.NextBranch[0][0] ^= 1
	if _, err = verifyTransition(pin, 1607, steps[0].Data.NextCommittee, next); err == nil || err.Error() != "PROVENANCE_BEACON_TRANSITION_PROOF" {
		t.Fatal(err)
	}
}
