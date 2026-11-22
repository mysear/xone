use super::*;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;

type Wake = Option<oneshot::Sender<()>>;

pub(super) struct Worker {
    wake: mpsc::Sender<Wake>,
    task: JoinHandle<()>,
}

impl Parser {
    pub(super) async fn start_chain_workers(&self) -> Result<()> {
        let chains = &self.config.config.chains;
        let (send, mut results) = mpsc::unbounded_channel();
        {
            let mut workers = self.workers.lock().expect("chain worker registry");
            for (index, chain) in chains.iter().cloned().enumerate() {
                let parser = self.clone();
                let (wake, receiver) = mpsc::channel(1);
                let initial = send.clone();
                let task = tokio::spawn(async move {
                    let mut stop = parser.stream_stop.subscribe();
                    let result = {
                        let _actor = parser.chain_actors[&chain.key].lock().await;
                        parser.initialize_worker(&chain).await
                    };
                    let _ = initial.send((index, result));
                    drop(initial);
                    parser.run_chain_worker(&chain, receiver, &mut stop).await;
                });
                workers.push(Worker { wake, task });
            }
        }
        drop(send);
        let mut first_error = None;
        while let Some((index, result)) = results.recv().await {
            self.check_startup_open()?;
            match result {
                Ok(()) => return Ok(()),
                Err(error) if first_error.as_ref().is_none_or(|(i, _)| index < *i) => {
                    first_error = Some((index, error));
                }
                Err(_) => (),
            }
        }
        Err(first_error.map_or_else(|| err("NO_INITIALIZED_CHAINS"), |(_, error)| error))
    }

    async fn initialize_worker(&self, chain: &Chain) -> Result<()> {
        self.check_startup_open()?;
        let result = self.initialize_chain(chain, true).await;
        self.check_startup_open()?;
        match result {
            Ok(()) => self.clear_pending_start(&chain.key),
            Err(error) => {
                self.isolate_start_failure(chain, &error).await?;
                if error.0 != "LEASE_NOT_OWNED" {
                    return Err(error);
                }
                self.logger
                    .event("info", "writer_standby", json!({"chain_key":chain.key}));
                Ok(())
            }
        }
    }

    async fn run_chain_worker(
        &self,
        chain: &Chain,
        mut receiver: mpsc::Receiver<Wake>,
        stop: &mut watch::Receiver<bool>,
    ) {
        let renewal = Duration::from_millis(self.config.config.control.renewal_ms.max(1));
        let mut timer = tokio::time::interval_at(tokio::time::Instant::now() + renewal, renewal);
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            if *stop.borrow() {
                return;
            }
            tokio::select! {
                biased;
                _ = stop.changed() => return,
                wake = receiver.recv() => {
                    let Some(done) = wake else { return };
                    {
                        let _actor = self.chain_actors[&chain.key].lock().await;
                        self.tick_chain(chain).await;
                    }
                    if let Some(done) = done { let _ = done.send(()); }
                }
                _ = timer.tick(), if self.control.get().is_some() => {
                    let _actor = self.chain_actors[&chain.key].lock().await;
                    if !*stop.borrow() { self.renew_chain(chain).await; }
                }
            }
        }
    }

    pub(super) async fn renew_chain(&self, chain: &Chain) {
        let Some(pg) = self.control.get() else { return };
        let mut writer = self.writers[&chain.key].lock().await;
        if let Some(active) = writer.as_mut() {
            if active.heartbeat(pg, &self.config).await.is_err() {
                active.close(pg, &self.config).await;
                *writer = None;
                self.stores[&chain.key].lock().map(|mut s| s.suspend()).ok();
            } else {
                self.stores[&chain.key]
                    .lock()
                    .map(|mut s| s.set_authority_deadline(active.deadline(&self.config)))
                    .ok();
            }
        }
    }

    // The scheduler never detaches a publication future. A chain worker owns
    // each accepted wake through Kafka commit and local reconciliation.
    pub(super) fn schedule_tick(&self) {
        if !self.started.load(Ordering::Acquire) || *self.stream_stop.borrow() {
            return;
        }
        for worker in self.workers.lock().expect("chain worker registry").iter() {
            let _ = worker.wake.try_send(None);
        }
    }

    pub async fn tick(&self) {
        if !self.started.load(Ordering::Acquire) || *self.stream_stop.borrow() {
            return;
        }
        let mut replies = Vec::new();
        for worker in self.workers.lock().expect("chain worker registry").iter() {
            let (done, reply) = oneshot::channel();
            if worker.wake.try_send(Some(done)).is_ok() {
                replies.push(reply);
            }
        }
        for reply in replies {
            let _ = reply.await;
        }
    }

    pub(super) async fn drain_chain_workers(&self) {
        let workers = std::mem::take(&mut *self.workers.lock().expect("chain worker registry"));
        for Worker { wake, task } in workers {
            drop(wake);
            let _ = task.await;
        }
    }
}
