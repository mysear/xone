package rpc

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"errors"
	"math/rand/v2"
	"net"
	"net/url"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/gorilla/websocket"
)

var (
	ErrWSProtocol        = errors.New("RPC_WS_PROTOCOL")
	ErrWSMessageBudget   = errors.New("RPC_WS_MESSAGE_BUDGET")
	ErrWSQueueBudget     = errors.New("RPC_WS_QUEUE_BUDGET")
	ErrWSReconnectBudget = errors.New("RPC_WS_RECONNECT_BUDGET")
	errWSIdle            = errors.New("idle")
	errWSDisconnected    = errors.New("disconnected")
)

type WSLimits struct {
	DialTimeout      time.Duration
	IdleTimeout      time.Duration
	SessionTimeout   time.Duration
	MaxMessageBytes  int64
	Queue            int
	MaxMessages      uint64
	MaxConnections   int
	ReconnectBackoff time.Duration
	MaxBackoff       time.Duration
}
type WSSignalKind string

const (
	WSConnected     WSSignalKind = "connected"
	WSHeadAvailable WSSignalKind = "head_available"
)

// HeadSignal is an UNTRUSTED wakeup hint, including its height/hash fields.
// It never represents a verified header, canonical cursor, or finality claim.
type HeadSignal struct {
	Kind       WSSignalKind
	ProviderID string
	Connection int
	Height     uint64
	Hash       string
	ParentHash string
}
type WSObservation struct {
	ProviderID  string
	Connections int
	Messages    uint64
	LastCause   string
}
type HeadStream struct {
	ProviderID  string
	Hints       <-chan HeadSignal
	Done        <-chan error
	mu          sync.Mutex
	observation WSObservation
}

func (s *HeadStream) Snapshot() WSObservation { s.mu.Lock(); defer s.mu.Unlock(); return s.observation }

// WatchHeads owns a bounded raw WebSocket subscription and closes Hints/Done on
// exit. Reconnection attempts never reset after success. Caller must consume
// Hints and Done and explicitly start another budgeted session when appropriate.
// There is no client-library subscription buffer or automatic transport retry.
func WatchHeads(ctx context.Context, endpoint string, limits WSLimits) (*HeadStream, error) {
	if limits.DialTimeout <= 0 || limits.IdleTimeout <= 0 || limits.SessionTimeout <= 0 || limits.MaxMessageBytes <= 0 || limits.Queue <= 0 || limits.Queue > 65536 || limits.MaxMessages == 0 || limits.MaxConnections <= 0 || limits.MaxConnections > 64 || limits.ReconnectBackoff <= 0 || limits.MaxBackoff < limits.ReconnectBackoff {
		return nil, errors.New("RPC_WS_LIMITS_INVALID")
	}
	parsed, err := url.Parse(endpoint)
	if err != nil || parsed.Host == "" || (parsed.Scheme != "ws" && parsed.Scheme != "wss") {
		return nil, errors.New("RPC_WS_URL_INVALID")
	}
	if ctx.Err() != nil {
		return nil, ctx.Err()
	}
	hints := make(chan HeadSignal, limits.Queue)
	done := make(chan error, 1)
	s := &HeadStream{ProviderID: endpointID(endpoint), Hints: hints, Done: done}
	s.observation.ProviderID = s.ProviderID
	ctx, cancel := context.WithTimeout(ctx, limits.SessionTimeout)
	go func() {
		defer cancel()
		defer close(done)
		defer close(hints)
		done <- s.run(ctx, endpoint, limits, hints)
	}()
	return s, nil
}
func (s *HeadStream) run(ctx context.Context, endpoint string, l WSLimits, hints chan<- HeadSignal) error {
	for connection := 1; connection <= l.MaxConnections; connection++ {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		s.mu.Lock()
		s.observation.Connections = connection
		s.mu.Unlock()
		err := s.connection(ctx, endpoint, l, connection, hints)
		if ctx.Err() != nil {
			return ctx.Err()
		}
		if errors.Is(err, ErrWSProtocol) || errors.Is(err, ErrWSMessageBudget) || errors.Is(err, ErrWSQueueBudget) {
			return err
		}
		s.mu.Lock()
		s.observation.LastCause = err.Error()
		s.mu.Unlock()
		if connection == l.MaxConnections {
			return ErrWSReconnectBudget
		}
		delay := l.ReconnectBackoff
		for i := 1; i < connection && delay < l.MaxBackoff; i++ {
			if delay > l.MaxBackoff/2 {
				delay = l.MaxBackoff
			} else {
				delay *= 2
			}
		}
		// Jitter retains a real minimum delay; a flapping connection cannot reset it.
		if room := l.MaxBackoff - delay; room > 0 {
			if room > delay {
				room = delay
			}
			delay += time.Duration(rand.Int64N(int64(room) + 1))
		}
		timer := time.NewTimer(delay)
		select {
		case <-ctx.Done():
			timer.Stop()
			return ctx.Err()
		case <-timer.C:
		}
	}
	return ErrWSReconnectBudget
}
func (s *HeadStream) connection(ctx context.Context, endpoint string, l WSLimits, connection int, hints chan<- HeadSignal) error {
	dialCtx, cancel := context.WithTimeout(ctx, l.DialTimeout)
	dialer := websocket.Dialer{HandshakeTimeout: l.DialTimeout, ReadBufferSize: 1024, WriteBufferSize: 1024, EnableCompression: false}
	// Own cancellation during the TCP/TLS/Upgrade handshake as well as its
	// deadline, independently of the selected gorilla release's dial behavior.
	stopDial := func() bool { return true }
	dialer.NetDialContext = func(netCtx context.Context, network, address string) (net.Conn, error) {
		var networkDialer net.Dialer
		transport, err := networkDialer.DialContext(netCtx, network, address)
		if err == nil {
			stopDial = context.AfterFunc(dialCtx, func() { transport.Close() })
		}
		return transport, err
	}
	conn, response, err := dialer.DialContext(dialCtx, endpoint, nil)
	stopDial()
	cancel()
	if err != nil {
		if response != nil && response.Body != nil {
			response.Body.Close()
		}
		if ctx.Err() != nil {
			return ctx.Err()
		}
		if response != nil && response.StatusCode >= 400 && response.StatusCode < 500 && response.StatusCode != 408 && response.StatusCode != 429 {
			return ErrWSProtocol
		}
		return errWSDisconnected
	}
	defer conn.Close()
	stop := context.AfterFunc(ctx, func() { conn.Close() })
	defer stop()
	conn.SetReadLimit(l.MaxMessageBytes)
	if conn.SetWriteDeadline(time.Now().Add(l.DialTimeout)) != nil {
		return errWSDisconnected
	}
	if conn.WriteJSON(map[string]any{"jsonrpc": "2.0", "id": 1, "method": "eth_subscribe", "params": []string{"newHeads"}}) != nil {
		return errWSDisconnected
	}
	receive := func() ([]byte, error) {
		if conn.SetReadDeadline(time.Now().Add(l.IdleTimeout)) != nil {
			return nil, errWSDisconnected
		}
		kind, b, err := conn.ReadMessage()
		if err != nil {
			if ctx.Err() != nil {
				return nil, ctx.Err()
			}
			if errors.Is(err, websocket.ErrReadLimit) {
				return nil, ErrWSMessageBudget
			}
			var n net.Error
			if errors.As(err, &n) && n.Timeout() {
				return nil, errWSIdle
			}
			return nil, errWSDisconnected
		}
		if kind != websocket.TextMessage || int64(len(b)) > l.MaxMessageBytes {
			return nil, ErrWSProtocol
		}
		return b, nil
	}
	ack, err := receive()
	if err != nil {
		return err
	}
	var responseAck struct {
		JSONRPC string          `json:"jsonrpc"`
		ID      int             `json:"id"`
		Result  string          `json:"result"`
		Error   json.RawMessage `json:"error"`
	}
	if json.Unmarshal(ack, &responseAck) != nil || responseAck.JSONRPC != "2.0" || responseAck.ID != 1 || responseAck.Result == "" || len(responseAck.Result) > 128 || len(responseAck.Error) > 0 && string(responseAck.Error) != "null" {
		return ErrWSProtocol
	}
	emit := func(h HeadSignal) error {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		h.ProviderID = s.ProviderID
		h.Connection = connection
		select {
		case hints <- h:
			return nil
		default:
			return ErrWSQueueBudget
		}
	}
	if err := emit(HeadSignal{Kind: WSConnected}); err != nil {
		return err
	}
	for {
		b, err := receive()
		if err != nil {
			return err
		}
		s.mu.Lock()
		s.observation.Messages++
		count := s.observation.Messages
		s.mu.Unlock()
		if count > l.MaxMessages {
			return ErrWSMessageBudget
		}
		var notification struct {
			JSONRPC string `json:"jsonrpc"`
			Method  string `json:"method"`
			Params  struct {
				Subscription string `json:"subscription"`
				Result       struct {
					Number     string `json:"number"`
					Hash       string `json:"hash"`
					ParentHash string `json:"parentHash"`
				} `json:"result"`
			} `json:"params"`
		}
		if json.Unmarshal(b, &notification) != nil || notification.JSONRPC != "2.0" || notification.Method != "eth_subscription" || notification.Params.Subscription != responseAck.Result {
			return ErrWSProtocol
		}
		head := notification.Params.Result
		number, parseErr := strconv.ParseUint(strings.TrimPrefix(head.Number, "0x"), 16, 64)
		if !hexQuantity(head.Number) || parseErr != nil || !wsHash(head.Hash) || !wsHash(head.ParentHash) {
			return ErrWSProtocol
		}
		if err := emit(HeadSignal{Kind: WSHeadAvailable, Height: number, Hash: strings.ToLower(head.Hash), ParentHash: strings.ToLower(head.ParentHash)}); err != nil {
			return err
		}
	}
}
func wsHash(hash string) bool {
	if len(hash) != 66 || !strings.HasPrefix(hash, "0x") {
		return false
	}
	_, err := hex.DecodeString(hash[2:])
	return err == nil
}
