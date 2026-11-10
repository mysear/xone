//! One dedicated persistence thread per WAL, with one admitted capture total.
//!
//! Startup opens FrameWal synchronously. All capture/read/durable-head work and
//! the eventual WAL drop belong to this worker. Cancellation or a deadline seals
//! admission permanently: an unacknowledged frame may still become durable, but
//! its late result cannot be returned as a live receipt. Callers must invalidate
//! online readiness on every capture error/cancellation, then reanchor separately.
use crate::stream::{CapturedFrame, FrameWal, RawFrame, WalCursor};
use std::sync::{
    atomic::{AtomicU8, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot, watch};
use xone_domain::{err, Result};

const OPEN: u8 = 0;
const BUSY: u8 = 1;
const SEALED: u8 = 2;

struct Shared {
    phase: AtomicU8,
    sealed: watch::Sender<bool>,
}
impl Shared {
    fn seal(&self) {
        self.phase.store(SEALED, Ordering::SeqCst);
        self.sealed.send_replace(true);
    }
    fn is_sealed(&self) -> bool {
        self.phase.load(Ordering::SeqCst) == SEALED
    }
}
enum Operation {
    Capture(RawFrame),
    Checkpoint(WalCursor),
    Committed(crate::owner_scope::CommittedWal),
}
enum Command {
    Capture {
        frame: RawFrame,
        reply: oneshot::Sender<Result<CapturedFrame>>,
    },
    Checkpoint {
        cursor: WalCursor,
        reply: oneshot::Sender<Result<()>>,
    },
    Committed {
        proof: crate::owner_scope::CommittedWal,
        reply: oneshot::Sender<Result<()>>,
    },
    Stop,
}

/// Owns one fixed writer thread. Busy callers are rejected immediately, so there
/// are no queued semaphore waiters or additional per-frame blocking tasks.
/// A sealed writer cannot be reopened; create a fresh source session and reopen
/// the WAL only after this worker has actually released its RocksDB lock.
pub struct StreamWalWriter {
    sender: mpsc::Sender<Command>,
    shared: Arc<Shared>,
    exited: watch::Receiver<bool>,
    timeout: Duration,
}
impl StreamWalWriter {
    /// Called once during startup, transferring the WAL to its dedicated thread.
    /// The same timeout bounds a capture acknowledgement and each close wait.
    pub fn open(mut wal: FrameWal, write_timeout_ms: u64) -> Result<Self> {
        Self::start(
            move |operation| match operation {
                Operation::Capture(frame) => {
                    let cursor = wal.capture(frame)?;
                    wal.read(cursor.sequence).map(Some)
                }
                Operation::Committed(proof) => {
                    wal.commit_checkpoint(&proof)?;
                    Ok(None)
                }
                Operation::Checkpoint(cursor) => {
                    wal.checkpoint(&cursor)?;
                    Ok(None)
                }
            },
            write_timeout_ms,
        )
    }

    fn start(
        capture: impl FnMut(Operation) -> Result<Option<CapturedFrame>> + Send + 'static,
        write_timeout_ms: u64,
    ) -> Result<Self> {
        let timeout = Duration::from_millis(write_timeout_ms);
        if timeout.is_zero() || Instant::now().checked_add(timeout).is_none() {
            return Err(err("STREAM_WAL_WRITER_LIMITS"));
        }
        let (sender, receiver) = mpsc::channel(1);
        let (sealed, _) = watch::channel(false);
        let shared = Arc::new(Shared {
            phase: AtomicU8::new(OPEN),
            sealed,
        });
        let (finished, exited) = watch::channel(false);
        let worker_shared = shared.clone();
        // Dropping this JoinHandle detaches the one owned worker. Neither close
        // nor Drop joins it on Tokio or spawns a replacement/reaper thread.
        std::thread::Builder::new()
            .name("xone-stream-wal".into())
            .spawn(move || {
                let exit = WorkerExit {
                    shared: worker_shared.clone(),
                    finished,
                };
                // The closure (and its FrameWal) drops inside run_worker before the
                // exit guard announces completion, including during panic unwinding.
                run_worker(capture, receiver, &worker_shared);
                drop(exit);
            })
            .map_err(|_| err("STREAM_WAL_WRITER_SPAWN"))?;
        Ok(Self {
            sender,
            shared,
            exited,
            timeout,
        })
    }

    #[cfg(test)]
    fn with_capture(
        mut capture: impl FnMut(RawFrame) -> Result<CapturedFrame> + Send + 'static,
        write_timeout_ms: u64,
    ) -> Result<Self> {
        Self::start(
            move |operation| match operation {
                Operation::Capture(frame) => capture(frame).map(Some),
                Operation::Checkpoint(_) | Operation::Committed(_) => Ok(None),
            },
            write_timeout_ms,
        )
    }

    pub fn is_sealed(&self) -> bool {
        self.shared.is_sealed()
    }

    pub async fn capture(&self, frame: RawFrame) -> Result<CapturedFrame> {
        self.reserve()?.capture(frame).await
    }

    /// Called only AFTER the owner prepared journal has committed. Uses the same
    /// worker and cancellation seal as capture; a late ACK never grants readiness.
    pub async fn checkpoint(&self, cursor: WalCursor) -> Result<()> {
        let mut reservation = self.reserve()?;
        let deadline = tokio::time::Instant::now() + self.timeout;
        let (reply, receive) = oneshot::channel();
        self.sender
            .try_send(Command::Checkpoint { cursor, reply })
            .map_err(|_| err("STREAM_WAL_WRITER_STOPPED"))?;
        tokio::time::timeout_at(deadline, receive)
            .await
            .map_err(|_| err("STREAM_WAL_WRITER_TIMEOUT"))?
            .map_err(|_| err("STREAM_WAL_WRITER_STOPPED"))??;
        if tokio::time::Instant::now() >= deadline {
            return Err(err("STREAM_WAL_WRITER_TIMEOUT"));
        }
        self.shared
            .phase
            .compare_exchange(BUSY, OPEN, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| err("STREAM_WAL_WRITER_SEALED"))?;
        reservation.acknowledged = true;
        Ok(())
    }

    pub async fn checkpoint_committed(
        &self,
        proof: crate::owner_scope::CommittedWal,
    ) -> Result<()> {
        let mut reservation = self.reserve()?;
        let deadline = tokio::time::Instant::now() + self.timeout;
        let (reply, wait) = oneshot::channel();
        self.sender
            .try_send(Command::Committed { proof, reply })
            .map_err(|_| err("STREAM_WAL_WRITER_STOPPED"))?;
        tokio::time::timeout_at(deadline, wait)
            .await
            .map_err(|_| err("STREAM_WAL_WRITER_TIMEOUT"))?
            .map_err(|_| err("STREAM_WAL_WRITER_STOPPED"))??;
        if tokio::time::Instant::now() >= deadline {
            return Err(err("STREAM_WAL_WRITER_TIMEOUT"));
        }
        self.shared
            .phase
            .compare_exchange(BUSY, OPEN, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| err("STREAM_WAL_WRITER_SEALED"))?;
        reservation.acknowledged = true;
        Ok(())
    }
    /// Seals synchronously, then waits asynchronously for WAL destruction. A
    /// timeout leaves the original worker/lock in place; repeated close calls
    /// only wait for that same worker. OS-blocked I/O cannot be forcibly killed.
    pub async fn close(&self) -> Result<()> {
        self.seal();
        let mut exited = self.exited.clone();
        if *exited.borrow() {
            return Ok(());
        }
        let deadline = tokio::time::Instant::now() + self.timeout;
        tokio::time::timeout_at(deadline, exited.wait_for(|done| *done))
            .await
            .map_err(|_| err("STREAM_WAL_WRITER_CLOSE_TIMEOUT"))?
            .map_err(|_| err("STREAM_WAL_WRITER_STOPPED"))?;
        if tokio::time::Instant::now() >= deadline {
            return Err(err("STREAM_WAL_WRITER_CLOSE_TIMEOUT"));
        }
        Ok(())
    }

    fn seal(&self) {
        self.shared.seal();
        // A full queue already contains the sole admitted capture (or Stop).
        // In that case the worker observes SEALED when it dequeues/finishes it.
        let _ = self.sender.try_send(Command::Stop);
    }

    /// Reserve before polling the network. Dropping this guard seals even when
    /// cancellation happens while waiting for the next frame, before enqueue.
    pub(crate) fn reserve(&self) -> Result<CaptureReservation<'_>> {
        match self
            .shared
            .phase
            .compare_exchange(OPEN, BUSY, Ordering::SeqCst, Ordering::SeqCst)
        {
            Ok(_) => Ok(CaptureReservation {
                writer: self,
                acknowledged: false,
            }),
            Err(BUSY) => Err(err("STREAM_WAL_WRITER_BUSY")),
            Err(_) => Err(err("STREAM_WAL_WRITER_SEALED")),
        }
    }
}
impl Drop for StreamWalWriter {
    fn drop(&mut self) {
        self.seal();
    }
}

pub(crate) struct CaptureReservation<'a> {
    writer: &'a StreamWalWriter,
    acknowledged: bool,
}
impl CaptureReservation<'_> {
    pub(crate) async fn cancelled(&self) {
        let mut sealed = self.writer.shared.sealed.subscribe();
        if !self.writer.is_sealed() {
            let _ = sealed.wait_for(|sealed| *sealed).await;
        }
    }

    pub(crate) async fn capture(mut self, frame: RawFrame) -> Result<CapturedFrame> {
        if self.writer.is_sealed() {
            return Err(err("STREAM_WAL_WRITER_SEALED"));
        }
        let deadline = tokio::time::Instant::now() + self.writer.timeout;
        let (reply, receiver) = oneshot::channel();
        self.writer
            .sender
            .try_send(Command::Capture { frame, reply })
            .map_err(|_| err("STREAM_WAL_WRITER_STOPPED"))?;
        let record = wait_capture_result(deadline, receiver, self.cancelled()).await?;
        // timeout polls its inner future first. If scheduling resumes with both
        // the timer and a late ACK ready, the ACK still must not grant admission.
        if tokio::time::Instant::now() >= deadline {
            return Err(err("STREAM_WAL_WRITER_TIMEOUT"));
        }
        // close/cancel may seal after the worker sends a receipt but before this
        // future consumes it. Only the still-active reservation can admit it.
        self.writer
            .shared
            .phase
            .compare_exchange(BUSY, OPEN, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| err("STREAM_WAL_WRITER_SEALED"))?;
        self.acknowledged = true;
        Ok(record)
    }
}
// Keep reply/cancellation arbitration separate from the receipt admission gate.
async fn wait_capture_result(
    deadline: tokio::time::Instant,
    mut receiver: oneshot::Receiver<Result<CapturedFrame>>,
    cancelled: impl std::future::Future<Output = ()>,
) -> Result<CapturedFrame> {
    tokio::select! {
        biased;
        result = tokio::time::timeout_at(deadline, &mut receiver) => {
            result.map_err(|_| err("STREAM_WAL_WRITER_TIMEOUT"))?
                .map_err(|_| err("STREAM_WAL_WRITER_STOPPED"))?
        }
        _ = cancelled => {
            // The reply may have been polled Pending just before the worker
            // sent its error and sealed. Preserve only an already-ready error:
            // never wait for blocked I/O or admit a success after sealing.
            match receiver.try_recv() {
                Ok(Err(error)) => Err(error),
                _ => Err(err("STREAM_WAL_WRITER_SEALED")),
            }
        },
    }
}
impl Drop for CaptureReservation<'_> {
    fn drop(&mut self) {
        if !self.acknowledged {
            self.writer.seal();
        }
    }
}

struct WorkerExit {
    shared: Arc<Shared>,
    finished: watch::Sender<bool>,
}
impl Drop for WorkerExit {
    fn drop(&mut self) {
        self.shared.seal();
        self.finished.send_replace(true);
    }
}
fn run_worker(
    mut capture: impl FnMut(Operation) -> Result<Option<CapturedFrame>>,
    mut receiver: mpsc::Receiver<Command>,
    shared: &Shared,
) {
    while let Some(command) = receiver.blocking_recv() {
        let (frame, reply) = match command {
            Command::Capture { frame, reply } => (frame, reply),
            Command::Committed { proof, reply } => {
                if shared.is_sealed() {
                    let _ = reply.send(Err(err("STREAM_WAL_WRITER_SEALED")));
                    return;
                }
                let result = capture(Operation::Committed(proof)).map(|_| ());
                let failed = result.is_err();
                let _ = reply.send(result);
                if failed {
                    shared.seal();
                    return;
                }
                continue;
            }
            Command::Checkpoint { cursor, reply } => {
                if shared.is_sealed() {
                    let _ = reply.send(Err(err("STREAM_WAL_WRITER_SEALED")));
                    return;
                }
                let result = capture(Operation::Checkpoint(cursor)).map(|_| ());
                let failed = result.is_err();
                let _ = reply.send(result);
                if failed {
                    shared.seal();
                    return;
                }
                continue;
            }
            Command::Stop => return,
        };
        if shared.is_sealed() {
            let _ = reply.send(Err(err("STREAM_WAL_WRITER_SEALED")));
            return;
        }
        match capture(Operation::Capture(frame))
            .and_then(|r| r.ok_or_else(|| err("STREAM_WAL_WRITER_STOPPED")))
        {
            Err(error) => {
                // Deliver the storage error before notifying cancellation. The
                // reservation stays BUSY until its error path seals admission.
                let _ = reply.send(Err(error));
                shared.seal();
                return;
            }
            Ok(record) => {
                let result = if shared.is_sealed() {
                    Err(err("STREAM_WAL_WRITER_SEALED"))
                } else {
                    Ok(record)
                };
                let _ = reply.send(result);
                if shared.is_sealed() {
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stream::{RawFrame, SourceSession, WalCursor};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Condvar, Mutex,
    };
    use std::time::{Duration, Instant};

    fn frame(sequence: u64) -> RawFrame {
        RawFrame {
            source: SourceSession {
                source_id: "provider".into(),
                session_id: "session".into(),
            },
            sequence,
            receive_timestamp_ms: 123,
            protobuf: vec![0, 255, sequence as u8],
        }
    }
    fn receipt(frame: RawFrame) -> CapturedFrame {
        CapturedFrame {
            cursor: WalCursor {
                sequence: frame.sequence,
                record_sha256: "test-only".into(),
            },
            wal_generation: 1,
            frame,
        }
    }
    // The controller releases even on an assertion failure; no test leaves a
    // deliberately blocked writer behind. The worker owns only the inner Arc.
    struct Gate(Arc<(Mutex<bool>, Condvar)>);
    impl Gate {
        fn new() -> Self {
            Self(Arc::new((Mutex::new(false), Condvar::new())))
        }
        fn release(&self) {
            *self.0 .0.lock().unwrap() = true;
            self.0 .1.notify_all();
        }
    }
    impl Drop for Gate {
        fn drop(&mut self) {
            self.release();
        }
    }
    fn wait(gate: &Arc<(Mutex<bool>, Condvar)>) {
        let mut released = gate.0.lock().unwrap();
        while !*released {
            released = gate.1.wait(released).unwrap();
        }
    }
    fn blocked(
        timeout_ms: u64,
    ) -> (
        Arc<StreamWalWriter>,
        Gate,
        tokio::sync::watch::Receiver<usize>,
        Arc<AtomicUsize>,
    ) {
        let gate = Gate::new();
        let worker_gate = gate.0.clone();
        let calls = Arc::new(AtomicUsize::new(0));
        let worker_calls = calls.clone();
        let (started, receiver) = tokio::sync::watch::channel(0);
        let writer = StreamWalWriter::with_capture(
            move |frame| {
                let count = worker_calls.fetch_add(1, Ordering::SeqCst) + 1;
                started.send_replace(count);
                wait(&worker_gate);
                Ok(receipt(frame))
            },
            timeout_ms,
        )
        .unwrap();
        (Arc::new(writer), gate, receiver, calls)
    }
    async fn entered(started: &mut tokio::sync::watch::Receiver<usize>) {
        tokio::time::timeout(Duration::from_secs(2), started.wait_for(|count| *count > 0))
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn slow_sync_capture_obeys_deadline_and_keeps_tokio_responsive() {
        let writer = StreamWalWriter::with_capture(
            |frame| {
                std::thread::sleep(Duration::from_millis(200));
                Ok(receipt(frame))
            },
            25,
        )
        .unwrap();
        let ticked = Arc::new(AtomicUsize::new(0));
        let tick = ticked.clone();
        let heartbeat = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(5)).await;
            tick.store(1, Ordering::SeqCst);
        });
        let start = Instant::now();
        let result = writer.capture(frame(1)).await;
        assert_eq!(result.unwrap_err().0, "STREAM_WAL_WRITER_TIMEOUT");
        assert!(
            start.elapsed() < Duration::from_millis(150),
            "sync capture blocked the runtime"
        );
        assert_eq!(ticked.load(Ordering::SeqCst), 1);
        assert!(writer.is_sealed());
        assert_eq!(
            writer.capture(frame(2)).await.unwrap_err().0,
            "STREAM_WAL_WRITER_SEALED"
        );
        heartbeat.await.unwrap();
        let mut exited = writer.exited.clone();
        tokio::time::timeout(Duration::from_secs(2), exited.wait_for(|done| *done))
            .await
            .unwrap()
            .unwrap();
        writer.close().await.unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_late_ready_ack_cannot_bypass_the_write_deadline() {
        use std::{
            future::Future,
            task::{Context, Poll, Wake, Waker},
        };
        struct AckWake(std::sync::mpsc::SyncSender<()>);
        impl Wake for AckWake {
            fn wake(self: Arc<Self>) {
                let _ = self.0.try_send(());
            }
            fn wake_by_ref(self: &Arc<Self>) {
                let _ = self.0.try_send(());
            }
        }
        let writer = StreamWalWriter::with_capture(
            |frame| {
                std::thread::sleep(Duration::from_millis(60));
                Ok(receipt(frame))
            },
            25,
        )
        .unwrap();
        let mut capture = Box::pin(writer.capture(frame(1)));
        let (wake, woken) = std::sync::mpsc::sync_channel(1);
        let waker = Waker::from(Arc::new(AckWake(wake)));
        let mut context = Context::from_waker(&waker);
        assert!(matches!(capture.as_mut().poll(&mut context), Poll::Pending));
        // Deliberately pause this test runtime until the writer's ack is ready.
        // Tokio's timeout polls its inner future first; checking only that
        // combinator could admit a late success when both ack and timer are ready.
        woken.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(capture.await.unwrap_err().0, "STREAM_WAL_WRITER_TIMEOUT");
        assert!(writer.is_sealed());
        writer.close().await.unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancelled_capture_seals_and_discards_the_late_ack() {
        let (writer, gate, mut started, calls) = blocked(1000);
        let capturing = writer.clone();
        let task = tokio::spawn(async move { capturing.capture(frame(1)).await });
        entered(&mut started).await;
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(writer.is_sealed());
        assert_eq!(
            writer.capture(frame(2)).await.unwrap_err().0,
            "STREAM_WAL_WRITER_SEALED"
        );
        gate.release();
        writer.close().await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn concurrent_capture_is_rejected_without_queuing_or_sealing_the_active_write() {
        let (writer, gate, mut started, calls) = blocked(1000);
        let capturing = writer.clone();
        let task = tokio::spawn(async move { capturing.capture(frame(1)).await });
        entered(&mut started).await;
        for sequence in 2..102 {
            assert_eq!(
                writer.capture(frame(sequence)).await.unwrap_err().0,
                "STREAM_WAL_WRITER_BUSY"
            );
        }
        assert!(!writer.is_sealed());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        gate.release();
        assert_eq!(task.await.unwrap().unwrap().frame.sequence, 1);
        assert_eq!(
            writer.capture(frame(102)).await.unwrap().frame.sequence,
            102
        );
        writer.close().await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn close_is_bounded_repeatable_and_prevents_an_inflight_ack_from_becoming_live() {
        let (writer, gate, mut started, calls) = blocked(25);
        let capturing = writer.clone();
        let task = tokio::spawn(async move { capturing.capture(frame(1)).await });
        entered(&mut started).await;
        for _ in 0..3 {
            let start = Instant::now();
            assert_eq!(
                writer.close().await.unwrap_err().0,
                "STREAM_WAL_WRITER_CLOSE_TIMEOUT"
            );
            assert!(start.elapsed() < Duration::from_millis(150));
        }
        assert!(task.await.unwrap().is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(writer.is_sealed());
        gate.release();
        writer.close().await.unwrap();
        writer.close().await.unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn late_worker_exit_cannot_bypass_the_close_deadline() {
        use std::{
            future::Future,
            task::{Context, Poll, Wake, Waker},
        };
        struct ExitWake(std::sync::mpsc::SyncSender<()>);
        impl Wake for ExitWake {
            fn wake(self: Arc<Self>) {
                let _ = self.0.try_send(());
            }
            fn wake_by_ref(self: &Arc<Self>) {
                let _ = self.0.try_send(());
            }
        }
        let (writer, gate, mut started, _) = blocked(25);
        let capturing = writer.clone();
        let task = tokio::spawn(async move { capturing.capture(frame(1)).await });
        entered(&mut started).await;
        let mut close = Box::pin(writer.close());
        let (wake, woken) = std::sync::mpsc::sync_channel(1);
        let waker = Waker::from(Arc::new(ExitWake(wake)));
        let mut context = Context::from_waker(&waker);
        assert!(matches!(close.as_mut().poll(&mut context), Poll::Pending));
        std::thread::sleep(Duration::from_millis(60));
        gate.release();
        woken.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(
            close.await.unwrap_err().0,
            "STREAM_WAL_WRITER_CLOSE_TIMEOUT"
        );
        assert!(task.await.unwrap().is_err());
        writer.close().await.unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn error_between_reply_poll_and_seal_poll_preserves_the_specific_cause() {
        let writer = StreamWalWriter::with_capture(|frame| Ok(receipt(frame)), 1000).unwrap();
        let reservation = writer.reserve().unwrap();
        let (reply, receiver) = oneshot::channel();
        let deadline = tokio::time::Instant::now() + writer.timeout;
        // Biased select polls the empty reply first. Its second branch then
        // performs the exact worker ordering: specific error, followed by seal.
        // No sleep or scheduler probability is involved in this interleaving.
        let result = wait_capture_result(deadline, receiver, async {
            reply.send(Err(err("TEST_WRITE_FAILED"))).unwrap();
            writer.shared.seal();
        })
        .await;
        assert_eq!(result.unwrap_err().0, "TEST_WRITE_FAILED");
        drop(reservation);
        assert_eq!(
            writer.capture(frame(2)).await.unwrap_err().0,
            "STREAM_WAL_WRITER_SEALED"
        );
        writer.close().await.unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn successful_reply_between_polls_cannot_bypass_a_seal() {
        let writer = StreamWalWriter::with_capture(|frame| Ok(receipt(frame)), 1000).unwrap();
        let reservation = writer.reserve().unwrap();
        let (reply, receiver) = oneshot::channel();
        let deadline = tokio::time::Instant::now() + writer.timeout;
        let result = wait_capture_result(deadline, receiver, async {
            reply.send(Ok(receipt(frame(1)))).unwrap();
            writer.shared.seal();
        })
        .await;
        assert_eq!(result.unwrap_err().0, "STREAM_WAL_WRITER_SEALED");
        drop(reservation);
        assert_eq!(
            writer.capture(frame(2)).await.unwrap_err().0,
            "STREAM_WAL_WRITER_SEALED"
        );
        writer.close().await.unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn seal_without_a_reply_returns_immediately_and_discards_late_error() {
        use std::future::Future;
        use std::task::{Context, Poll, Waker};
        let writer = StreamWalWriter::with_capture(|frame| Ok(receipt(frame)), 1000).unwrap();
        let reservation = writer.reserve().unwrap();
        let (reply, receiver) = oneshot::channel();
        let deadline = tokio::time::Instant::now() + writer.timeout;
        let result = {
            let mut wait = Box::pin(wait_capture_result(deadline, receiver, async {
                writer.shared.seal();
            }));
            let mut context = Context::from_waker(Waker::noop());
            match wait.as_mut().poll(&mut context) {
                Poll::Ready(result) => result,
                Poll::Pending => panic!("sealing must not wait for a possibly blocked writer"),
            }
        };
        assert_eq!(result.unwrap_err().0, "STREAM_WAL_WRITER_SEALED");
        assert!(reply.send(Err(err("TEST_LATE_WRITE_FAILED"))).is_err());
        drop(reservation);
        writer.close().await.unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_synchronous_writer_error_is_terminal_and_preserves_the_error() {
        let writer =
            StreamWalWriter::with_capture(|_| Err(xone_domain::err("TEST_WRITE_FAILED")), 1000)
                .unwrap();
        assert_eq!(
            writer.capture(frame(1)).await.unwrap_err().0,
            "TEST_WRITE_FAILED"
        );
        assert!(writer.is_sealed());
        assert_eq!(
            writer.capture(frame(2)).await.unwrap_err().0,
            "STREAM_WAL_WRITER_SEALED"
        );
        writer.close().await.unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_panicking_writer_seals_and_close_observes_its_exit() {
        let writer =
            StreamWalWriter::with_capture(|_| panic!("injected writer panic"), 1000).unwrap();
        assert!(writer.capture(frame(1)).await.is_err());
        assert!(writer.is_sealed());
        writer.close().await.unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn successful_captures_always_run_on_one_dedicated_thread() {
        let threads = Arc::new(Mutex::new(Vec::new()));
        let worker_threads = threads.clone();
        let runtime_thread = std::thread::current().id();
        let writer = StreamWalWriter::with_capture(
            move |frame| {
                worker_threads
                    .lock()
                    .unwrap()
                    .push(std::thread::current().id());
                Ok(receipt(frame))
            },
            1000,
        )
        .unwrap();
        for sequence in 1..9 {
            writer.capture(frame(sequence)).await.unwrap();
        }
        writer.close().await.unwrap();
        let threads = threads.lock().unwrap();
        assert_eq!(threads.len(), 8);
        assert_ne!(threads[0], runtime_thread);
        assert!(threads.iter().all(|thread| *thread == threads[0]));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancelling_a_network_reservation_seals_before_any_frame_can_be_queued() {
        let (writer, _gate, _started, calls) = blocked(1000);
        let reservation = writer.reserve().unwrap();
        drop(reservation);
        assert!(writer.is_sealed());
        assert_eq!(
            writer.capture(frame(1)).await.unwrap_err().0,
            "STREAM_WAL_WRITER_SEALED"
        );
        writer.close().await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn close_cancels_a_network_reservation_without_waiting_for_a_frame() {
        let (writer, _gate, _started, calls) = blocked(1000);
        let reservation = writer.reserve().unwrap();
        writer.close().await.unwrap();
        tokio::time::timeout(Duration::from_millis(100), reservation.cancelled())
            .await
            .unwrap();
        assert_eq!(
            reservation.capture(frame(1)).await.unwrap_err().0,
            "STREAM_WAL_WRITER_SEALED"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_timed_out_real_wal_keeps_its_lock_until_the_original_thread_exits() {
        use crate::stream::{FrameWal, FrameWalLimits};
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "xone-async-wal-lock-{}-{unique}",
            std::process::id()
        ));
        struct Temp(std::path::PathBuf);
        impl Drop for Temp {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let directory = Temp(path);
        let limits = FrameWalLimits {
            max_frame_bytes: 1024,
            max_total_bytes: 10000,
            max_records: 10,
        };
        let mut wal = FrameWal::open(&directory.0, "solana:test", limits).unwrap();
        let gate = Gate::new();
        let worker_gate = gate.0.clone();
        let writer = StreamWalWriter::with_capture(
            move |frame| {
                wait(&worker_gate);
                let cursor = wal.capture(frame)?;
                wal.read(cursor.sequence)
            },
            25,
        )
        .unwrap();
        assert_eq!(
            writer.capture(frame(1)).await.unwrap_err().0,
            "STREAM_WAL_WRITER_TIMEOUT"
        );
        assert_eq!(
            writer.close().await.unwrap_err().0,
            "STREAM_WAL_WRITER_CLOSE_TIMEOUT"
        );
        assert!(
            FrameWal::open(&directory.0, "solana:test", limits).is_err(),
            "a timed-out worker must retain its WAL lock"
        );
        gate.release();
        let mut exited = writer.exited.clone();
        tokio::time::timeout(Duration::from_secs(2), exited.wait_for(|done| *done))
            .await
            .unwrap()
            .unwrap();
        writer.close().await.unwrap();
        assert_eq!(
            writer.capture(frame(2)).await.unwrap_err().0,
            "STREAM_WAL_WRITER_SEALED"
        );
        let reopened = FrameWal::open(&directory.0, "solana:test", limits).unwrap();
        assert_eq!(reopened.len(), 1);
        assert_eq!(reopened.read(1).unwrap().frame, frame(1));
        drop(reopened);
    }

    #[test]
    fn zero_write_timeout_is_rejected_before_starting_a_worker() {
        assert!(StreamWalWriter::with_capture(|frame| Ok(receipt(frame)), 0).is_err());
    }
}
