use serde_json::{json, Value};
use std::{
    io::Write,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, SyncSender},
        Arc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use xone_config::ConfigSnapshot;
#[derive(Default)]
pub struct Counters {
    pub dropped: AtomicU64,
    pub write_errors: AtomicU64,
}
enum Message {
    Event(Vec<u8>),
    Flush(SyncSender<()>),
}
#[derive(Clone)]
pub struct Logger {
    sender: SyncSender<Message>,
    pub counters: Arc<Counters>,
    service: String,
    role: String,
    release: String,
    public_hash: String,
    level: String,
    max_bytes: usize,
}
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
pub fn redact(value: Value) -> Value {
    match value {
        Value::Object(m) => Value::Object(
            m.into_iter()
                .map(|(k, v)| {
                    let lower = k.to_lowercase();
                    let sensitive = [
                        "authorization",
                        "password",
                        "secret",
                        "token",
                        "private_key",
                        "dsn",
                        "rpc_url",
                    ]
                    .iter()
                    .any(|s| lower.contains(s));
                    (
                        k,
                        if sensitive {
                            json!("[REDACTED]")
                        } else {
                            redact(v)
                        },
                    )
                })
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.into_iter().map(redact).collect()),
        v => v,
    }
}
fn rank(level: &str) -> u8 {
    match level {
        "debug" => 0,
        "info" => 1,
        "warn" => 2,
        _ => 3,
    }
}
impl Logger {
    pub fn new(cfg: &ConfigSnapshot) -> Self {
        Self::with_writer(cfg, Box::new(std::io::stdout()))
    }
    pub fn with_writer(cfg: &ConfigSnapshot, mut writer: Box<dyn Write + Send>) -> Self {
        let (sender, receiver) = mpsc::sync_channel::<Message>(cfg.config.logging.queue_capacity);
        let counters = Arc::new(Counters::default());
        let worker = counters.clone();
        std::thread::spawn(move || {
            while let Ok(message) = receiver.recv() {
                match message {
                    Message::Event(b) => {
                        if writer.write_all(&b).is_err() {
                            worker.write_errors.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    Message::Flush(done) => {
                        if writer.flush().is_err() {
                            worker.write_errors.fetch_add(1, Ordering::Relaxed);
                        }
                        let _ = done.try_send(());
                    }
                }
            }
        });
        Self {
            sender,
            counters,
            service: cfg.config.service.name.clone(),
            role: cfg.config.service.role.clone(),
            release: cfg.release.clone(),
            public_hash: cfg.public_hash.clone(),
            level: cfg.config.logging.level.clone(),
            max_bytes: cfg.config.logging.max_event_bytes,
        }
    }
    pub fn event(&self, level: &str, event: &str, fields: Value) {
        if rank(level) < rank(&self.level) {
            return;
        }
        let value = json!({"timestamp_ms":now_ms(),"service":self.service,"role":self.role,"level":level,"event":event,"config_release":self.release,"config_public_hash":self.public_hash,"fields":redact(fields)});
        if let Ok(mut b) = serde_json::to_vec(&value) {
            if b.len() <= self.max_bytes {
                b.push(b'\n');
                if self.sender.try_send(Message::Event(b)).is_ok() {
                    return;
                }
            }
        }
        self.counters.dropped.fetch_add(1, Ordering::Relaxed);
    }
    pub fn flush(&self) {
        let (tx, rx) = mpsc::sync_channel(1);
        if self.sender.try_send(Message::Flush(tx)).is_ok() {
            let _ = rx.recv_timeout(Duration::from_secs(1));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn credentials_and_injection_are_escaped() {
        let v =
            redact(json!({"authorization":"secret","nested":{"password":"hide"},"event":"a\nb"}));
        assert!(!v.to_string().contains("secret"));
        assert!(!v.to_string().contains("hide"));
        assert!(v.to_string().contains("a\\nb"));
    }
}
#[cfg(test)]
mod pressure_tests {
    use super::*;
    use std::sync::{Condvar, Mutex};
    struct Blocked(Arc<(Mutex<bool>, Condvar)>);
    impl Write for Blocked {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            let (lock, cond) = &*self.0;
            let mut released = lock.lock().unwrap();
            while !*released {
                released = cond.wait(released).unwrap();
            }
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn bounded_queue_reports_drops() {
        let mut cfg = xone_config::load("../../config/config-test.yaml", "dex-router")
            .or_else(|_| xone_config::load("../../config/config-test.yaml", "chain-parser-rust"))
            .unwrap();
        cfg.config.logging.queue_capacity = 1;
        cfg.config.logging.level = "info".into();
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let logger = Logger::with_writer(&cfg, Box::new(Blocked(gate.clone())));
        for _ in 0..100 {
            logger.event("info", "pressure", json!({}));
        }
        let dropped = logger.counters.dropped.load(Ordering::Relaxed);
        {
            let (lock, cond) = &*gate;
            *lock.lock().unwrap() = true;
            cond.notify_all();
        }
        logger.flush();
        assert!(dropped > 0);
    }
}
