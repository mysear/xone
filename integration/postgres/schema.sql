-- Runtime owner data is a projection of validated configuration, not a second policy source.
CREATE SCHEMA IF NOT EXISTS xone;
CREATE SEQUENCE IF NOT EXISTS xone.writer_epoch_sequence AS bigint MINVALUE 1;
CREATE TABLE IF NOT EXISTS xone.chain_registry (
 chain_key text PRIMARY KEY,
 owner_service text NOT NULL CHECK(owner_service IN ('chain-parser-go','chain-parser-rust')),
 assignment_version bigint NOT NULL CHECK(assignment_version>0),
 config_public_hash text NOT NULL,
 config_release text NOT NULL
);
CREATE TABLE IF NOT EXISTS xone.chain_lease (
 chain_key text PRIMARY KEY REFERENCES xone.chain_registry(chain_key),
 holder text NOT NULL,
 writer_epoch bigint NOT NULL CHECK(writer_epoch>0),
 expires_at timestamptz NOT NULL,
 online_generation bigint NOT NULL DEFAULT 0,
 canonical_epoch bigint NOT NULL DEFAULT 1,
 journal_seq bigint NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS xone.control_audit (
 id bigserial PRIMARY KEY,
 chain_key text NOT NULL,
 event text NOT NULL,
 holder text NOT NULL,
 writer_epoch bigint NOT NULL,
 details jsonb NOT NULL DEFAULT '{}',
 created_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
