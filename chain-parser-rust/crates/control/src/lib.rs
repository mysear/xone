use std::sync::Arc;
use tokio_postgres::{Client, NoTls, Row};
use xone_domain::{err, Result, SnapshotRef};
#[derive(Clone, Debug)]
pub struct Lease {
    pub chain: String,
    pub holder: String,
    pub writer_epoch: u64,
    pub expires_ms: u64,
    pub generation: u64,
    pub canonical_epoch: u64,
    pub journal_seq: u64,
}
#[derive(Clone)]
pub struct Postgres {
    client: Arc<Client>,
}
fn i(v: u64) -> Result<i64> {
    i64::try_from(v).map_err(|_| err("CONTROL_VERSION_RANGE"))
}
fn lease(row: Option<Row>, chain: &str, holder: &str) -> Result<Lease> {
    let r = row.ok_or_else(|| err("LEASE_NOT_OWNED"))?;
    let values = (0..5)
        .map(|n| u64::try_from(r.get::<_, i64>(n)).map_err(|_| err("CONTROL_LEASE_CORRUPT")))
        .collect::<Result<Vec<_>>>()?;
    Ok(Lease {
        chain: chain.into(),
        holder: holder.into(),
        writer_epoch: values[0],
        expires_ms: values[1],
        generation: values[2],
        canonical_epoch: values[3],
        journal_seq: values[4],
    })
}
impl Postgres {
    pub async fn open(dsn: &str) -> Result<Self> {
        let config: tokio_postgres::Config = dsn.parse().map_err(|_| err("CONTROL_DSN_INVALID"))?;
        if config.get_ssl_mode() != tokio_postgres::config::SslMode::Disable
            || config.get_hosts().is_empty()
            || config.get_dbname().is_none()
            || config.get_user().is_none()
        {
            return Err(err("CONTROL_EXPLICIT_DEVELOPMENT_DSN_REQUIRED"));
        }
        let (client, connection) =
            tokio::time::timeout(std::time::Duration::from_secs(3), config.connect(NoTls))
                .await
                .map_err(|_| err("CONTROL_CONNECT"))?
                .map_err(|_| err("CONTROL_CONNECT"))?;
        tokio::spawn(async move {
            let _ = connection.await;
        });
        Ok(Self {
            client: Arc::new(client),
        })
    }
    pub async fn project(
        &self,
        chain: &str,
        owner: &str,
        assignment: u64,
        hash: &str,
        release: &str,
    ) -> Result<()> {
        let changed=self.client.execute("INSERT INTO xone.chain_registry(chain_key,owner_service,assignment_version,config_public_hash,config_release) VALUES($1,$2,$3,$4,$5) ON CONFLICT(chain_key) DO UPDATE SET config_public_hash=EXCLUDED.config_public_hash,config_release=EXCLUDED.config_release WHERE chain_registry.owner_service=EXCLUDED.owner_service AND chain_registry.assignment_version=EXCLUDED.assignment_version",&[&chain,&owner,&i(assignment)?,&hash,&release]).await.map_err(|_|err("CONTROL_PROJECTION_FAILED"))?;
        if changed != 1 {
            return Err(err("CONTROL_OWNER_ASSIGNMENT_MISMATCH"));
        }
        Ok(())
    }
    pub async fn acquire(
        &self,
        chain: &str,
        owner: &str,
        assignment: u64,
        holder: &str,
        ttl: u64,
    ) -> Result<Lease> {
        let row=self.client.query_opt("WITH owned AS (SELECT chain_key FROM xone.chain_registry WHERE chain_key=$1 AND owner_service=$2 AND assignment_version=$3), acquired AS (INSERT INTO xone.chain_lease(chain_key,holder,writer_epoch,expires_at) SELECT chain_key,$4,nextval('xone.writer_epoch_sequence'),clock_timestamp()+$5::bigint*interval '1 millisecond' FROM owned ON CONFLICT(chain_key) DO UPDATE SET holder=EXCLUDED.holder,writer_epoch=GREATEST(chain_lease.writer_epoch+1,EXCLUDED.writer_epoch),expires_at=EXCLUDED.expires_at WHERE chain_lease.expires_at<=clock_timestamp() RETURNING *), audit AS (INSERT INTO xone.control_audit(chain_key,event,holder,writer_epoch) SELECT chain_key,'LEASE_ACQUIRED',holder,writer_epoch FROM acquired) SELECT writer_epoch,(extract(epoch FROM expires_at)*1000)::bigint,online_generation,canonical_epoch,journal_seq FROM acquired",&[&chain,&owner,&i(assignment)?,&holder,&i(ttl)?]).await.map_err(|_|err("CONTROL_LEASE_QUERY"))?;
        lease(row, chain, holder)
    }
    pub async fn renew(&self, l: &Lease, ttl: u64) -> Result<Lease> {
        let row=self.client.query_opt("UPDATE xone.chain_lease SET expires_at=clock_timestamp()+$4::bigint*interval '1 millisecond' WHERE chain_key=$1 AND holder=$2 AND writer_epoch=$3 AND expires_at>clock_timestamp() RETURNING writer_epoch,(extract(epoch FROM expires_at)*1000)::bigint,online_generation,canonical_epoch,journal_seq",&[&l.chain,&l.holder,&i(l.writer_epoch)?,&i(ttl)?]).await.map_err(|_|err("CONTROL_LEASE_QUERY"))?;
        lease(row, &l.chain, &l.holder)
    }
    pub async fn check(&self, l: &Lease, remaining: u64) -> Result<()> {
        let row=self.client.query_one("SELECT EXISTS(SELECT 1 FROM xone.chain_lease WHERE chain_key=$1 AND holder=$2 AND writer_epoch=$3 AND expires_at>clock_timestamp()+$4::bigint*interval '1 millisecond')",&[&l.chain,&l.holder,&i(l.writer_epoch)?,&i(remaining)?]).await.map_err(|_|err("CONTROL_UNAVAILABLE"))?;
        if !row.get::<_, bool>(0) {
            return Err(err("LEASE_EXPIRED_OR_FENCED"));
        }
        Ok(())
    }
    pub async fn allocate_baseline(&self, l: &Lease, f: &SnapshotRef) -> Result<Lease> {
        let row=self.client.query_opt("WITH changed AS(UPDATE xone.chain_lease SET writer_epoch=GREATEST(writer_epoch,$4::bigint+1),online_generation=GREATEST(online_generation,$5::bigint)+1,canonical_epoch=GREATEST(canonical_epoch,$6::bigint),journal_seq=GREATEST(journal_seq,$7::bigint) WHERE chain_key=$1 AND holder=$2 AND writer_epoch=$3 AND expires_at>clock_timestamp() RETURNING *),audit AS(INSERT INTO xone.control_audit(chain_key,event,holder,writer_epoch) SELECT chain_key,'BASELINE_ALLOCATED',holder,writer_epoch FROM changed) SELECT writer_epoch,(extract(epoch FROM expires_at)*1000)::bigint,online_generation,canonical_epoch,journal_seq FROM changed",&[&l.chain,&l.holder,&i(l.writer_epoch)?,&i(f.writer_epoch)?,&i(f.online_generation)?,&i(f.canonical_epoch)?,&i(f.journal_seq)?]).await.map_err(|_|err("CONTROL_LEASE_QUERY"))?;
        lease(row, &l.chain, &l.holder)
    }
    pub async fn committed(&self, l: &Lease, r: &SnapshotRef) -> Result<()> {
        let changed=self.client.execute("UPDATE xone.chain_lease SET online_generation=GREATEST(online_generation,$4),canonical_epoch=GREATEST(canonical_epoch,$5),journal_seq=GREATEST(journal_seq,$6) WHERE chain_key=$1 AND holder=$2 AND writer_epoch=$3 AND expires_at>clock_timestamp()",&[&l.chain,&l.holder,&i(l.writer_epoch)?,&i(r.online_generation)?,&i(r.canonical_epoch)?,&i(r.journal_seq)?]).await.map_err(|_|err("CONTROL_COMMIT_PROJECTION"))?;
        if changed != 1 {
            return Err(err("LEASE_EXPIRED_OR_FENCED"));
        }
        Ok(())
    }
    pub async fn release(&self, l: &Lease) -> Result<()> {
        self.client.execute("UPDATE xone.chain_lease SET expires_at=clock_timestamp() WHERE chain_key=$1 AND holder=$2 AND writer_epoch=$3",&[&l.chain,&l.holder,&i(l.writer_epoch)?]).await.map_err(|_|err("CONTROL_RELEASE"))?;
        Ok(())
    }
}
