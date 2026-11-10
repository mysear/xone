//! Bounded raw Yellowstone gRPC transport. Raw frames reach the durable WAL
//! before protobuf interpretation. Transport connectivity grants no readiness.
use crate::{
    async_frame_wal::StreamWalWriter,
    stream::{CapturedFrame, RawFrame, SourceSession},
};
use bytes::{Buf, BufMut};
use prost::Message;
use std::{collections::HashMap, time::Duration};
use tonic::{
    codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder},
    transport::{Channel, ClientTlsConfig, Endpoint},
    Status,
};
use xone_domain::{err, Result};

#[derive(Clone, PartialEq, Message)]
pub struct BlockFilter {
    #[prost(string, repeated, tag = "1")]
    pub account_include: Vec<String>,
    #[prost(bool, optional, tag = "2")]
    pub include_transactions: Option<bool>,
    #[prost(bool, optional, tag = "3")]
    pub include_accounts: Option<bool>,
    #[prost(bool, optional, tag = "4")]
    pub include_entries: Option<bool>,
}
#[derive(Clone, PartialEq, Message)]
pub struct SlotFilter {
    #[prost(bool, optional, tag = "1")]
    pub filter_by_commitment: Option<bool>,
    #[prost(bool, optional, tag = "2")]
    pub interslot_updates: Option<bool>,
}
#[derive(Clone, PartialEq, Message)]
pub struct SubscribeRequest {
    #[prost(map = "string, message", tag = "2")]
    pub slots: HashMap<String, SlotFilter>,
    #[prost(map = "string, message", tag = "4")]
    pub blocks: HashMap<String, BlockFilter>,
    #[prost(int32, optional, tag = "6")]
    pub commitment: Option<i32>,
    #[prost(uint64, optional, tag = "11")]
    pub from_slot: Option<u64>,
}
#[derive(Clone, Default)]
pub struct RawCodec;
pub struct RawEncoder;
pub struct RawDecoder;
impl Codec for RawCodec {
    type Encode = Vec<u8>;
    type Decode = Vec<u8>;
    type Encoder = RawEncoder;
    type Decoder = RawDecoder;
    fn encoder(&mut self) -> RawEncoder {
        RawEncoder
    }
    fn decoder(&mut self) -> RawDecoder {
        RawDecoder
    }
}
impl Encoder for RawEncoder {
    type Item = Vec<u8>;
    type Error = Status;
    fn encode(
        &mut self,
        item: Self::Item,
        dst: &mut EncodeBuf<'_>,
    ) -> std::result::Result<(), Status> {
        dst.put_slice(&item);
        Ok(())
    }
}
impl Decoder for RawDecoder {
    type Item = Vec<u8>;
    type Error = Status;
    fn decode(&mut self, src: &mut DecodeBuf<'_>) -> std::result::Result<Option<Vec<u8>>, Status> {
        Ok(Some(src.copy_to_bytes(src.remaining()).to_vec()))
    }
}
#[derive(Clone, Debug)]
pub struct TransportConfig {
    pub endpoint: String,
    pub source_id: String,
    pub connect_timeout_ms: u64,
    pub idle_timeout_ms: u64,
    pub max_frame_bytes: usize,
    pub allow_loopback_plaintext: bool,
}
impl TransportConfig {
    pub fn validate(&self) -> Result<()> {
        let uri: tonic::codegen::http::Uri = self
            .endpoint
            .parse()
            .map_err(|_| err("YELLOWSTONE_ENDPOINT"))?;
        let host = uri.host().ok_or_else(|| err("YELLOWSTONE_ENDPOINT"))?;
        let secure = uri.scheme_str() == Some("https");
        let loopback = matches!(host, "127.0.0.1" | "localhost" | "[::1]" | "::1");
        if !(secure
            || self.allow_loopback_plaintext && loopback && uri.scheme_str() == Some("http"))
            || self.endpoint.contains('@')
            || uri.path_and_query().is_some_and(|p| p.as_str() != "/")
            || self.source_id.is_empty()
            || self.source_id.len() > 128
            || self.connect_timeout_ms == 0
            || self.idle_timeout_ms == 0
            || !(1..=32 * 1024 * 1024).contains(&self.max_frame_bytes)
        {
            return Err(err("YELLOWSTONE_TRANSPORT_CONFIG"));
        }
        Ok(())
    }
}
pub struct Subscription {
    stream: tonic::Streaming<Vec<u8>>,
    _sender: tokio::sync::mpsc::Sender<Vec<u8>>,
    source: SourceSession,
    sequence: u64,
    idle: Duration,
    max_frame_bytes: usize,
}
impl Subscription {
    pub async fn connect(
        config: TransportConfig,
        session_id: String,
        auth_token: Option<&str>,
        from_slot: Option<u64>,
    ) -> Result<Self> {
        Self::connect_with_slot_filter(config, session_id, auth_token, from_slot, false).await
    }
    /// The normal finalized owner needs only finalized slot barriers. Intermediate
    /// CreatedBank notifications cannot require a not-yet-complete bank manifest.
    pub async fn connect_finalized(
        config: TransportConfig,
        session_id: String,
        auth_token: Option<&str>,
        from_slot: Option<u64>,
    ) -> Result<Self> {
        Self::connect_with_slot_filter(config, session_id, auth_token, from_slot, true).await
    }
    async fn connect_with_slot_filter(
        config: TransportConfig,
        session_id: String,
        auth_token: Option<&str>,
        from_slot: Option<u64>,
        finalized_only: bool,
    ) -> Result<Self> {
        config.validate()?;
        if session_id.is_empty() || session_id.len() > 128 {
            return Err(err("YELLOWSTONE_SESSION"));
        }
        let mut endpoint = Endpoint::from_shared(config.endpoint.clone())
            .map_err(|_| err("YELLOWSTONE_ENDPOINT"))?
            .connect_timeout(Duration::from_millis(config.connect_timeout_ms));
        if config.endpoint.starts_with("https://") {
            endpoint = endpoint
                .tls_config(ClientTlsConfig::new().with_webpki_roots())
                .map_err(|_| err("YELLOWSTONE_TLS"))?;
        }
        let channel = tokio::time::timeout(
            Duration::from_millis(config.connect_timeout_ms),
            endpoint.connect(),
        )
        .await
        .map_err(|_| err("YELLOWSTONE_CONNECT_DEADLINE"))?
        .map_err(|_| err("YELLOWSTONE_CONNECT"))?;
        Self::subscribe(
            channel,
            config,
            session_id,
            auth_token,
            from_slot,
            finalized_only,
        )
        .await
    }
    async fn subscribe(
        channel: Channel,
        config: TransportConfig,
        session_id: String,
        auth_token: Option<&str>,
        from_slot: Option<u64>,
        finalized_only: bool,
    ) -> Result<Self> {
        let request = SubscribeRequest {
            slots: HashMap::from([(
                "scope".into(),
                SlotFilter {
                    filter_by_commitment: Some(finalized_only),
                    interslot_updates: Some(!finalized_only),
                },
            )]),
            // No account filter: only an unfiltered complete block may be used to
            // compare advertised transaction/account counts. Never reinterpret a
            // filtered stream's counts as full-bank completeness.
            blocks: HashMap::from([(
                "full-block".into(),
                BlockFilter {
                    account_include: vec![],
                    include_transactions: Some(true),
                    include_accounts: Some(true),
                    include_entries: Some(true),
                },
            )]),
            commitment: Some(2),
            from_slot,
        };
        let (sender, receiver) = tokio::sync::mpsc::channel(2);
        sender
            .send(request.encode_to_vec())
            .await
            .map_err(|_| err("YELLOWSTONE_REQUEST"))?;
        let mut req = tonic::Request::new(tokio_stream::wrappers::ReceiverStream::new(receiver));
        req.metadata_mut().insert(
            "x-xone-session-id",
            session_id.parse().map_err(|_| err("YELLOWSTONE_SESSION"))?,
        );
        if let Some(token) = auth_token {
            req.metadata_mut().insert(
                "x-token",
                token.parse().map_err(|_| err("YELLOWSTONE_AUTH_FORMAT"))?,
            );
        }
        let mut client = tonic::client::Grpc::new(channel)
            .max_decoding_message_size(config.max_frame_bytes)
            .max_encoding_message_size(65536);
        client
            .ready()
            .await
            .map_err(|_| err("YELLOWSTONE_NOT_READY"))?;
        let path = tonic::codegen::http::uri::PathAndQuery::from_static("/geyser.Geyser/Subscribe");
        let response = tokio::time::timeout(
            Duration::from_millis(config.connect_timeout_ms),
            client.streaming(req, path, RawCodec),
        )
        .await
        .map_err(|_| err("YELLOWSTONE_SUBSCRIBE_DEADLINE"))?
        .map_err(|_| err("YELLOWSTONE_SUBSCRIBE"))?;
        Ok(Self {
            stream: response.into_inner(),
            _sender: sender,
            source: SourceSession {
                source_id: config.source_id,
                session_id,
            },
            sequence: 0,
            idle: Duration::from_millis(config.idle_timeout_ms),
            max_frame_bytes: config.max_frame_bytes,
        })
    }
    /// Respond only after the strict projection of this durable receipt. The
    /// pinned upstream SubscribeRequest.ping is tag 9 (id=1); no filter changes.
    pub fn respond_to_ping(&self, receipt: &CapturedFrame) -> Result<()> {
        #[derive(Clone, PartialEq, Message)]
        struct Empty {}
        #[derive(Clone, PartialEq, Message)]
        struct Probe {
            #[prost(message, optional, tag = "6")]
            ping: Option<Empty>,
        }
        let update = Probe::decode(receipt.frame.protobuf.as_slice())
            .map_err(|_| err("YELLOWSTONE_PING_FORMAT"))?;
        if update.ping.is_some() {
            self._sender
                .try_send(vec![0x4a, 0x02, 0x08, 0x01])
                .map_err(|_| err("YELLOWSTONE_PING_BACKPRESSURE"))?;
        }
        Ok(())
    }
    /// Reserve before receiving. Once reserved, errors/cancellation seal the writer;
    /// callers must invalidate readiness and establish a fresh source session.
    pub async fn capture_next(&mut self, writer: &StreamWalWriter) -> Result<CapturedFrame> {
        let reservation = writer.reserve()?;
        let idle_deadline = tokio::time::Instant::now() + self.idle;
        let raw = tokio::select! {
            biased;
            _ = reservation.cancelled() => return Err(err("STREAM_WAL_WRITER_SEALED")),
            result = tokio::time::timeout_at(idle_deadline, self.stream.message()) => result
                .map_err(|_| err("YELLOWSTONE_IDLE_DEADLINE"))?
                .map_err(|_| err("YELLOWSTONE_STREAM"))?
                .ok_or_else(|| err("YELLOWSTONE_DISCONNECTED"))?,
        };
        // timeout polls its inner future first. A late ready frame must still
        // fail the original receive deadline before sequence or WAL admission.
        if tokio::time::Instant::now() >= idle_deadline {
            return Err(err("YELLOWSTONE_IDLE_DEADLINE"));
        }
        if raw.len() > self.max_frame_bytes {
            return Err(err("YELLOWSTONE_FRAME_LIMIT"));
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| err("YELLOWSTONE_SEQUENCE_OVERFLOW"))?;
        let frame = RawFrame {
            source: self.source.clone(),
            sequence: self.sequence,
            receive_timestamp_ms: xone_observability::now_ms(),
            protobuf: raw,
        };
        reservation.capture(frame).await
    }
}
