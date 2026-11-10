use prost::Message;
use std::{
    convert::Infallible,
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    task::{Context, Poll},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tonic::codegen::{http, Service};
use xone_solana_ingest::{
    async_frame_wal::StreamWalWriter,
    stream::{FrameWal, FrameWalLimits},
    yellowstone::{BlockFilter, RawCodec, SubscribeRequest, Subscription, TransportConfig},
};

fn config(endpoint: &str) -> TransportConfig {
    TransportConfig {
        endpoint: endpoint.into(),
        source_id: "test-provider".into(),
        connect_timeout_ms: 1000,
        idle_timeout_ms: 1000,
        max_frame_bytes: 1024,
        allow_loopback_plaintext: false,
    }
}
#[test]
fn endpoint_auth_and_resource_admission_fail_before_connect() {
    let good = config("https://geyser.example");
    assert!(good.validate().is_ok());
    for url in [
        "http://geyser.example",
        "https://user:secret@geyser.example",
        "https://geyser.example/path",
        "https://geyser.example?token=secret",
    ] {
        assert!(
            TransportConfig {
                endpoint: url.into(),
                ..good.clone()
            }
            .validate()
            .is_err(),
            "{url}"
        );
    }
    for c in [
        TransportConfig {
            source_id: String::new(),
            ..good.clone()
        },
        TransportConfig {
            max_frame_bytes: 0,
            ..good.clone()
        },
        TransportConfig {
            idle_timeout_ms: 0,
            ..good.clone()
        },
    ] {
        assert!(c.validate().is_err());
    }
    assert!(TransportConfig {
        endpoint: "http://127.0.0.1:1234".into(),
        allow_loopback_plaintext: true,
        ..good
    }
    .validate()
    .is_ok());
}
#[test]
fn subscribe_request_uses_upstream_field_tags_and_replay_slot() {
    let request = SubscribeRequest {
        blocks: std::collections::HashMap::from([(
            "full".into(),
            BlockFilter {
                account_include: vec![],
                include_accounts: Some(true),
                include_transactions: Some(true),
                include_entries: Some(true),
            },
        )]),
        slots: Default::default(),
        commitment: Some(2),
        from_slot: Some(42),
    };
    let bytes = request.encode_to_vec();
    assert_eq!(SubscribeRequest::decode(bytes.as_slice()).unwrap(), request);
    assert!(bytes.windows(2).any(|b| b == [0x58, 42]));
    assert!(bytes.windows(2).any(|b| b == [0x30, 2]));
}

type Frames = tokio::sync::mpsc::Receiver<Result<Vec<u8>, tonic::Status>>;
#[derive(Clone)]
struct Geyser {
    frames: Arc<Mutex<Option<Frames>>>,
    observed: Arc<Mutex<Option<SubscribeRequest>>>,
}
struct Subscribe(Geyser);
impl tonic::server::StreamingService<Vec<u8>> for Subscribe {
    type Response = Vec<u8>;
    type ResponseStream = tokio_stream::wrappers::ReceiverStream<Result<Vec<u8>, tonic::Status>>;
    type Future = Pin<
        Box<
            dyn Future<Output = Result<tonic::Response<Self::ResponseStream>, tonic::Status>>
                + Send,
        >,
    >;
    fn call(&mut self, mut request: tonic::Request<tonic::Streaming<Vec<u8>>>) -> Self::Future {
        let service = self.0.clone();
        Box::pin(async move {
            if request
                .metadata()
                .get("x-token")
                .and_then(|value| value.to_str().ok())
                != Some("private-token")
            {
                return Err(tonic::Status::unauthenticated("token"));
            }
            let data = request.get_mut().message().await?.unwrap();
            let value = SubscribeRequest::decode(data.as_slice()).unwrap();
            assert!(value.blocks["full-block"].account_include.is_empty());
            assert_eq!(value.commitment, Some(2));
            *service.observed.lock().unwrap() = Some(value);
            let receiver = service.frames.lock().unwrap().take().unwrap();
            Ok(tonic::Response::new(
                tokio_stream::wrappers::ReceiverStream::new(receiver),
            ))
        })
    }
}
impl Service<http::Request<tonic::body::Body>> for Geyser {
    type Response = http::Response<tonic::body::Body>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;
    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
    fn call(&mut self, request: http::Request<tonic::body::Body>) -> Self::Future {
        let service = self.clone();
        Box::pin(async move {
            let mut grpc = tonic::server::Grpc::new(RawCodec);
            Ok(grpc.streaming(Subscribe(service), request).await)
        })
    }
}
impl tonic::server::NamedService for Geyser {
    const NAME: &'static str = "geyser.Geyser";
}
struct Fixture {
    config: TransportConfig,
    frames: Option<tokio::sync::mpsc::Sender<Result<Vec<u8>, tonic::Status>>>,
    observed: Arc<Mutex<Option<SubscribeRequest>>>,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    server: tokio::task::JoinHandle<()>,
}
impl Fixture {
    async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (frames, receiver) = tokio::sync::mpsc::channel(1);
        let observed = Arc::new(Mutex::new(None));
        let service = Geyser {
            frames: Arc::new(Mutex::new(Some(receiver))),
            observed: observed.clone(),
        };
        let (stop, shutdown) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(service)
                .serve_with_incoming_shutdown(
                    tokio_stream::wrappers::TcpListenerStream::new(listener),
                    async {
                        let _ = shutdown.await;
                    },
                )
                .await
                .unwrap();
        });
        Self {
            config: TransportConfig {
                allow_loopback_plaintext: true,
                ..config(&format!("http://{address}"))
            },
            frames: Some(frames),
            observed,
            shutdown: Some(stop),
            server,
        }
    }
    async fn connect(&self, from_slot: Option<u64>) -> Subscription {
        Subscription::connect(
            self.config.clone(),
            "new-session".into(),
            Some("private-token"),
            from_slot,
        )
        .await
        .unwrap()
    }
    async fn finish(mut self) {
        self.frames.take();
        let _ = self.shutdown.take().unwrap().send(());
        tokio::time::timeout(Duration::from_secs(2), &mut self.server)
            .await
            .unwrap()
            .unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Self(std::env::temp_dir().join(format!(
            "xone-async-geyser-{}-{unique}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        )))
    }
    fn open(&self) -> FrameWal {
        FrameWal::open(
            &self.0,
            "solana:test",
            FrameWalLimits {
                max_frame_bytes: 1024,
                max_total_bytes: 50000,
                max_records: 10,
            },
        )
        .unwrap()
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn real_grpc_subscription_retains_exact_invalid_protobuf_before_decode_and_rejects_end() {
    let mut fixture = Fixture::start().await;
    let directory = Temp::new();
    let writer = StreamWalWriter::open(directory.open(), 1000).unwrap();
    let mut stream = fixture.connect(Some(42)).await;
    fixture
        .frames
        .as_ref()
        .unwrap()
        .send(Ok(vec![0, 255, 170]))
        .await
        .unwrap();
    let captured = stream.capture_next(&writer).await.unwrap();
    assert_eq!(captured.frame.protobuf, vec![0, 255, 170]);
    fixture.frames.take();
    assert!(stream.capture_next(&writer).await.is_err());
    assert!(writer.is_sealed());
    assert_eq!(
        fixture.observed.lock().unwrap().as_ref().unwrap().from_slot,
        Some(42)
    );
    writer.close().await.unwrap();
    let wal = directory.open();
    assert_eq!(wal.len(), 1);
    assert_eq!(wal.read(1).unwrap().frame.protobuf, vec![0, 255, 170]);
    drop(wal);
    drop(stream);
    fixture.finish().await;
}

#[tokio::test(flavor = "current_thread")]
async fn cancelled_network_capture_seals_the_writer_and_never_admits_another_frame() {
    let fixture = Fixture::start().await;
    let directory = Temp::new();
    let writer = Arc::new(StreamWalWriter::open(directory.open(), 1000).unwrap());
    let mut stream = fixture.connect(None).await;
    let capturing = writer.clone();
    let (started, entered) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let _ = started.send(());
        stream.capture_next(&capturing).await
    });
    entered.await.unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(writer.is_sealed());
    writer.close().await.unwrap();
    assert!(directory.open().is_empty());
    fixture.finish().await;
}

#[tokio::test(flavor = "current_thread")]
async fn close_interrupts_a_pending_network_capture() {
    let fixture = Fixture::start().await;
    let directory = Temp::new();
    let writer = Arc::new(StreamWalWriter::open(directory.open(), 1000).unwrap());
    let mut stream = fixture.connect(None).await;
    let capturing = writer.clone();
    let (started, entered) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let _ = started.send(());
        stream.capture_next(&capturing).await
    });
    entered.await.unwrap();
    writer.close().await.unwrap();
    let result = tokio::time::timeout(Duration::from_millis(100), task)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.unwrap_err().0, "STREAM_WAL_WRITER_SEALED");
    assert!(directory.open().is_empty());
    fixture.finish().await;
}

#[tokio::test(flavor = "current_thread")]
async fn a_sealed_writer_is_rejected_before_polling_the_network() {
    let fixture = Fixture::start().await;
    let directory = Temp::new();
    let writer = StreamWalWriter::open(directory.open(), 1000).unwrap();
    let mut stream = fixture.connect(None).await;
    writer.close().await.unwrap();
    let result = tokio::time::timeout(Duration::from_millis(100), stream.capture_next(&writer))
        .await
        .unwrap();
    assert_eq!(result.unwrap_err().0, "STREAM_WAL_WRITER_SEALED");
    drop(stream);
    fixture.finish().await;
}

#[tokio::test(flavor = "current_thread")]
async fn an_idle_network_deadline_seals_the_writer() {
    let mut fixture = Fixture::start().await;
    fixture.config.idle_timeout_ms = 10;
    let directory = Temp::new();
    let writer = StreamWalWriter::open(directory.open(), 1000).unwrap();
    let mut stream = fixture.connect(None).await;
    assert_eq!(
        stream.capture_next(&writer).await.unwrap_err().0,
        "YELLOWSTONE_IDLE_DEADLINE"
    );
    assert!(writer.is_sealed());
    writer.close().await.unwrap();
    drop(stream);
    fixture.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_late_ready_network_frame_cannot_bypass_the_idle_deadline() {
    use std::task::{Wake, Waker};
    struct FrameWake(tokio::sync::watch::Sender<u64>);
    impl Wake for FrameWake {
        fn wake(self: Arc<Self>) {
            self.0.send_modify(|count| *count += 1);
        }
        fn wake_by_ref(self: &Arc<Self>) {
            self.0.send_modify(|count| *count += 1);
        }
    }

    let mut fixture = Fixture::start().await;
    fixture.config.idle_timeout_ms = 25;
    let directory = Temp::new();
    let writer = StreamWalWriter::open(directory.open(), 1000).unwrap();
    let mut stream = fixture.connect(None).await;
    let mut capture = Box::pin(stream.capture_next(&writer));
    let (wake, mut woken) = tokio::sync::watch::channel(0);
    let waker = Waker::from(Arc::new(FrameWake(wake)));
    {
        let mut context = Context::from_waker(&waker);
        assert!(capture.as_mut().poll(&mut context).is_pending());
    }

    // Hold only the capture future unpolled. The loopback server and client I/O
    // keep running, first expiring the idle timer and then receiving a late frame.
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert!(
        *woken.borrow() > 0,
        "idle timer did not wake the pending capture"
    );
    let before_frame = *woken.borrow_and_update();
    fixture
        .frames
        .as_ref()
        .unwrap()
        .send(Ok(vec![0x32, 0x00]))
        .await
        .unwrap();
    tokio::time::timeout(
        Duration::from_secs(2),
        woken.wait_for(|count| *count > before_frame),
    )
    .await
    .unwrap()
    .unwrap();

    // Both the expired timer and raw network data are ready on this next poll.
    let result = capture.await;
    let sealed_before_close = writer.is_sealed();
    writer.close().await.unwrap();
    let wal = directory.open();
    let records = wal.len();
    drop(wal);
    drop(stream);
    fixture.finish().await;

    assert!(
        result.is_err(),
        "late frame admitted: {result:?}; wal_records={records}; sealed_before_close={sealed_before_close}"
    );
    assert_eq!(result.unwrap_err().0, "YELLOWSTONE_IDLE_DEADLINE");
    assert!(sealed_before_close);
    assert_eq!(
        records, 0,
        "expired network frames must not reach WAL capture"
    );
}
