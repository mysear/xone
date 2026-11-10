package service

import (
	"context"
	"time"
	"xone/chain-parser-go/internal/rpc"
)

func (s *Service) Wake() <-chan struct{} { return s.wake }
func (s *Service) startWebsockets(ctx context.Context) error {
	ctx, cancel := context.WithCancel(ctx)
	s.wsCancel = cancel
	for _, chain := range s.cfg.Config.Chains {
		ws := chain.Websocket
		if ws == nil {
			continue
		}
		stream, e := rpc.WatchHeads(ctx, ws.URL, rpc.WSLimits{DialTimeout: time.Duration(ws.DialTimeoutMS) * time.Millisecond, IdleTimeout: time.Duration(ws.IdleTimeoutMS) * time.Millisecond, SessionTimeout: time.Duration(ws.SessionTimeoutMS) * time.Millisecond, MaxMessageBytes: ws.MaxMessageBytes, Queue: ws.Queue, MaxMessages: ws.MaxMessages, MaxConnections: ws.MaxConnections, ReconnectBackoff: time.Duration(ws.ReconnectBackoffMS) * time.Millisecond, MaxBackoff: time.Duration(ws.MaxBackoffMS) * time.Millisecond})
		if e != nil {
			cancel()
			return e
		}
		key := chain.Key
		s.wsWG.Add(1)
		go func() {
			defer s.wsWG.Done()
			for {
				select {
				case _, ok := <-stream.Hints:
					if !ok {
						return
					}
					select {
					case s.wake <- struct{}{}:
					default:
					}
				case <-stream.Done:
					s.log.Event("warn", "rpc_ws_budget_ended", map[string]any{"chain_key": key, "provider_id": stream.ProviderID, "http_polling_continues": true})
					return
				case <-ctx.Done():
					return
				}
			}
		}()
	}
	return nil
}
