#!/usr/bin/env python3
"""Run normal Rust owner and independent router in separate current test binaries."""
import os
from pathlib import Path
import re
import signal
import subprocess
import time
ROOT=Path(__file__).resolve().parents[3]

def stop(child):
    if child.poll() is None:
        os.killpg(child.pid,signal.SIGTERM)
        try:child.wait(timeout=3)
        except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait()

def run_live(out,broker,environment):
    directory=out/'actual-live-recovery';directory.mkdir(exist_ok=False)
    env=dict(environment,XONE_TEST_KAFKA_BROKER=broker,XONE_RUST_LIVE_RECOVERY_DIRECTORY=str(directory))
    owner_env=dict(env,CARGO_TARGET_DIR=str(ROOT/'integration/.runtime/design-review-parser-target'))
    router_env=dict(env,CARGO_TARGET_DIR=str(ROOT/'integration/.runtime/design-review-target'))
    owner_args=['cargo','+1.90.0','test','--locked','--manifest-path','chain-parser-rust/Cargo.toml','-p','xone-solana-ingest','--lib','actual_rust_owner_serves_uploaded_checkpoint_and_two_real_kafka_baselines','--','--include-ignored']
    router_args=['cargo','+1.90.0','test','--locked','--manifest-path','dex-router/Cargo.toml','-p','xone-quote-api','--lib','actual_rust_owner_remote_checkpoint_replays_into_independent_router','--','--include-ignored']
    with (out/'live-rust-owner.log').open('wb') as owner_log,(out/'live-independent-router.log').open('wb') as router_log:
        owner=subprocess.Popen(owner_args,cwd=ROOT,env=owner_env,stdout=owner_log,stderr=subprocess.STDOUT,start_new_session=True)
        router=None
        try:
            deadline=time.monotonic()+60
            while not (directory/'ready.json').is_file():
                if owner.poll() is not None:raise RuntimeError('Live Rust owner exited before readiness; see '+str(out/'live-rust-owner.log'))
                if time.monotonic()>deadline:raise RuntimeError('Live Rust owner ready deadline exceeded')
                time.sleep(.05)
            router=subprocess.Popen(router_args,cwd=ROOT,env=router_env,stdout=router_log,stderr=subprocess.STDOUT,start_new_session=True)
            try:code=router.wait(timeout=50)
            finally:(directory/'release').write_text('router execution finished\n')
            if code:raise RuntimeError('Independent router recovery failed; see '+str(out/'live-independent-router.log'))
            if owner.wait(timeout=10):raise RuntimeError('Live Rust owner completion failed')
        finally:
            (directory/'release').write_text('cleanup\n')
            if router is not None:stop(router)
            stop(owner)
    results=[]
    for name,required in [('live-rust-owner','service::checkpoint::recovery_tests::actual_rust_owner_serves_uploaded_checkpoint_and_two_real_kafka_baselines_to_external_router'),('live-independent-router','remote_checkpoint::live_rust_owner_tests::actual_rust_owner_remote_checkpoint_replays_into_independent_router_and_quotes_attested_head')]:
        raw=(out/(name+'.log')).read_bytes();text=raw.decode(errors='replace')
        if b'\0' in raw or 'test '+required+' ... ok' not in text:raise RuntimeError('Missing actual live PASS: '+name)
        results.append((name,raw,sum(int(n) for n in re.findall(r'test result: ok\. (\d+) passed;',text))))
    if not (directory/'independent-router.json').is_file():raise RuntimeError('Missing independent router actual HTTP proof')
    return results
