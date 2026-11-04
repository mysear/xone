package analysis

import (
	"context"
	"errors"
	"path/filepath"
	"time"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/journal"
)

// RunJournalJob is a finite, independently restartable role of the parser
// binary. It consumes public committed facts and never opens online KV or PG.
func RunJournalJob(ctx context.Context, cfg *config.Snapshot) error {
	if cfg.Config.Service.Role != "analytics" || cfg.Config.Service.Mode != "journal" || cfg.Config.Journal.Transport != "kafka" || cfg.Config.Control.Backend != "disabled" {
		return errors.New("ANALYTICS_JOURNAL_CONFIG")
	}
	for _, chain := range cfg.Config.Chains {
		interval := chain.AnalysisIntervalMS
		if interval == 0 || interval > 86400000 {
			return errors.New("ANALYTICS_JOURNAL_DEFINITION")
		}
		path := filepath.Join(cfg.Resolve(cfg.Config.State.Path), "analytics", chain.Key+".journal.json")
		state, e := LoadJournalState(path, cfg.Config.Journal.MaxBytes, chain.Key, chain.Owner, chain.Assignment, interval)
		if e != nil {
			return e
		}
		readCtx, cancel := context.WithTimeout(ctx, time.Duration(cfg.Config.Journal.TimeoutMS)*time.Millisecond)
		reader, e := journal.OpenReadOnly(readCtx, cfg.Config.Journal.Kafka.Brokers, cfg.Config.Journal.Kafka.Topics[chain.Key], cfg.Config.Journal.MaxBytes, cfg.Config.Journal.Kafka.MaxTailRecords)
		if e == nil {
			if state.TopicID != "" && state.TopicID != reader.TopicID() {
				e = errors.New("ANALYTICS_TOPIC_RECREATED")
			} else {
				state.TopicID = reader.TopicID()
				cursor := state.Cursor
				if cursor.Topic == "" {
					cursor = journal.Cursor{Topic: cfg.Config.Journal.Kafka.Topics[chain.Key]}
				}
				e = reader.Consume(readCtx, cursor, func(unit journal.Unit, at journal.Cursor) error {
					return state.ApplyAndSave(unit, at, path, cfg.Config.Journal.MaxBytes)
				})
			}
			reader.Close()
		}
		cancel()
		if e != nil {
			state.Ready = false
			state.SuspendedReason = e.Error()
			if saveError := state.Save(path, cfg.Config.Journal.MaxBytes); saveError != nil {
				return errors.New("ANALYTICS_FAILED_CHECKPOINT")
			}
			return e
		}
		state.ConfirmReadComplete()
		if state.Reference == nil {
			return errors.New("ANALYTICS_NO_COMMITTED_BASELINE")
		}
		if e = state.Save(path, cfg.Config.Journal.MaxBytes); e != nil {
			return e
		}
	}
	return nil
}
