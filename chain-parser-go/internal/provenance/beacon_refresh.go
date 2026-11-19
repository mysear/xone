package provenance

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"syscall"
	"time"

	beacon "github.com/ethereum/go-ethereum/beacon/types"
)

// RefreshBeaconProof installs a verified REST observation at the file read by
// the owner. The release supplies the checkpoint and both freshness limits.
func RefreshBeaconProof(ctx context.Context, endpoint, root, path string, checkpointAge, headAge uint64) (FinalityBinding, error) {
	base, err := url.Parse(endpoint)
	if err != nil || base.Scheme != "https" || base.Hostname() == "" || base.User != nil || base.RawQuery != "" || base.Fragment != "" {
		return FinalityBinding{}, invalid("BEACON_REFRESH_ENDPOINT")
	}
	client := http.Client{Timeout: 20 * time.Second, CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }}
	fetch := func(route string, limit int) (json.RawMessage, error) {
		req, err := http.NewRequestWithContext(ctx, http.MethodGet, strings.TrimRight(base.String(), "/")+"/eth/v1/beacon/light_client/"+route, nil)
		if err != nil {
			return nil, invalid("BEACON_REFRESH_REQUEST")
		}
		req.Header.Set("Accept", "application/json")
		res, err := client.Do(req)
		if err != nil {
			return nil, invalid("BEACON_REFRESH_NETWORK")
		}
		defer res.Body.Close()
		if res.StatusCode != http.StatusOK {
			return nil, fmt.Errorf("BEACON_REFRESH_HTTP_%d", res.StatusCode)
		}
		data, err := io.ReadAll(io.LimitReader(res.Body, int64(limit)+1))
		if err != nil || len(data) > limit {
			return nil, invalid("BEACON_REFRESH_RESPONSE")
		}
		return data, nil
	}
	return refreshBeaconProof(ctx, root, path, checkpointAge, headAge, fetch, time.Now)
}

func refreshBeaconProof(ctx context.Context, root, path string, checkpointAge, headAge uint64, fetch func(string, int) (json.RawMessage, error), now func() time.Time) (FinalityBinding, error) {
	var observation OnlineBeaconProof
	var err error
	observation.Bootstrap, err = fetch("bootstrap/"+root, MaxBootstrapBytes)
	if err != nil {
		return FinalityBinding{}, err
	}
	pin, _, err := verifyCheckpoint(root, observation.Bootstrap)
	if err != nil {
		return FinalityBinding{}, err
	}
	observation.Finality, err = fetch("finality_update", MaxBootstrapBytes)
	if err != nil {
		return FinalityBinding{}, err
	}
	var hint struct {
		Data struct {
			SignatureSlot uint64 `json:"signature_slot,string"`
		} `json:"data"`
	}
	if json.Unmarshal(observation.Finality, &hint) != nil {
		return FinalityBinding{}, invalid("BEACON_UPDATE_ENCODING")
	}
	start, end := beacon.SyncPeriod(pin.Slot), beacon.SyncPeriod(hint.Data.SignatureSlot)
	if end < start || end-start > maxCommitteeTransitions {
		return FinalityBinding{}, invalid("BEACON_UPDATES_LIMIT")
	}
	if end > start {
		route := fmt.Sprintf("updates?start_period=%d&count=%d", start, end-start)
		observation.Updates, err = fetch(route, MaxBeaconUpdatesBytes)
		if err != nil {
			return FinalityBinding{}, err
		}
	}
	proof, err := VerifyExecutionFinality(root, observation.Bootstrap, observation.Finality, observation.Updates)
	if err != nil {
		return FinalityBinding{}, err
	}
	if err := proof.CheckOnlineAge(now(), checkpointAge, headAge); err != nil {
		return FinalityBinding{}, err
	}
	data, err := json.Marshal(observation)
	if err != nil {
		return FinalityBinding{}, err
	}
	if err := installBeaconProof(ctx, path, root, proof, data, checkpointAge, headAge, now); err != nil {
		return FinalityBinding{}, err
	}
	return proof.finality, nil
}

func installBeaconProof(ctx context.Context, path, root string, proof *VerifiedExecutionFinality, data []byte, checkpointAge, headAge uint64, now func() time.Time) error {
	lock, err := os.OpenFile(path+".lock", os.O_CREATE|os.O_RDWR, 0600)
	if err != nil {
		return err
	}
	defer lock.Close()
	if err := syscall.Flock(int(lock.Fd()), syscall.LOCK_EX|syscall.LOCK_NB); err != nil {
		return invalid("BEACON_REFRESH_BUSY")
	}
	if _, err := os.Lstat(path); err == nil {
		old, err := ReadOnlineBeaconProof(path, root)
		if err != nil {
			return err
		}
		prev, next := old.finality, proof.finality
		if next.FinalizedSlot < prev.FinalizedSlot || next.ExecutionNumber < prev.ExecutionNumber || (next.FinalizedSlot == prev.FinalizedSlot || next.ExecutionNumber == prev.ExecutionNumber) && (next.FinalizedRoot != prev.FinalizedRoot || next.ExecutionHash != prev.ExecutionHash) {
			return invalid("BEACON_REFRESH_CONFLICT")
		}
	} else if !errors.Is(err, os.ErrNotExist) {
		return err
	}
	dir := filepath.Dir(path)
	tmp, err := os.CreateTemp(dir, ".beacon-update-")
	if err != nil {
		return err
	}
	defer os.Remove(tmp.Name())
	if _, err = tmp.Write(data); err == nil {
		err = tmp.Sync()
	}
	closeErr := tmp.Close()
	if err != nil {
		return err
	}
	if closeErr != nil {
		return closeErr
	}
	if err = ctx.Err(); err != nil {
		return err
	}
	if err = proof.CheckOnlineAge(now(), checkpointAge, headAge); err != nil {
		return err
	}
	if err = os.Rename(tmp.Name(), path); err != nil {
		return err
	}
	parent, err := os.Open(dir)
	if err != nil {
		return err
	}
	defer parent.Close()
	return parent.Sync()
}
