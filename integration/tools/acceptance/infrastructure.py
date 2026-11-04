#!/usr/bin/env python3
"""Dedicated loopback-only PG/Kafka instances. Never uses a developer database."""
from pathlib import Path
import atexit,json,os,secrets,socket,subprocess,time
ROOT=Path(__file__).resolve().parents[3]
BASE=ROOT/'integration/.runtime/infrastructure'
PG=Path('/usr/local/opt/postgresql@13/bin')
KAFKA=ROOT/'integration/.runtime/deps/kafka_2.13-3.9.1'
def port():
 with socket.socket() as s:s.bind(('127.0.0.1',0));return s.getsockname()[1]
def command(args,**kw):return subprocess.run([str(v) for v in args],check=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True,**kw).stdout
class Infrastructure:
 def __init__(self):self.kafka=None;self.log=None;self.pg_started=False;self.env=os.environ.copy();self.env['JAVA_HOME']='/Library/Java/JavaVirtualMachines/jdk-11.jdk/Contents/Home';self.env['KAFKA_HEAP_OPTS']='-Xms256m -Xmx512m';self.path=BASE/(str(os.getpid())+'-'+secrets.token_hex(3));self.path.mkdir(parents=True);self.pgport=port();self.kafka_port=port();self.controller_port=port()
 def start(self):
  command([PG/'initdb','-D',self.path/'pg','-U','xone_test','-A','trust','--no-locale','-E','UTF8'])
  command([PG/'pg_ctl','-D',self.path/'pg','-l',self.path/'postgres.log','-o',f'-h 127.0.0.1 -p {self.pgport} -k {self.path}','-w','start']);self.pg_started=True
  self.dsn=f'postgresql://xone_test@127.0.0.1:{self.pgport}/postgres?sslmode=disable';self.dsnfile=self.path/'postgres-dsn';self.dsnfile.write_text(self.dsn);self.dsnfile.chmod(0o600)
  command([PG/'psql',self.dsn,'-v','ON_ERROR_STOP=1','-f',ROOT/'integration/postgres/schema.sql'])
  config=self.path/'kafka.properties';config.write_text(f'''process.roles=broker,controller
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
''')
  cluster=command([KAFKA/'bin/kafka-storage.sh','random-uuid'],env=self.env).strip();command([KAFKA/'bin/kafka-storage.sh','format','-t',cluster,'-c',config],env=self.env)
  self.log=open(self.path/'kafka.log','w');self.kafka=subprocess.Popen([str(KAFKA/'bin/kafka-server-start.sh'),str(config)],env=self.env,stdout=self.log,stderr=subprocess.STDOUT)
  deadline=time.monotonic()+45
  while time.monotonic()<deadline:
   if self.kafka.poll() is not None:raise RuntimeError('Kafka startup failed: '+str(self.path/'kafka.log'))
   try:
    with socket.create_connection(('127.0.0.1',self.kafka_port),timeout=.3):break
   except OSError:time.sleep(.1)
  else:raise RuntimeError('Kafka startup timeout')
  self.broker=f'127.0.0.1:{self.kafka_port}'
  for topic in ['canonical.evm-31337.v1','canonical.solana-localnet.v1','test.fencing','test.crash','test.reorg','test.router-reorg','test.analytics','test.fact-crash','test.abort','test.invalid-partitions']:
   command([KAFKA/'bin/kafka-topics.sh','--bootstrap-server',self.broker,'--create','--topic',topic,'--partitions','2' if topic=='test.invalid-partitions' else '1','--replication-factor','1'],env=self.env)
  return self
 def close(self):
  if self.kafka is not None:
   self.kafka.terminate()
   try:self.kafka.wait(timeout=10)
   except subprocess.TimeoutExpired:self.kafka.kill();self.kafka.wait()
  if self.log:self.log.close()
  if self.pg_started:
   try:command([PG/'pg_ctl','-D',self.path/'pg','-m','immediate','-w','stop'])
   except subprocess.CalledProcessError:pass
 def __enter__(self):
  try:return self.start()
  except Exception:self.close();raise
 def __exit__(self,*args):self.close()
