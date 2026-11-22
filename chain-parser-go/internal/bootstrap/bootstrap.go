package bootstrap

import (
	"errors"

	"xone/chain-parser-go/internal/domain"
)

var ErrBaselineTooOld = errors.New("BASELINE_TOO_OLD")

func CheckFresh(s domain.Snapshot, nowMS, maxAgeMS uint64) error {
	return CheckTimestampMS(s.Reference.Block.TimestampMS, nowMS, maxAgeMS)
}

// CheckTimestampMS applies the same age and future-skew limits to a header
// before expensive state reads and to the snapshot before publication.
func CheckTimestampMS(ts, nowMS, maxAgeMS uint64) error {
	if ts > nowMS && ts-nowMS > 30000 {
		return errors.New("BASELINE_FUTURE_TIMESTAMP")
	}
	if nowMS > ts && nowMS-ts > maxAgeMS {
		return ErrBaselineTooOld
	}
	return nil
}
