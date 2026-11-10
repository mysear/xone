package service

// Normal-service integration (main-owned files, intentionally not edited here):
//
//  1. Add Service.rawMaintenance *RawMaintenance and
//     Journal.RawMaintenance *config.RawMaintenanceConfig
//     `json:"raw_maintenance,omitempty"`; call its nil-safe Validate(RawEvidence).
//  2. NewWithSource: after rawSources/adapters/initRuntimeSources, construct
//     s.rawMaintenance, err = NewRawMaintenance(s, cfg.Config.Journal.RawMaintenance).
//     Treat construction errors as normal New errors and Close partial state.
//     The constructor binds each existing recorder to this component and opens
//     a separately quota-locked, bounded file sink per chain.
//  3. Start: after the first owner scope and runtime-source admission, call
//     s.rawMaintenance.Start(ctx). This owns the periodic maintenance goroutine;
//     do NOT also run Tick synchronously inside s.syncMu. Explicit Tick(ctx) is
//     provided for a separately scheduled lifecycle hook or deterministic runs.
//  4. Preserve staging.go's two-block Pipeline. For each window, before creating
//     Pipeline.Fetch, wrap its local reader with
//     m.ProtectReader(ch.Key, base.Reference, reader). Keep original OnComplete.
//     This records complete stagedState bytes with acquisition generation/scope
//     provenance, and pins every response in that same height/hash capture group.
//     Fetch's existing Archive.Put is idempotent for the exact staged bytes.
//     Each wrapped ReadComplete registers cancellation/wait ownership with
//     maintenance; Close cancels both Pipeline readers before waiting. Finishing
//     a read seals its capture and never cleans up unknown or uncommitted pins.
//  5. In the existing ordered Commit closure, after forming the normal unit,
//     call m.PublishStaged(ctx, unit, selection), then advance current only on nil.
//     Or call OwnerRecordsForUnit(unit), pass its records and runtimeScopeGate to
//     w.publishOwnerRecords / Store.ApplyLocalOwnerRecords, then call
//     AcknowledgeOwnerRecords(unit, records). Owner payloads contain bounded
//     group-manifest references/digests/counts, never a per-RPC pin array. Capture
//     MaxPinsPerCapture is explicitly configurable up to 65,536, subject to raw
//     metadata byte and timeout budgets; bulk-auto remains the reader's policy.
//     Acknowledge transfers the actual immutable reducer proof to archive ledger,
//     durably releases its group pins, then CAS-retires the normal owner record.
//     Do not use bare publish/ApplyLocal
//     when raw maintenance is enabled. All owner records share the prepared/apply
//     unit/state/journal/cursor batch. Failed/unknown results keep durable pins.
//  6. Mount m.Handler() under /internal/chains/{chain}/raw/ in Service.Handler.
//     It authenticates independently with the loaded Snapshot.Token and admits
//     only identity/version-bounded replay requests, not scripts or state payloads.
//     Status, DLQ, incident and audit pagination use configured budgets.
//     List pages obey both MaxPageRecords and the actual encoded JSON byte limit
//     MaxDecodedBytes. next_offset reflects the smaller returned prefix; a single
//     record which cannot fit returns 413 and X-Raw-Error: RAW_PAGE_RECORD_BUDGET.
//  7. Close: stop new normal work; m.Close() cancels worker and active replay,
//     waits for both, and releases sink locks BEFORE archives/reducers close.
//     Surface Status.error and sink.Failed to normal health/backpressure. If
//     Archive.FailureRecoveryRequired is set, reads and pending-owner confirmation
//     remain accessible but maintenance/old-success reuse/GC remain blocked.
//
// GET status; GET dlq?offset=N; GET incidents/{DeadLetterID}?offset=N;
// GET audit/{DeadLetterID}?offset=N; POST replay with RawReplayRequest.
// New replay: dead_letter_id, incident_id, namespace, attempt and current
// expected_generation. Old pending: pending_attempt_id ONLY; its frozen Spec is
// recovered internally and Confirm reads the immutable reducer owner record.
//
// MaxRawBytes bounds each persisted encoded payload before admission, including
// base64 HTTP envelopes and acquisition/group/owner-ledger metadata. It is the
// same limit readEvidence uses after restart; the archive's larger record quota
// does not override it. Oversized metadata fails closed while already captured
// bounded source inputs retain their pins for explicit owner/recovery handling.
//
// Only staged-state/public-adapter-v1 is executable. HTTP observations cannot
// reconstruct a scope. Old staged data without matching acquisition-generation
// provenance is history-only; old/removed adapter versions or unadmitted pools
// fail closed. History uses no online commit/confirm/catalog callback and cannot
// discharge online acquisition pins or online DLQ.
//
// Every maintenance-enabled HTTP read needs an explicit responsibility:
//   - VerifyEVM / ProviderRuntime.Refresh / VerifyAnchor / ordinary Header use
//     Observe(ctx, chain, RawObserveIdentity/Probe/Anchor/Header, operation).
//     ProviderRuntime.SetObserver can map its probe/anchor purpose to these names.
//     Only successful, uncancelled completion writes an observation ticket. Tick
//     archives and verifies the exact inputs before releasing that ticket's pins.
//     It remains raw-observation-only; it is not a canonical or owner proof.
//   - Baseline RPCs use BeginBaselineCapture; BaselineOwnerRecords binds the unit
//     plus actual ScopeJob/fence. Publish with PrepareScopeRecordsCAS or local
//     InstallBaselineScopeRecordsCAS, then AcknowledgeOwnerRecords. Empty-pool D0
//     instead uses Observe(ctx, chain, RawObserveChainBaseline, operation): this
//     validates identity/header and hands off weak evidence without installing a
//     Snapshot/ScopeJob or declaring scope ready. BaselineOwnerRecords explicitly
//     rejects empty pool state as RAW_D0_OBSERVATION_REQUIRED. End capture and
//     restore the original context before
//     catalog/history sidecars, since publication seals the baseline responsibility.
//   - An entire reorg uses BeginOwnerCapture(ctx, chain, "reorg"); final
//     OwnerUnitRecords(ctx, REORG_COMMIT) binds all FactBlocks and the full unit.
//     Publish through same-batch owner records then AcknowledgeOwnerRecords.
//     Reducer PrepareOwnerRecords must permit REORG_COMMIT while retaining normal
//     StageReorg validation. REORG_BEGIN / ONLINE_SCOPE_REVOKED have no data record.
//
// Nested Observe inherits the outer baseline/fact/reorg/history responsibility;
// it cannot issue a weaker ticket to release those inputs. Failed/unknown reads
// remain pinned across restart. An ungrouped read with maintenance returns
// RAW_CAPTURE_RESPONSIBILITY_REQUIRED before writing; legacy nil maintenance
// retains ordinary Put behavior and acquires no orphan pins. The management API
// token is project-owned internal authentication, not a source supplier account.
//
// The local file sink supplies copy/fsync/content+manifest verification and hard
// logical byte/file limits (including orphans), not WORM, encryption or independent
// disaster recovery. Reducer owner records and Archive audit also have finite
// capacity. Exhaustion stops writes; no archive deletion or invented proof frees
// space. Production Kafka/PG lease/outage acceptance is separate from local
// controlled public-RPC tests. Operator recovery never auto-clears an unclean
// failure-session guard whose first failure marker may have been lost.
