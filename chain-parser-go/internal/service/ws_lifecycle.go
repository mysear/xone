package service

import (
	"context"
	"errors"
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
		limits := rpc.WSLimits{
			DialTimeout: time.Duration(ws.DialTimeoutMS) * time.Millisecond, IdleTimeout: time.Duration(ws.IdleTimeoutMS) * time.Millisecond,
			SessionTimeout: time.Duration(ws.SessionTimeoutMS) * time.Millisecond, MaxMessageBytes: ws.MaxMessageBytes,
			Queue: ws.Queue, MaxMessages: ws.MaxMessages, MaxConnections: ws.MaxConnections,
			ReconnectBackoff: time.Duration(ws.ReconnectBackoffMS) * time.Millisecond, MaxBackoff: time.Duration(ws.MaxBackoffMS) * time.Millisecond,
		}
		stream, e := rpc.WatchHeads(ctx, ws.URL, limits)
		if e != nil {
			cancel()
			return e
		}
		s.wsWG.Add(1)
		go s.followWebsocketSessions(ctx, chain.Key, ws.URL, limits, stream)
	}
	return nil
}

func (s *Service) followWebsocketSessions(ctx context.Context, chain, endpoint string, limits rpc.WSLimits, stream *rpc.HeadStream) {
	defer s.wsWG.Done()
	cooldown := max(limits.MaxBackoff, time.Duration(s.cfg.Config.Journal.PollMS)*time.Millisecond)
	for {
		for range stream.Hints {
			select {
			case s.wake <- struct{}{}:
			default:
			}
		}
		err := <-stream.Done
		if ctx.Err() != nil {
			return
		}
		// A fresh budget is available after cooldown. Protocol, message size
		// and queue failures require a healthy configuration before reopening.
		renew := errors.Is(err, context.DeadlineExceeded) || errors.Is(err, rpc.ErrWSMessageCount) || errors.Is(err, rpc.ErrWSReconnectBudget)
		s.log.Event("warn", "rpc_ws_budget_ended", map[string]any{
			"chain_key": chain, "provider_id": stream.ProviderID, "reason": err.Error(),
			"http_polling_continues": true, "renew_session": renew,
		})
		if !renew {
			return
		}
		timer := time.NewTimer(cooldown)
		select {
		case <-ctx.Done():
			timer.Stop()
			return
		case <-timer.C:
		}
		stream, err = rpc.WatchHeads(ctx, endpoint, limits)
		if err != nil {
			return
		}
	}
}
