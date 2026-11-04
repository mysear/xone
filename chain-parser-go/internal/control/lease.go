package control

import (
	"context"
	"errors"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
	"math"
	"time"
	"xone/chain-parser-go/internal/domain"
)

type Lease struct {
	Chain          string
	Holder         string
	WriterEpoch    uint64
	ExpiresMS      uint64
	Generation     uint64
	CanonicalEpoch uint64
	JournalSeq     uint64
}
type Postgres struct{ pool *pgxpool.Pool }

func Open(ctx context.Context, dsn string) (*Postgres, error) {
	cfg, e := explicitConfig(dsn)
	if e != nil {
		return nil, errors.New("CONTROL_DSN_INVALID")
	}
	if cfg.ConnConfig.Host == "" || cfg.ConnConfig.Database == "" || cfg.ConnConfig.User == "" {
		return nil, errors.New("CONTROL_DSN_INCOMPLETE")
	}
	cfg.MaxConns = 4
	cfg.ConnConfig.ConnectTimeout = 3 * time.Second
	p, e := pgxpool.NewWithConfig(ctx, cfg)
	if e != nil {
		return nil, errors.New("CONTROL_CONNECT")
	}
	if e = p.Ping(ctx); e != nil {
		p.Close()
		return nil, errors.New("CONTROL_CONNECT")
	}
	return &Postgres{p}, nil
}
func (p *Postgres) Close() { p.pool.Close() }
func (p *Postgres) Project(ctx context.Context, chain, owner string, assignment uint64, hash, release string) error {
	if assignment == 0 || assignment > math.MaxInt64 {
		return errors.New("CONTROL_ASSIGNMENT_RANGE")
	}
	tag, e := p.pool.Exec(ctx, `INSERT INTO xone.chain_registry(chain_key,owner_service,assignment_version,config_public_hash,config_release) VALUES($1,$2,$3,$4,$5) ON CONFLICT(chain_key) DO UPDATE SET config_public_hash=EXCLUDED.config_public_hash,config_release=EXCLUDED.config_release WHERE chain_registry.owner_service=EXCLUDED.owner_service AND chain_registry.assignment_version=EXCLUDED.assignment_version`, chain, owner, int64(assignment), hash, release)
	if e != nil {
		return errors.New("CONTROL_PROJECTION_FAILED")
	}
	if tag.RowsAffected() != 1 {
		return errors.New("CONTROL_OWNER_ASSIGNMENT_MISMATCH")
	}
	return nil
}
func scanLease(row pgx.Row, chain, holder string) (Lease, error) {
	var epoch, expires, gen, canonical, seq int64
	if e := row.Scan(&epoch, &expires, &gen, &canonical, &seq); e != nil {
		if errors.Is(e, pgx.ErrNoRows) {
			return Lease{}, errors.New("LEASE_NOT_OWNED")
		}
		return Lease{}, errors.New("CONTROL_LEASE_QUERY")
	}
	if epoch <= 0 || expires <= 0 || gen < 0 || canonical <= 0 || seq < 0 {
		return Lease{}, errors.New("CONTROL_LEASE_CORRUPT")
	}
	return Lease{chain, holder, uint64(epoch), uint64(expires), uint64(gen), uint64(canonical), uint64(seq)}, nil
}
func (p *Postgres) Acquire(ctx context.Context, chain, owner string, assignment uint64, holder string, ttlMS uint64) (Lease, error) {
	if holder == "" || ttlMS < 1000 || ttlMS > 3600000 || assignment > math.MaxInt64 {
		return Lease{}, errors.New("LEASE_ARGUMENT")
	}
	return scanLease(p.pool.QueryRow(ctx, `WITH owned AS (SELECT chain_key FROM xone.chain_registry WHERE chain_key=$1 AND owner_service=$2 AND assignment_version=$3), acquired AS (INSERT INTO xone.chain_lease(chain_key,holder,writer_epoch,expires_at) SELECT chain_key,$4,nextval('xone.writer_epoch_sequence'),clock_timestamp()+$5::bigint*interval '1 millisecond' FROM owned ON CONFLICT(chain_key) DO UPDATE SET holder=EXCLUDED.holder,writer_epoch=GREATEST(chain_lease.writer_epoch+1,EXCLUDED.writer_epoch),expires_at=EXCLUDED.expires_at WHERE chain_lease.expires_at<=clock_timestamp() RETURNING *), audit AS (INSERT INTO xone.control_audit(chain_key,event,holder,writer_epoch) SELECT chain_key,'LEASE_ACQUIRED',holder,writer_epoch FROM acquired) SELECT writer_epoch,(extract(epoch FROM expires_at)*1000)::bigint,online_generation,canonical_epoch,journal_seq FROM acquired`, chain, owner, int64(assignment), holder, int64(ttlMS)), chain, holder)
}
func (p *Postgres) Renew(ctx context.Context, l Lease, ttlMS uint64) (Lease, error) {
	if ttlMS > 3600000 {
		return Lease{}, errors.New("LEASE_ARGUMENT")
	}
	return scanLease(p.pool.QueryRow(ctx, `UPDATE xone.chain_lease SET expires_at=clock_timestamp()+$4::bigint*interval '1 millisecond' WHERE chain_key=$1 AND holder=$2 AND writer_epoch=$3 AND expires_at>clock_timestamp() RETURNING writer_epoch,(extract(epoch FROM expires_at)*1000)::bigint,online_generation,canonical_epoch,journal_seq`, l.Chain, l.Holder, int64(l.WriterEpoch), int64(ttlMS)), l.Chain, l.Holder)
}
func (p *Postgres) Check(ctx context.Context, l Lease, minRemainingMS uint64) error {
	var ok bool
	e := p.pool.QueryRow(ctx, `SELECT EXISTS(SELECT 1 FROM xone.chain_lease WHERE chain_key=$1 AND holder=$2 AND writer_epoch=$3 AND expires_at>clock_timestamp()+$4::bigint*interval '1 millisecond')`, l.Chain, l.Holder, int64(l.WriterEpoch), int64(minRemainingMS)).Scan(&ok)
	if e != nil {
		return errors.New("CONTROL_UNAVAILABLE")
	}
	if !ok {
		return errors.New("LEASE_EXPIRED_OR_FENCED")
	}
	return nil
}
func (p *Postgres) AllocateBaseline(ctx context.Context, l Lease, floor domain.SnapshotRef) (Lease, error) {
	if floor.WriterEpoch >= math.MaxInt64 || floor.OnlineGeneration >= math.MaxInt64 || floor.JournalSeq >= math.MaxInt64 || floor.CanonicalEpoch > math.MaxInt64 {
		return Lease{}, errors.New("CONTROL_VERSION_RANGE")
	}
	return scanLease(p.pool.QueryRow(ctx, `WITH changed AS(UPDATE xone.chain_lease SET writer_epoch=GREATEST(writer_epoch,$4::bigint+1),online_generation=GREATEST(online_generation,$5::bigint)+1,canonical_epoch=GREATEST(canonical_epoch,$6::bigint),journal_seq=GREATEST(journal_seq,$7::bigint) WHERE chain_key=$1 AND holder=$2 AND writer_epoch=$3 AND expires_at>clock_timestamp() RETURNING *),audit AS(INSERT INTO xone.control_audit(chain_key,event,holder,writer_epoch) SELECT chain_key,'BASELINE_ALLOCATED',holder,writer_epoch FROM changed) SELECT writer_epoch,(extract(epoch FROM expires_at)*1000)::bigint,online_generation,canonical_epoch,journal_seq FROM changed`, l.Chain, l.Holder, int64(l.WriterEpoch), int64(floor.WriterEpoch), int64(floor.OnlineGeneration), int64(floor.CanonicalEpoch), int64(floor.JournalSeq)), l.Chain, l.Holder)
}
func (p *Postgres) Committed(ctx context.Context, l Lease, r domain.SnapshotRef) error {
	tag, e := p.pool.Exec(ctx, `UPDATE xone.chain_lease SET online_generation=GREATEST(online_generation,$4),canonical_epoch=GREATEST(canonical_epoch,$5),journal_seq=GREATEST(journal_seq,$6) WHERE chain_key=$1 AND holder=$2 AND writer_epoch=$3 AND expires_at>clock_timestamp()`, l.Chain, l.Holder, int64(l.WriterEpoch), int64(r.OnlineGeneration), int64(r.CanonicalEpoch), int64(r.JournalSeq))
	if e != nil {
		return errors.New("CONTROL_COMMIT_PROJECTION")
	}
	if tag.RowsAffected() != 1 {
		return errors.New("LEASE_EXPIRED_OR_FENCED")
	}
	return nil
}
func (p *Postgres) Release(ctx context.Context, l Lease) error {
	_, e := p.pool.Exec(ctx, `UPDATE xone.chain_lease SET expires_at=clock_timestamp() WHERE chain_key=$1 AND holder=$2 AND writer_epoch=$3`, l.Chain, l.Holder, int64(l.WriterEpoch))
	if e != nil {
		return errors.New("CONTROL_RELEASE")
	}
	return nil
}
