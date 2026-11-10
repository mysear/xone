package app

import (
	"context"
	"fmt"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"
	"xone/chain-parser-go/internal/analysis"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/observability"
	"xone/chain-parser-go/internal/service"
)

// Run starts the service with RPC adapters unless a caller supplies a source.
func Run(cfg *config.Snapshot, source service.SnapshotSource) error {
	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	if cfg.Config.Service.Role == "analytics" {
		return analyticsJob(ctx, cfg)
	}
	s, e := service.NewWithSource(cfg, source)
	if e != nil {
		return e
	}
	defer s.Close()
	if e = s.Start(ctx); e != nil {
		return e
	}
	server := &http.Server{Addr: cfg.Config.API.Listen, Handler: s.Handler(), ReadHeaderTimeout: time.Duration(cfg.Config.Journal.TimeoutMS) * time.Millisecond, ReadTimeout: 5 * time.Second, WriteTimeout: 5 * time.Second, MaxHeaderBytes: 16384}
	fail := make(chan error, 1)
	go func() { fail <- server.ListenAndServe() }()
	ticker := time.NewTicker(time.Duration(cfg.Config.Journal.PollMS) * time.Millisecond)
	defer ticker.Stop()
	for {
		select {
		case <-ctx.Done():
			shutdown, cancel := context.WithTimeout(context.Background(), 5*time.Second)
			defer cancel()
			return server.Shutdown(shutdown)
		case e := <-fail:
			if e != http.ErrServerClosed {
				return fmt.Errorf("HTTP_LISTENER_FAILED")
			}
			return nil
		case <-ticker.C:
			s.Tick(ctx)
		case <-s.Wake():
			s.Tick(ctx)
		}
	}
}
func analyticsJob(ctx context.Context, cfg *config.Snapshot) error {
	if cfg.Config.Service.Mode != "journal" {
		return fmt.Errorf("ANALYTICS_JOURNAL_REQUIRED")
	}
	logger := observability.New(os.Stdout, "chain-parser-go", "analytics", cfg.Release, cfg.PublicHash, cfg.Config.Logging)
	defer logger.Close()
	if e := analysis.RunJournalJob(ctx, cfg); e != nil {
		return e
	}
	logger.Event("info", "analytics_journal_checkpoint_published", map[string]any{"plugin_version": "candles/1"})
	return nil
}
