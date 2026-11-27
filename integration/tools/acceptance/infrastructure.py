#!/usr/bin/env python3
"""Dedicated loopback-only PG/Kafka instances for authority acceptance."""
from pathlib import Path
import os
import secrets
import socket
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[3]
BASE = ROOT / "integration/.runtime/infrastructure"
PG = Path(os.environ.get("XONE_AUTHORITY_PG_BIN", "/usr/local/opt/postgresql@13/bin"))
KAFKA = ROOT / "integration/.runtime/deps/kafka_2.13-3.9.1"
KAFKA_IMAGE = os.environ.get("XONE_AUTHORITY_KAFKA_IMAGE", "apache/kafka:3.9.1")
POSTGRES_IMAGE = os.environ.get("XONE_AUTHORITY_POSTGRES_IMAGE", "postgres:16-alpine")


def port():
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def command(args, **kwargs):
    return subprocess.run(
        [str(value) for value in args],
        check=kwargs.pop("check", True),
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        **kwargs,
    ).stdout


class Infrastructure:
    def __init__(self):
        self.kafka = None
        self.kafka_container = None
        self.log = None
        self.pg_started = False
        self.pg_container = None
        self.env = dict(
            os.environ,
            JAVA_HOME="/Library/Java/JavaVirtualMachines/jdk-11.jdk/Contents/Home",
            KAFKA_HEAP_OPTS="-Xms256m -Xmx512m",
        )
        self.path = BASE / f"{os.getpid()}-{secrets.token_hex(3)}"
        self.path.mkdir(parents=True)
        self.pgport, self.kafka_port, self.controller_port = port(), port(), port()
        self.kafka_logs = tempfile.TemporaryDirectory(prefix="kafka-logs-", dir=self.path)
        self.env["LOG_DIR"] = self.kafka_logs.name

    def docker(self, args, **kwargs):
        return command(["docker", *args], **kwargs)

    def wait_port(self, number, timeout=45):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                with socket.create_connection(("127.0.0.1", number), timeout=0.3):
                    return
            except OSError:
                time.sleep(0.1)
        raise RuntimeError(f"isolated service did not open port {number}")

    def start_postgres_docker(self):
        data = self.path / "pgdata"
        data.mkdir()
        self.pg_container = self.docker([
            "run", "--rm", "-d", "--name", f"{self.path.name}-pg",
            "-p", f"127.0.0.1:{self.pgport}:5432",
            "-e", "POSTGRES_USER=xone_test", "-e", "POSTGRES_PASSWORD=xone_test",
            "-e", "POSTGRES_DB=postgres", "-v", f"{data}:/var/lib/postgresql/data",
            POSTGRES_IMAGE,
        ]).strip()
        self.wait_port(self.pgport)
        self.dsn = f"postgresql://xone_test:xone_test@127.0.0.1:{self.pgport}/postgres?sslmode=disable"
        self.dsnfile = self.path / "postgres-dsn"
        self.dsnfile.write_text(self.dsn)
        self.dsnfile.chmod(0o600)
        schema = (ROOT / "integration/postgres/schema.sql").read_text()
        deadline = time.monotonic() + 45
        while True:
            try:
                self.docker([
                    "exec", "-i", self.pg_container, "psql", "-U", "xone_test", "-d", "postgres",
                    "-v", "ON_ERROR_STOP=1",
                ], input=schema)
                break
            except subprocess.CalledProcessError:
                if time.monotonic() >= deadline:
                    raise
                time.sleep(0.5)

    def start_kafka_docker(self):
        self.kafka_container = self.docker([
            "run", "--rm", "-d", "--name", f"{self.path.name}-kafka",
            "-p", f"127.0.0.1:{self.kafka_port}:9094",
            "-e", "KAFKA_NODE_ID=1", "-e", "KAFKA_PROCESS_ROLES=broker,controller",
            "-e", "KAFKA_CONTROLLER_QUORUM_VOTERS=1@localhost:9093",
            "-e", "KAFKA_LISTENER_SECURITY_PROTOCOL_MAP=CONTROLLER:PLAINTEXT,INTERNAL:PLAINTEXT,EXTERNAL:PLAINTEXT",
            "-e", "KAFKA_LISTENERS=CONTROLLER://:9093,INTERNAL://:9092,EXTERNAL://:9094",
            "-e", f"KAFKA_ADVERTISED_LISTENERS=INTERNAL://localhost:9092,EXTERNAL://127.0.0.1:{self.kafka_port}",
            "-e", "KAFKA_CONTROLLER_LISTENER_NAMES=CONTROLLER",
            "-e", "KAFKA_INTER_BROKER_LISTENER_NAME=INTERNAL",
            "-e", "KAFKA_OFFSETS_TOPIC_REPLICATION_FACTOR=1",
            "-e", "KAFKA_NUM_PARTITIONS=1",
            "-e", "KAFKA_TRANSACTION_STATE_LOG_NUM_PARTITIONS=1",
            "-e", "KAFKA_TRANSACTION_STATE_LOG_REPLICATION_FACTOR=1",
            "-e", "KAFKA_TRANSACTION_STATE_LOG_MIN_ISR=1",
            "-e", "KAFKA_GROUP_INITIAL_REBALANCE_DELAY_MS=0",
            KAFKA_IMAGE,
        ]).strip()
        self.wait_port(self.kafka_port)
        self.broker = f"127.0.0.1:{self.kafka_port}"

    def kafka_command(self, args):
        if self.kafka_container:
            args = list(args)
            for index, value in enumerate(args[:-1]):
                if value == "--bootstrap-server" and args[index + 1] == self.broker:
                    args[index + 1] = "localhost:9092"
            deadline = time.monotonic() + 45
            while True:
                try:
                    return self.docker(["exec", self.kafka_container, "/opt/kafka/bin/" + args[0], *args[1:]])
                except subprocess.CalledProcessError:
                    if time.monotonic() >= deadline:
                        raise
                    time.sleep(0.5)
        return command([KAFKA / "bin" / args[0], *args[1:]], env=self.env)

    def start(self):
        if all((PG / name).is_file() for name in ("initdb", "pg_ctl", "psql")):
            command([PG / "initdb", "-D", self.path / "pg", "-U", "xone_test", "-A", "trust", "--no-locale", "-E", "UTF8"])
            command([PG / "pg_ctl", "-D", self.path / "pg", "-l", self.path / "postgres.log", "-o", f"-h 127.0.0.1 -p {self.pgport} -k {self.path}", "-w", "start"])
            self.pg_started = True
            self.dsn = f"postgresql://xone_test@127.0.0.1:{self.pgport}/postgres?sslmode=disable"
            self.dsnfile = self.path / "postgres-dsn"
            self.dsnfile.write_text(self.dsn)
            self.dsnfile.chmod(0o600)
            command([PG / "psql", self.dsn, "-v", "ON_ERROR_STOP=1", "-f", ROOT / "integration/postgres/schema.sql"])
        else:
            self.start_postgres_docker()

        config = self.path / "kafka.properties"
        config.write_text(f"""process.roles=broker,controller
node.id=1
controller.quorum.voters=1@127.0.0.1:{self.controller_port}
listeners=PLAINTEXT://127.0.0.1:{self.kafka_port},CONTROLLER://127.0.0.1:{self.controller_port}
advertised.listeners=PLAINTEXT://127.0.0.1:{self.kafka_port}
controller.listener.names=CONTROLLER
listener.security.protocol.map=CONTROLLER:PLAINTEXT,PLAINTEXT:PLAINTEXT
inter.broker.listener.name=PLAINTEXT
log.dirs={self.path}/kafka-data
num.partitions=1
auto.create.topics.enable=false
offsets.topic.replication.factor=1
transaction.state.log.num.partitions=1
offsets.topic.num.partitions=1
transaction.state.log.replication.factor=1
transaction.state.log.min.isr=1
group.initial.rebalance.delay.ms=0
log.retention.hours=1
""")
        if (KAFKA / "bin/kafka-storage.sh").is_file():
            cluster = command([KAFKA / "bin/kafka-storage.sh", "random-uuid"], env=self.env).strip()
            command([KAFKA / "bin/kafka-storage.sh", "format", "-t", cluster, "-c", config], env=self.env)
            self.log = open(self.path / "kafka.log", "w")
            self.kafka = subprocess.Popen([str(KAFKA / "bin/kafka-server-start.sh"), str(config)], env=self.env, stdout=self.log, stderr=subprocess.STDOUT)
            self.wait_port(self.kafka_port)
            self.broker = f"127.0.0.1:{self.kafka_port}"
        else:
            self.start_kafka_docker()
        for topic in ["canonical.evm-31337.v1", "canonical.solana-localnet.v1", "test.fencing", "test.crash", "test.reorg", "test.router-reorg", "test.analytics", "test.fact-crash", "test.abort", "test.invalid-partitions"]:
            self.kafka_command(["kafka-topics.sh", "--bootstrap-server", self.broker, "--create", "--topic", topic, "--partitions", "2" if topic == "test.invalid-partitions" else "1", "--replication-factor", "1"])
        return self

    def close(self):
        if self.kafka is not None:
            self.kafka.terminate()
            try:
                self.kafka.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.kafka.kill()
                self.kafka.wait()
        if self.log:
            self.log.close()
        self.kafka_logs.cleanup()
        if self.kafka_container:
            (self.path / "kafka-container.log").write_text(self.docker(["logs", self.kafka_container], check=False))
            self.docker(["rm", "-f", self.kafka_container], check=False)
        if self.pg_container:
            (self.path / "postgres-container.log").write_text(self.docker(["logs", self.pg_container], check=False))
            self.docker(["rm", "-f", self.pg_container], check=False)
        if self.pg_started:
            try:
                command([PG / "pg_ctl", "-D", self.path / "pg", "-m", "immediate", "-w", "stop"])
            except subprocess.CalledProcessError:
                pass

    def __enter__(self):
        try:
            return self.start()
        except Exception:
            self.close()
            raise

    def __exit__(self, *_args):
        self.close()
