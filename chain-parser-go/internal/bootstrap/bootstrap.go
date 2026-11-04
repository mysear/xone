package bootstrap

import (
	"errors"
	"xone/chain-parser-go/internal/domain"
)

func CheckFresh(s domain.Snapshot, nowMS, maxAgeMS uint64) error {
	ts := s.Reference.Block.TimestampMS
	if ts > nowMS+30000 {
		return errors.New("BASELINE_FUTURE_TIMESTAMP")
	}
	if nowMS > ts && nowMS-ts > maxAgeMS {
		return errors.New("BASELINE_TOO_OLD")
	}
	return nil
}
