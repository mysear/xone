"""Static source/test/evidence rules for design clauses.

Completion states and scopes are read only from DEVELOPMENT_PROGRESS.md.
No progress snapshots, dates or completion flags belong in this module.
"""
BINDINGS = {'7.9.1': {'paths': ['chain-parser-go/internal/service/reorg.go', 'chain-parser-go/internal/rpc/client.go'],
           'tests': ['TestForkConfirmationRequiresDistinctAgreeingProviders',
                     'TestDeepOrUnconfirmedForkCannotPublishReady'],
           'evidence_refs': ['integration/artifacts/local/reorg-smoke.json',
                             'integration/artifacts/local/reorg-http-smoke.json',
                             'integration/artifacts/local/verification.json']},
 '7.9.2': {'paths': ['chain-parser-go/internal/reducer/reorg.go', 'chain-parser-go/internal/service/reorg.go'],
           'tests': ['TestBoundedForkPublishesOnlyBeginAndCompleteBranch',
                     'TestDeepOrUnconfirmedForkCannotPublishReady'],
           'evidence_refs': ['integration/artifacts/local/reorg-smoke.json',
                             'integration/artifacts/local/reorg-http-smoke.json',
                             'integration/artifacts/local/verification.json']},
 '7.9.3': {'paths': ['chain-parser-go/internal/journal/frame.go',
                     'chain-parser-go/internal/reducer/store.go',
                     'dex-router/crates/state-replica/src/lib.rs'],
           'tests': ['TestReorgBarrierDurableAndCommitBoundToBegin',
                     'reorg_barrier_revokes_all_handles_and_requires_exact_commit'],
           'evidence_refs': ['integration/artifacts/local/reorg-smoke.json',
                             'integration/artifacts/local/reorg-http-smoke.json',
                             'integration/artifacts/local/verification.json']},
 '7.9.4': {'paths': ['chain-parser-go/internal/reducer/store.go', 'chain-parser-go/internal/reducer/reorg.go'],
           'tests': ['TestDescendingUndoAndReplacementBranchRetention',
                     'TestAdvancingReorgPrunesUndoWithinWindow'],
           'evidence_refs': ['integration/artifacts/local/reorg-smoke.json',
                             'integration/artifacts/local/reorg-http-smoke.json',
                             'integration/artifacts/local/verification.json']},
 '7.9.5': {'paths': ['chain-parser-go/internal/service/reorg.go', 'chain-parser-go/internal/chain/evm/adapter.go'],
           'tests': ['TestBoundedForkPublishesOnlyBeginAndCompleteBranch',
                     'TestHashPinnedBaselineAndIncompleteReceipts'],
           'evidence_refs': ['integration/artifacts/local/reorg-smoke.json',
                             'integration/artifacts/local/reorg-http-smoke.json',
                             'integration/artifacts/local/verification.json']},
 '7.9.6': {'paths': ['dex-router/crates/state-replica/src/lib.rs',
                     'dex-router/crates/quote-api/src/lib.rs',
                     'chain-parser-go/internal/reducer/store.go'],
           'tests': ['TestRealJournalReorgCommitCrashRetainsBarrierAndUndo',
                     'kafka_poll_failure_between_barrier_and_commit_resumes_exact_applied_cursor'],
           'evidence_refs': ['integration/artifacts/local/reorg-smoke.json',
                             'integration/artifacts/local/reorg-http-smoke.json',
                             'integration/artifacts/local/verification.json']},
 '7.9.7': {'paths': ['chain-parser-go/internal/analysis/journal.go',
                     'chain-parser-go/internal/analysis/job.go',
                     'chain-parser-go/internal/service/reorg.go',
                     'common/protoc/proto/dex/v1/state.proto'],
           'tests': ['TestJournalReorgRecomputesOHLCAndPublishesDeletedWindow',
                     'TestSameEpochBaselineAfterBeginRetractsUnreconciledTrades',
                     'TestRealNormalizedFactPreparedWireRecovery'],
           'evidence_refs': ['integration/artifacts/local/analytics-journal-smoke.json',
                             'integration/artifacts/local/journal-integration.json']},
 '7.9.deep-reorg': {'paths': ['chain-parser-go/internal/service/service.go',
                              'chain-parser-go/internal/service/session.go'],
                    'tests': ['TestDeepOrUnconfirmedForkCannotPublishReady'],
                    'evidence_refs': ['integration/artifacts/local/reorg-smoke.json',
                                      'integration/artifacts/local/reorg-http-smoke.json',
                                      'integration/artifacts/local/verification.json']},
 '16.1': {'paths': ['chain-parser-go/internal/analysis',
                    'chain-parser-go/internal/journal',
                    'docs/TECHNICAL_REFERENCE.md'],
          'tests': ['TestSwapFactsUseFullReceiptPositionAndRawIntegerAmounts'],
          'evidence_refs': ['integration/artifacts/local/analytics-journal-smoke.json',
                            'integration/artifacts/local/journal-integration.json',
                            'integration/artifacts/local/verification.json']},
 '16.3': {'paths': ['chain-parser-go/internal/analysis',
                    'chain-parser-go/internal/journal',
                    'docs/TECHNICAL_REFERENCE.md'],
          'tests': ['TestJournalAnalyticsModeIsExplicitAndCannotOwnLease'],
          'evidence_refs': ['integration/artifacts/local/analytics-journal-smoke.json',
                            'integration/artifacts/local/journal-integration.json',
                            'integration/artifacts/local/verification.json']},
 '16.4': {'paths': ['chain-parser-go/internal/analysis',
                    'chain-parser-go/internal/journal',
                    'docs/TECHNICAL_REFERENCE.md'],
          'tests': ['TestJournalReorgRecomputesOHLCAndPublishesDeletedWindow',
                    'TestBaselineTakeoverDoesNotLoseEarlierReorgFacts'],
          'evidence_refs': ['integration/artifacts/local/analytics-journal-smoke.json',
                            'integration/artifacts/local/journal-integration.json',
                            'integration/artifacts/local/verification.json']},
 '16.8': {'paths': ['chain-parser-go/internal/analysis',
                    'chain-parser-go/internal/journal',
                    'docs/TECHNICAL_REFERENCE.md'],
          'tests': ['TestCandidateExhaustionCanPersistUnpublishableDurableRelease',
                    'TestReadOnlyAnalyticsReaderDoesNotFenceOwner'],
          'evidence_refs': ['integration/artifacts/local/analytics-journal-smoke.json',
                            'integration/artifacts/local/journal-integration.json',
                            'integration/artifacts/local/verification.json']},
 '7.7.facts-crash': {'paths': ['chain-parser-go/internal/analysis',
                               'chain-parser-go/internal/journal',
                               'docs/TECHNICAL_REFERENCE.md'],
                     'tests': ['TestRealNormalizedFactPreparedWireRecovery',
                               'TestCanonicalUnitIdentityIgnoresPrivateRepeatedFieldShape'],
                     'evidence_refs': ['integration/artifacts/local/analytics-journal-smoke.json',
                                       'integration/artifacts/local/journal-integration.json',
                                       'integration/artifacts/local/verification.json']},
 '11.3.v3': {'paths': ['common/protoc/proto/dex/v1/state.proto',
                       'chain-parser-go/internal/domain/v3.go',
                       'dex-router/crates/domain/src/v3.rs'],
             'tests': ['TestV3SnapshotProtobufKeepsSignedLiquidityAndProof',
                       'independent_protoc_v3_state_proof_and_signed_net',
                       'TestMinimumInitializationPriceIsAValidV3State'],
             'evidence_refs': ['integration/artifacts/local/v3-differential.json',
                               'integration/artifacts/local/v3-three-service-smoke.json']},
 '7.3.v3': {'paths': ['chain-parser-go/internal/chain/evm/v3.go', 'chain-parser-go/internal/chain/evm/adapter.go'],
            'tests': ['TestV3HashPinnedReadChecksCodeBitmapAndEveryTick'],
            'evidence_refs': ['integration/artifacts/local/v3-differential.json',
                              'integration/artifacts/local/v3-three-service-smoke.json']},
 '12.4.v3': {'paths': ['dex-router/crates/protocol-math/src/v3.rs', 'integration/tools/ci/v3-golden.cjs'],
             'tests': ['official_sdk_integer_differential_all_cases',
                       'v3_range_and_segment_limits_do_not_fake_full_input'],
             'evidence_refs': ['integration/artifacts/local/v3-differential.json',
                               'integration/artifacts/local/v3-three-service-smoke.json']},
 '12.10.v3': {'paths': ['dex-router/crates/routing/src/lib.rs', 'dex-router/crates/optimizer/src/lib.rs'],
              'tests': ['paths_unusable_for_whole_order_are_preserved_for_valid_split',
                        'shared_v3_sequential_allocations_match_official_sdk_oracle_in_both_orders'],
              'evidence_refs': ['integration/artifacts/local/v3-differential.json',
                                'integration/artifacts/local/v3-three-service-smoke.json']},
 '12.11.v3': {'paths': ['chain-parser-go/internal/chain/evm/v3.go',
                        'dex-router/crates/protocol-math/src/v3.rs',
                        'dex-router/crates/execution-plan/src/lib.rs',
                        'integration/tools/acceptance/v3_smoke.py'],
              'tests': ['v3_quote_cannot_be_encoded_as_v2_router_calldata',
                        'mixed_v2_v3_path_uses_v3_post_state_and_exact_output'],
              'evidence_refs': ['integration/artifacts/local/v3-differential.json',
                                'integration/artifacts/local/v3-three-service-smoke.json']},
 '4.dex-version-matrix': {'paths': ['DEVELOPMENT_PROGRESS.md'],
                          'tests': ['TestDexVersionProfilesCannotMixFeeLayoutOrIdentity'],
                          'evidence_refs': ['integration/artifacts/local/multidex-differential.json',
                                            'integration/artifacts/local/multidex-three-service-smoke.json']},
 '7.3.dex-versions': {'paths': ['chain-parser-go/internal/chain/evm/v2.go',
                                'chain-parser-go/internal/chain/evm/v3.go'],
                      'tests': ['TestVersionedV2ReadsIdentityAndFixedFee',
                                'TestV3HashPinnedReadChecksCodeBitmapAndEveryTick'],
                      'evidence_refs': ['integration/artifacts/local/multidex-differential.json',
                                        'integration/artifacts/local/multidex-three-service-smoke.json']},
 '11.3.dex-version-identity': {'paths': ['common/protoc/proto/dex/v1/state.proto',
                                         'chain-parser-go/internal/domain/protocols.go',
                                         'dex-router/crates/domain/src/protocols.rs'],
                               'tests': ['TestProtocDifferentDexVersionStatesRemainDistinct',
                                         'independent_protoc_preserves_six_dex_version_profiles',
                                         'legacy_label_cannot_bypass_physical_address_alias_guard'],
                               'evidence_refs': ['integration/artifacts/local/multidex-differential.json',
                                                 'integration/artifacts/local/multidex-three-service-smoke.json']},
 '12.2.dex-fees': {'paths': ['dex-router/crates/protocol-math/src/lib.rs',
                             'dex-router/crates/protocol-math/src/v3.rs',
                             'integration/tools/ci/multidex-golden.cjs'],
                   'tests': ['version_specific_fees_change_output_and_cannot_be_overridden',
                             'pinned_dex_sdk_and_pair_invariant_differential'],
                   'evidence_refs': ['integration/artifacts/local/multidex-differential.json',
                                     'integration/artifacts/local/multidex-three-service-smoke.json']},
 '12.11.dex-adapter-matrix': {'paths': ['DEVELOPMENT_PROGRESS.md',
                                        'integration/tools/acceptance/multidex_smoke.py'],
                              'tests': ['mixed_dex_version_path_keeps_each_adapter_and_fee'],
                              'evidence_refs': ['integration/artifacts/local/multidex-differential.json',
                                                'integration/artifacts/local/multidex-three-service-smoke.json']},
 '11.3.stable-and-slipstream-state': {'paths': ['common/protoc/proto/dex/v1/state.proto',
                                                'chain-parser-go/internal/domain/stable.go',
                                                'dex-router/crates/domain/src/stable.rs'],
                                      'tests': ['TestStableParametersSurviveSharedWire',
                                                'TestStableDomainRejectsContractImpossibleFeeRanges'],
                                      'evidence_refs': ['integration/artifacts/local/stable-differential.json',
                                                        'integration/artifacts/local/stable-three-service-smoke.json',
                                                        'integration/artifacts/local/stable-golden-generation.json']},
 '7.3.stable-and-slipstream-dependencies': {'paths': ['chain-parser-go/internal/chain/evm/stable.go',
                                                      'chain-parser-go/internal/chain/evm/v3.go',
                                                      'integration/tools/acceptance/stable_smoke.py'],
                                            'tests': ['TestStableHashPinnedModelsAndDependencyRejections',
                                                      'TestStandardCloneRequiresPinnedImplementationAndPublishesNoGuess'],
                                            'evidence_refs': ['integration/artifacts/local/stable-differential.json',
                                                              'integration/artifacts/local/stable-three-service-smoke.json',
                                                              'integration/artifacts/local/stable-golden-generation.json']},
 '12.2.stable-protocol-specific-rounding': {'paths': ['dex-router/crates/protocol-math/src/aerodrome.rs',
                                                      'dex-router/crates/protocol-math/src/curve_plain.rs',
                                                      'dex-router/crates/protocol-math/src/balancer_stable.rs'],
                                            'tests': ['executed_primary_contracts_match_amounts_and_post_balances'],
                                            'evidence_refs': ['integration/artifacts/local/stable-differential.json',
                                                              'integration/artifacts/local/stable-three-service-smoke.json',
                                                              'integration/artifacts/local/stable-golden-generation.json']},
 '12.9.stable-shared-physical-pool': {'paths': ['dex-router/crates/optimizer/tests/stable_shared.rs',
                                                'integration/tools/ci/stable-golden.cjs'],
                                      'tests': ['stable_allocations_reuse_physical_post_state_against_executed_contracts'],
                                      'evidence_refs': ['integration/artifacts/local/stable-differential.json',
                                                        'integration/artifacts/local/stable-three-service-smoke.json',
                                                        'integration/artifacts/local/stable-golden-generation.json']},
 '12.11.requested-actual-version-audit': {'paths': ['DEVELOPMENT_PROGRESS.md',
                                                    'integration/tools/reference/protocol-sources/source-metadata.json',
                                                    'chain-parser-rust/crates/solana-ingest/src/rpc.rs'],
                                          'tests': ['rejects_creator_rates_and_share_even_when_pool_fee_is_disabled',
                                                    'bank_timestamp_rejects_future_opening_even_when_wall_clock_has_passed'],
                                          'evidence_refs': ['integration/artifacts/local/stable-differential.json',
                                                            'integration/artifacts/local/stable-three-service-smoke.json',
                                                            'integration/artifacts/local/stable-golden-generation.json']},
 '11.3.singleton-key-state': {'paths': ['common/protoc/proto/dex/v1/state.proto',
                                        'chain-parser-go/internal/domain/singleton.go',
                                        'dex-router/crates/domain/src/singleton.rs'],
                              'tests': ['TestSingletonIdentityAndTypedWire',
                                        'singleton_key_and_wire_are_independently_typed'],
                              'evidence_refs': ['integration/artifacts/local/singleton-differential.json',
                                                'integration/artifacts/local/singleton-three-service-smoke.json',
                                                'integration/artifacts/local/singleton-golden-generation.json']},
 '7.3.singleton-manager-dependencies': {'paths': ['chain-parser-go/internal/chain/evm/singleton.go',
                                                  'integration/tools/acceptance/singleton_smoke.py'],
                                        'tests': ['TestSingletonCLPinnedOfficialReads',
                                                  'TestSingletonCLRejectsMalformedOrUnsafeRPCState',
                                                  'TestSingletonCLRejectsInvalidConfigurationBeforeRPC',
                                                  'TestSingletonCLBoundaryWindowsAndBudgets',
                                                  'TestIndependentSingletonProtocolWireProof'],
                                        'evidence_refs': ['integration/artifacts/local/singleton-differential.json',
                                                          'integration/artifacts/local/singleton-three-service-smoke.json',
                                                          'integration/artifacts/local/singleton-golden-generation.json']},
 '12.2.singleton-directional-fees': {'paths': ['dex-router/crates/protocol-math/src/singleton.rs'],
                                     'tests': ['actual_pinned_pool_swaps_match_singleton_amount_and_core_post_state',
                                               'TestBothSingletonCoresFloorNegativeMinimumForGrossLiquidity'],
                                     'evidence_refs': ['integration/artifacts/local/singleton-differential.json',
                                                       'integration/artifacts/local/singleton-three-service-smoke.json',
                                                       'integration/artifacts/local/singleton-golden-generation.json']},
 '12.9.singleton-shared-state': {'paths': ['dex-router/crates/optimizer/tests/singleton_shared.rs'],
                                 'tests': ['mixed_direction_shared_singleton_continuation_matches_original_pool_swap',
                                           'shared_singleton_allocations_recompute_stale_candidate_amounts',
                                           'identical_v4_key_in_different_manager_is_a_distinct_physical_pool'],
                                 'evidence_refs': ['integration/artifacts/local/singleton-differential.json',
                                                   'integration/artifacts/local/singleton-three-service-smoke.json',
                                                   'integration/artifacts/local/singleton-golden-generation.json']},
 '12.14.singleton-primary-reference': {'paths': ['integration/tools/reference/singleton-cl/README.md',
                                                 'integration/tools/ci/singleton-golden.cjs'],
                                       'tests': ['actual_pinned_pool_swaps_match_singleton_amount_and_core_post_state'],
                                       'evidence_refs': ['integration/artifacts/local/singleton-differential.json',
                                                         'integration/artifacts/local/singleton-three-service-smoke.json',
                                                         'integration/artifacts/local/singleton-golden-generation.json']},
 '12.11.singleton-execution-boundary': {'paths': ['dex-router/crates/execution-plan/tests/plan.rs'],
                                        'tests': ['singleton_quotes_cannot_be_encoded_as_v2_router_calldata'],
                                        'evidence_refs': ['integration/artifacts/local/singleton-differential.json',
                                                          'integration/artifacts/local/singleton-three-service-smoke.json',
                                                          'integration/artifacts/local/singleton-golden-generation.json']},
 '11.3.raydium-creator-state': {'paths': ['chain-parser-go/internal/domain/raydium.go',
                                          'dex-router/crates/domain/src/raydium.rs'],
                                'tests': ['TestRaydiumCreatorStateHasOwnIdentityAndWire',
                                          'TestRaydiumProofMetadataCannotMixProgramsOrFutureEpoch',
                                          'raydium_creator_metadata_and_net_reserves_are_strict'],
                                'evidence_refs': ['integration/artifacts/local/raydium-differential.json',
                                                  'integration/artifacts/local/raydium-three-service-smoke.json',
                                                  'integration/artifacts/local/raydium-golden-generation.json']},
 '7.3.raydium-finalized-account-set': {'paths': ['chain-parser-rust/crates/solana-ingest/src/raydium.rs',
                                                 'chain-parser-rust/crates/solana-ingest/src/rpc.rs'],
                                       'tests': ['native_wsol_requires_rent_and_amount_backing_but_allows_unsynced_donations',
                                                 'on_chain_fee_updates_are_authoritative_over_static_config_values'],
                                       'evidence_refs': ['integration/artifacts/local/raydium-differential.json',
                                                         'integration/artifacts/local/raydium-three-service-smoke.json',
                                                         'integration/artifacts/local/raydium-golden-generation.json']},
 '12.2.raydium-creator-fee-rounding': {'paths': ['dex-router/crates/protocol-math/src/raydium.rs'],
                                       'tests': ['actual_raydium_ceil_fee_does_not_use_v2_output',
                                                 'unchanged_raydium_program_matches_amounts_raw_ledgers_and_net_poststate',
                                                 'zero_combined_input_fee_preserves_original_none_branch'],
                                       'evidence_refs': ['integration/artifacts/local/raydium-differential.json',
                                                         'integration/artifacts/local/raydium-three-service-smoke.json',
                                                         'integration/artifacts/local/raydium-golden-generation.json']},
 '12.9.raydium-shared-fee-ledger': {'paths': ['dex-router/crates/optimizer/tests/raydium_shared.rs'],
                                    'tests': ['shared_raydium_sequences_reuse_raw_fee_ledgers_in_native_program_order'],
                                    'evidence_refs': ['integration/artifacts/local/raydium-differential.json',
                                                      'integration/artifacts/local/raydium-three-service-smoke.json',
                                                      'integration/artifacts/local/raydium-golden-generation.json']},
 '7.3.solana-owner-failure-revocation': {'paths': ['chain-parser-rust/crates/solana-ingest/src/service.rs',
                                                   'chain-parser-rust/crates/solana-ingest/src/session.rs',
                                                   'chain-parser-rust/crates/solana-ingest/src/store.rs'],
                                         'tests': ['committed_scope_revocation_is_durable_and_cannot_restore_old_readiness',
                                                   'pending_revocation_hides_locally_and_survives_restart_without_a_valid_source',
                                                   'malformed_revocation_cannot_replace_the_published_scope'],
                                         'evidence_refs': ['integration/artifacts/local/raydium-differential.json',
                                                           'integration/artifacts/local/raydium-three-service-smoke.json',
                                                           'integration/artifacts/local/raydium-golden-generation.json']},
 '12.14.raydium-primary-reference': {'paths': ['integration/tools/reference/raydium-cpmm/README.md',
                                               'integration/tools/ci/raydium-golden.py'],
                                     'tests': ['TestIndependentRaydiumCreatorProtocWire',
                                               'independent_protoc_preserves_raydium_creator_program_bank_and_fee_ledgers',
                                               'raydium_quote_cannot_be_encoded_as_evm_v2_calldata'],
                                     'evidence_refs': ['integration/artifacts/local/raydium-differential.json',
                                                       'integration/artifacts/local/raydium-three-service-smoke.json',
                                                       'integration/artifacts/local/raydium-golden-generation.json']},
 '11.3.orca-q64-array-state': {'paths': ['chain-parser-go/internal/domain/orca.go',
                                         'dex-router/crates/domain/src/orca.rs'],
                               'tests': ['TestOrcaStaticIndependentStateAndWire',
                                         'TestOrcaIndependentOriginalTickMathVectors',
                                         'independent_original_orca_tickmath_vectors'],
                               'evidence_refs': ['integration/artifacts/local/orca-differential.json',
                                                 'integration/artifacts/local/orca-three-service-smoke.json',
                                                 'integration/artifacts/local/orca-golden-generation.json']},
 '7.3.orca-finalized-static-accounts': {'paths': ['chain-parser-rust/crates/solana-ingest/src/orca.rs',
                                                  'chain-parser-rust/crates/solana-ingest/src/rpc.rs'],
                                        'tests': ['decodes_both_real_layouts_without_virtual_reserves',
                                                  'only_declared_oracle_null_can_become_virtual_system_empty',
                                                  'parses_initialized_ticks_fee_outside_and_rejects_inconsistent_gross_net',
                                                  'native_mint_requires_classic_metadata_native_flag_and_lamport_backing'],
                                        'evidence_refs': ['integration/artifacts/local/orca-differential.json',
                                                          'integration/artifacts/local/orca-three-service-smoke.json',
                                                          'integration/artifacts/local/orca-golden-generation.json']},
 '12.2.orca-q64-static-rounding': {'paths': ['dex-router/crates/protocol-math/src/orca.rs'],
                                   'tests': ['orca_q64_profile_dispatches_without_pseudo_v3_state',
                                             'original_pinned_manager_matches_every_quote_and_shared_pool_sequence'],
                                   'evidence_refs': ['integration/artifacts/local/orca-differential.json',
                                                     'integration/artifacts/local/orca-three-service-smoke.json',
                                                     'integration/artifacts/local/orca-golden-generation.json']},
 '12.9.orca-shared-array-poststate': {'paths': ['dex-router/crates/optimizer/tests/orca_shared.rs'],
                                      'tests': ['shared_orca_sequences_reuse_q64_arrays_and_fee_growth_in_native_program_order'],
                                      'evidence_refs': ['integration/artifacts/local/orca-differential.json',
                                                        'integration/artifacts/local/orca-three-service-smoke.json',
                                                        'integration/artifacts/local/orca-golden-generation.json']},
 '12.14.orca-primary-manager-reference': {'paths': ['integration/tools/reference/orca-whirlpools/README.md',
                                                    'integration/tools/ci/orca-golden.py'],
                                          'tests': ['TestIndependentOrcaQ64TickArrayProtocWire',
                                                    'independent_protoc_preserves_orca_q64_array_proof_and_fee_growth'],
                                          'evidence_refs': ['integration/artifacts/local/orca-differential.json',
                                                            'integration/artifacts/local/orca-three-service-smoke.json',
                                                            'integration/artifacts/local/orca-golden-generation.json']},
 '12.11.orca-execution-boundary': {'paths': ['dex-router/crates/execution-plan/tests/plan.rs'],
                                   'tests': ['orca_quote_cannot_be_encoded_as_evm_v2_calldata'],
                                   'evidence_refs': ['integration/artifacts/local/orca-differential.json',
                                                     'integration/artifacts/local/orca-three-service-smoke.json',
                                                     'integration/artifacts/local/orca-golden-generation.json']},
 '11.3.rayclmm-bitmap-observation-state': {'paths': ['chain-parser-go/internal/domain/raydium_clmm.go',
                                                     'dex-router/crates/domain/src/raydium_clmm.rs'],
                                           'tests': ['TestRayCLMMIndependentBitmapObservationAndWire',
                                                     'TestRayCLMMIndependentOriginalTickMathVectors',
                                                     'independent_original_rayclmm_tickmath_vectors'],
                                           'evidence_refs': ['integration/artifacts/local/rayclmm-differential.json',
                                                             'integration/artifacts/local/rayclmm-three-service-smoke.json',
                                                             'integration/artifacts/local/rayclmm-golden-generation.json']},
 '7.3.rayclmm-finalized-pinned-accounts': {'paths': ['chain-parser-rust/crates/solana-ingest/src/raydium_clmm.rs',
                                                     'chain-parser-rust/crates/solana-ingest/src/rpc.rs'],
                                           'tests': ['pinned_layout_decodes_independent_clmm_state',
                                                     'original_program_account_fixture_matches_full_independent_state',
                                                     'observes_active_entrypoint_open_time_and_clock_bounds',
                                                     'rejects_tick_orders_rewards_padding_count_and_bitmap_mismatch'],
                                           'evidence_refs': ['integration/artifacts/local/rayclmm-differential.json',
                                                             'integration/artifacts/local/rayclmm-three-service-smoke.json',
                                                             'integration/artifacts/local/rayclmm-golden-generation.json']},
 '12.2.rayclmm-static-input-source-rounding': {'paths': ['dex-router/crates/protocol-math/src/raydium_clmm.rs'],
                                               'tests': ['rayclmm_profile_uses_independent_q64_bitmap_and_observation',
                                                         'native_original_swap_internal_matches_full_post_state_and_sequences'],
                                               'evidence_refs': ['integration/artifacts/local/rayclmm-differential.json',
                                                                 'integration/artifacts/local/rayclmm-three-service-smoke.json',
                                                                 'integration/artifacts/local/rayclmm-golden-generation.json']},
 '12.9.rayclmm-shared-pool-poststate': {'paths': ['dex-router/crates/optimizer/tests/rayclmm_shared.rs'],
                                        'tests': ['shared_rayclmm_sequences_reuse_bitmap_arrays_observation_and_fee_ledgers_in_native_program_order'],
                                        'evidence_refs': ['integration/artifacts/local/rayclmm-differential.json',
                                                          'integration/artifacts/local/rayclmm-three-service-smoke.json',
                                                          'integration/artifacts/local/rayclmm-golden-generation.json']},
 '12.14.rayclmm-native-original-reference': {'paths': ['integration/tools/reference/raydium-clmm/README.md',
                                                       'integration/tools/ci/rayclmm-golden.py'],
                                             'tests': ['TestIndependentRayCLMMBitmapAndObservationProtocWire',
                                                       'independent_protoc_preserves_rayclmm_base_bitmap_observation_and_signed_ticks'],
                                             'evidence_refs': ['integration/artifacts/local/rayclmm-differential.json',
                                                               'integration/artifacts/local/rayclmm-three-service-smoke.json',
                                                               'integration/artifacts/local/rayclmm-golden-generation.json']},
 '12.11.rayclmm-execution-boundary': {'paths': ['dex-router/crates/execution-plan/tests/plan.rs'],
                                      'tests': ['rayclmm_quote_cannot_be_encoded_as_evm_v2_calldata'],
                                      'evidence_refs': ['integration/artifacts/local/rayclmm-differential.json',
                                                        'integration/artifacts/local/rayclmm-three-service-smoke.json',
                                                        'integration/artifacts/local/rayclmm-golden-generation.json']},
 '11.3.rayclmm-token-fee-state': {'paths': ['chain-parser-go/internal/domain/raydium_clmm.go',
                                            'dex-router/crates/domain/src/raydium_clmm.rs'],
                                  'tests': ['TestRayCLMMTokenFeeExplicitProfileAndWire',
                                            'rayclmm_token_fee_has_explicit_profile_and_wire_mode',
                                            'TestIndependentRayCLMMTokenFeeProtocModes',
                                            'independent_protoc_preserves_token_fee_modes_and_rejects_profile_confusion'],
                                  'evidence_refs': ['integration/artifacts/local/rayclmm-token-fee-differential.json',
                                                    'integration/artifacts/local/rayclmm-token-fee-three-service-smoke.json',
                                                    'integration/artifacts/local/rayclmm-token-fee-golden-generation.json']},
 '7.3.rayclmm-token-fee-accounts': {'paths': ['chain-parser-rust/crates/solana-ingest/src/raydium_clmm.rs',
                                              'chain-parser-rust/crates/solana-ingest/src/rpc.rs'],
                                    'tests': ['token_fee_profile_reads_actual_fee_byte_and_keeps_old_admission_closed',
                                              'token_fee_rpc_preserves_fee_mode_in_finalized_atomic_bank'],
                                    'evidence_refs': ['integration/artifacts/local/rayclmm-token-fee-differential.json',
                                                      'integration/artifacts/local/rayclmm-token-fee-three-service-smoke.json',
                                                      'integration/artifacts/local/rayclmm-token-fee-golden-generation.json']},
 '12.2.rayclmm-token-fee-rounding': {'paths': ['dex-router/crates/protocol-math/src/raydium_clmm.rs'],
                                     'tests': ['output_fee_uses_full_input_for_price_and_net_output_for_raw_transfers',
                                               'output_fees_credit_only_the_selected_token_ledger_and_growth',
                                               'output_partial_step_charges_all_input_despite_price_rounding_dust',
                                               'pinned_native_token_fee_cases_match_complete_post_pool'],
                                     'evidence_refs': ['integration/artifacts/local/rayclmm-token-fee-differential.json',
                                                       'integration/artifacts/local/rayclmm-token-fee-three-service-smoke.json',
                                                       'integration/artifacts/local/rayclmm-token-fee-golden-generation.json']},
 '12.9.rayclmm-token-fee-shared': {'paths': ['dex-router/crates/optimizer/tests/rayclmm_token_fee_shared.rs'],
                                   'tests': ['shared_rayclmm_token_fee_sequences_update_the_charged_token_in_native_program_order',
                                             'pinned_native_token_fee_shared_sequences_match_each_and_final_pool'],
                                   'evidence_refs': ['integration/artifacts/local/rayclmm-token-fee-differential.json',
                                                     'integration/artifacts/local/rayclmm-token-fee-three-service-smoke.json',
                                                     'integration/artifacts/local/rayclmm-token-fee-golden-generation.json']},
 '12.14.rayclmm-token-fee-reference': {'paths': ['integration/tools/reference/raydium-clmm/README.md',
                                                 'integration/tools/ci/rayclmm-token-fee-golden.py'],
                                       'tests': ['pinned_native_token_fee_cases_match_complete_post_pool'],
                                       'evidence_refs': ['integration/artifacts/local/rayclmm-token-fee-differential.json',
                                                         'integration/artifacts/local/rayclmm-token-fee-three-service-smoke.json',
                                                         'integration/artifacts/local/rayclmm-token-fee-golden-generation.json']},
 '12.11.rayclmm-token-fee-execution-boundary': {'paths': ['dex-router/crates/execution-plan/tests/plan.rs'],
                                                'tests': ['rayclmm_token_fee_quote_cannot_be_encoded_as_evm_v2_calldata'],
                                                'evidence_refs': ['integration/artifacts/local/rayclmm-token-fee-differential.json',
                                                                  'integration/artifacts/local/rayclmm-token-fee-three-service-smoke.json',
                                                                  'integration/artifacts/local/rayclmm-token-fee-golden-generation.json']},
 '11.3.rayclmm-bitmap-extension-state': {'paths': ['chain-parser-go/internal/domain/rayclmm_bitmap.go',
                                                   'dex-router/crates/domain/src/raydium_clmm.rs',
                                                   'common/protoc/proto/dex/v1/state.proto'],
                                         'tests': ['TestRayCLMMBitmapExtensionIndexAndProofIdentity',
                                                   'TestRayCLMMOriginalExtensionIndexVectors',
                                                   'original_extension_index_vectors_match_source_flip_check',
                                                   'TestIndependentRayCLMMExtensionProtocWordsAndAdmission',
                                                   'independent_protoc_preserves_extension_high_words_and_profile_proof'],
                                         'evidence_refs': ['integration/artifacts/local/rayclmm-bitmap-differential.json',
                                                           'integration/artifacts/local/rayclmm-bitmap-three-service-smoke.json',
                                                           'integration/artifacts/local/rayclmm-bitmap-golden-generation.json']},
 '7.3.rayclmm-bitmap-extension-finalized-account': {'paths': ['chain-parser-rust/crates/solana-ingest/src/raydium_clmm.rs',
                                                              'chain-parser-rust/crates/solana-ingest/src/rpc.rs',
                                                              'chain-parser-rust/crates/solana-ingest/tests/rayclmm_bitmap.rs'],
                                                    'tests': ['bitmap_required_accounts_adds_derived_proof_even_for_base_arrays',
                                                              'original_bitmap_account_fixture_matches_all_words_and_full_pool'],
                                                    'evidence_refs': ['integration/artifacts/local/rayclmm-bitmap-differential.json',
                                                                      'integration/artifacts/local/rayclmm-bitmap-three-service-smoke.json',
                                                                      'integration/artifacts/local/rayclmm-bitmap-golden-generation.json']},
 '12.2.rayclmm-bitmap-extension-boundary-search': {'paths': ['dex-router/crates/protocol-math/src/raydium_clmm.rs',
                                                             'dex-router/crates/protocol-math/tests/rayclmm_bitmap_quotes.rs'],
                                                   'tests': ['positive_base_extension_boundary_in_both_directions',
                                                             'negative_base_extension_boundary_in_both_directions',
                                                             'negative_whole_group_boundary_in_both_directions',
                                                             'nearest_initialized_array_requires_proof_even_if_a_later_array_is_proven'],
                                                   'evidence_refs': ['integration/artifacts/local/rayclmm-bitmap-differential.json',
                                                                     'integration/artifacts/local/rayclmm-bitmap-three-service-smoke.json',
                                                                     'integration/artifacts/local/rayclmm-bitmap-golden-generation.json']},
 '12.9.rayclmm-bitmap-extension-shared-poststate': {'paths': ['dex-router/crates/optimizer/tests/rayclmm_bitmap_shared.rs'],
                                                    'tests': ['shared_rayclmm_bitmap_sequences_preserve_extension_and_full_native_post_pool'],
                                                    'evidence_refs': ['integration/artifacts/local/rayclmm-bitmap-differential.json',
                                                                      'integration/artifacts/local/rayclmm-bitmap-three-service-smoke.json',
                                                                      'integration/artifacts/local/rayclmm-bitmap-golden-generation.json']},
 '12.14.rayclmm-bitmap-extension-original-reference': {'paths': ['integration/tools/reference/raydium-clmm/README.md',
                                                                 'integration/tools/ci/rayclmm-bitmap-golden.py'],
                                                       'tests': ['TestRayCLMMOriginalExtensionIndexVectors',
                                                                 'original_bitmap_account_fixture_matches_all_words_and_full_pool'],
                                                       'evidence_refs': ['integration/artifacts/local/rayclmm-bitmap-differential.json',
                                                                         'integration/artifacts/local/rayclmm-bitmap-three-service-smoke.json',
                                                                         'integration/artifacts/local/rayclmm-bitmap-golden-generation.json']},
 '12.11.rayclmm-bitmap-extension-execution-boundary': {'paths': ['dex-router/crates/execution-plan/tests/plan.rs',
                                                                 'integration/tools/acceptance/rayclmm_bitmap_smoke.py'],
                                                       'tests': ['rayclmm_bitmap_quote_cannot_be_encoded_as_evm_v2_calldata'],
                                                       'evidence_refs': ['integration/artifacts/local/rayclmm-bitmap-differential.json',
                                                                         'integration/artifacts/local/rayclmm-bitmap-three-service-smoke.json',
                                                                         'integration/artifacts/local/rayclmm-bitmap-golden-generation.json']},
 '11.3.rayclmm-custom-seed-state': {'paths': ['chain-parser-go/internal/domain/rayclmm_seed_test.go',
                                              'dex-router/crates/domain/tests/rayclmm_seed.rs',
                                              'common/protoc/proto/dex/v1/state.proto',
                                              'chain-parser-go/internal/journal/frame_test.go',
                                              'dex-router/crates/kafka-transport/tests/frames.rs'],
                                    'tests': ['TestRayCLMMCustomSeedIdentityAndWire',
                                              'TestRayCLMMCustomSeedRejectsLegacyAndWidth',
                                              'custom_seed_is_bound_to_pool_and_protobuf_width',
                                              'independent_protoc_preserves_custom_seed_identity_and_u16_width',
                                              'TestIndependentRayCLMMCustomSeedProtocIdentity'],
                                    'evidence_refs': ['integration/artifacts/local/rayclmm-seed-native-source.json',
                                                      'integration/artifacts/local/rayclmm-seed-differential.json',
                                                      'integration/artifacts/local/rayclmm-seed-three-service-smoke.json']},
 '7.3.rayclmm-custom-seed-accounts': {'paths': ['chain-parser-rust/crates/solana-ingest/src/raydium_clmm.rs',
                                                'chain-parser-rust/crates/solana-ingest/src/rpc.rs',
                                                'chain-parser-rust/crates/solana-ingest/tests/rayclmm_seed.rs'],
                                      'tests': ['seed_decodes_all_u16_boundaries_and_static_fee_modes',
                                                'seed_rejects_zero_wrong_endianness_padding_and_legacy_pool_identity',
                                                'seed_rpc_fetches_complete_finalized_15_to_19_account_batches',
                                                'seed_rpc_keeps_same_mint_custom_pools_distinct_in_one_bank_and_scope'],
                                      'evidence_refs': ['integration/artifacts/local/rayclmm-seed-native-source.json',
                                                        'integration/artifacts/local/rayclmm-seed-differential.json',
                                                        'integration/artifacts/local/rayclmm-seed-three-service-smoke.json']},
 '12.2.rayclmm-custom-seed-exact-in': {'paths': ['dex-router/crates/protocol-math/tests/rayclmm_seed_differential.rs'],
                                       'tests': ['pinned_native_custom_seed_cases_match_complete_post_pool',
                                                 'pinned_native_custom_seed_shared_sequences_match_each_and_final_pool'],
                                       'evidence_refs': ['integration/artifacts/local/rayclmm-seed-native-source.json',
                                                         'integration/artifacts/local/rayclmm-seed-differential.json',
                                                         'integration/artifacts/local/rayclmm-seed-three-service-smoke.json']},
 '12.9.rayclmm-custom-seed-physical-pool': {'paths': ['dex-router/crates/optimizer/tests/rayclmm_seed_shared.rs'],
                                            'tests': ['shared_rayclmm_custom_seed_sequences_preserve_complete_native_post_pool',
                                                      'distinct_nonzero_seeds_share_mints_but_never_share_physical_pool_state'],
                                            'evidence_refs': ['integration/artifacts/local/rayclmm-seed-native-source.json',
                                                              'integration/artifacts/local/rayclmm-seed-differential.json',
                                                              'integration/artifacts/local/rayclmm-seed-three-service-smoke.json']},
 '12.14.rayclmm-custom-seed-reference': {'paths': ['integration/tools/reference/raydium-clmm/README.md',
                                                   'integration/tools/ci/rayclmm-seed-golden.py',
                                                   'dex-router/crates/protocol-math/tests/rayclmm_seed_differential.rs'],
                                         'tests': ['pinned_native_custom_seed_cases_match_complete_post_pool'],
                                         'evidence_refs': ['integration/artifacts/local/rayclmm-seed-native-source.json',
                                                           'integration/artifacts/local/rayclmm-seed-differential.json',
                                                           'integration/artifacts/local/rayclmm-seed-three-service-smoke.json']},
 '12.11.rayclmm-custom-seed-execution-boundary': {'paths': ['dex-router/crates/execution-plan/tests/plan.rs',
                                                            'integration/tools/acceptance/rayclmm_seed_smoke.py'],
                                                  'tests': ['rayclmm_seed_quote_cannot_be_encoded_as_evm_v2_calldata'],
                                                  'evidence_refs': ['integration/artifacts/local/rayclmm-seed-native-source.json',
                                                                    'integration/artifacts/local/rayclmm-seed-differential.json',
                                                                    'integration/artifacts/local/rayclmm-seed-three-service-smoke.json']},
 '11.3.rayclmm-dynamic-state': {'paths': ['chain-parser-go/internal/domain/rayclmm_dynamic_test.go',
                                          'dex-router/crates/domain/tests/rayclmm_dynamic.rs',
                                          'common/protoc/proto/dex/v1/state.proto',
                                          'chain-parser-go/internal/journal/frame_test.go',
                                          'dex-router/crates/kafka-transport/tests/frames.rs'],
                                'tests': ['TestRayCLMMDynamicStateProfileAndWire',
                                          'TestRayCLMMDynamicParametersAndOldProfileRejection',
                                          'dynamic_state_profile_width_and_wire_are_explicit',
                                          'independent_protoc_preserves_dynamic_signed_reference_and_rejects_bad_profile',
                                          'TestIndependentRayCLMMDynamicProtocParametersAndAdmission'],
                                'evidence_refs': ['integration/artifacts/local/rayclmm-dynamic-native-source.json',
                                                  'integration/artifacts/local/rayclmm-dynamic-differential.json',
                                                  'integration/artifacts/local/rayclmm-dynamic-three-service-smoke.json']},
 '7.3.rayclmm-dynamic-accounts': {'paths': ['chain-parser-rust/crates/solana-ingest/src/raydium_clmm.rs',
                                            'chain-parser-rust/crates/solana-ingest/src/rpc.rs',
                                            'chain-parser-rust/crates/solana-ingest/tests/rayclmm_dynamic.rs'],
                                  'tests': ['dynamic_actual_packed_offsets_signed_index_le_seed_and_all_fee_modes',
                                            'dynamic_padding_default_and_old_profiles_fail_closed',
                                            'dynamic_parameter_and_time_boundaries',
                                            'original_dynamic_account_fixture_matches_full_pool_and_next_bank'],
                                  'evidence_refs': ['integration/artifacts/local/rayclmm-dynamic-native-source.json',
                                                    'integration/artifacts/local/rayclmm-dynamic-differential.json',
                                                    'integration/artifacts/local/rayclmm-dynamic-three-service-smoke.json']},
 '12.2.rayclmm-dynamic-exact-in': {'paths': ['dex-router/crates/protocol-math/tests/rayclmm_dynamic.rs',
                                             'dex-router/crates/protocol-math/tests/rayclmm_dynamic_differential.rs'],
                                   'tests': ['pinned_native_dynamic_cases_match_complete_post_pool',
                                             'pinned_native_dynamic_shared_sequences_match_each_and_final_pool',
                                             'dynamic_reference_time_windows_include_equalities_and_future_saturation',
                                             'dynamic_budget_counts_spacing_iterations_including_no_price_movement'],
                                   'evidence_refs': ['integration/artifacts/local/rayclmm-dynamic-native-source.json',
                                                     'integration/artifacts/local/rayclmm-dynamic-differential.json',
                                                     'integration/artifacts/local/rayclmm-dynamic-three-service-smoke.json']},
 '12.9.rayclmm-dynamic-physical-pool': {'paths': ['dex-router/crates/optimizer/tests/rayclmm_dynamic_shared.rs'],
                                        'tests': ['shared_rayclmm_dynamic_sequences_preserve_volatility_fees_and_complete_native_post_pool'],
                                        'evidence_refs': ['integration/artifacts/local/rayclmm-dynamic-native-source.json',
                                                          'integration/artifacts/local/rayclmm-dynamic-differential.json',
                                                          'integration/artifacts/local/rayclmm-dynamic-three-service-smoke.json']},
 '12.14.rayclmm-dynamic-reference': {'paths': ['integration/tools/reference/raydium-clmm/README.md',
                                               'integration/tools/ci/rayclmm-dynamic-golden.py',
                                               'dex-router/crates/protocol-math/tests/rayclmm_dynamic_differential.rs'],
                                     'tests': ['pinned_native_dynamic_cases_match_complete_post_pool',
                                               'native_dynamic_conservative_admission_is_distinct_from_actual_step_budget'],
                                     'evidence_refs': ['integration/artifacts/local/rayclmm-dynamic-native-source.json',
                                                       'integration/artifacts/local/rayclmm-dynamic-differential.json',
                                                       'integration/artifacts/local/rayclmm-dynamic-three-service-smoke.json']},
 '12.11.rayclmm-dynamic-execution-boundary': {'paths': ['dex-router/crates/execution-plan/tests/plan.rs',
                                                        'integration/tools/acceptance/rayclmm_dynamic_smoke.py'],
                                              'tests': ['rayclmm_dynamic_quote_cannot_be_encoded_as_evm_v2_calldata'],
                                              'evidence_refs': ['integration/artifacts/local/rayclmm-dynamic-native-source.json',
                                                                'integration/artifacts/local/rayclmm-dynamic-differential.json',
                                                                'integration/artifacts/local/rayclmm-dynamic-three-service-smoke.json']}}

# Receipt completeness is a block-assembly requirement shared by EVM profiles.
BINDINGS['7.3.evm-receipt-integrity'] = {
    'paths': ['chain-parser-go/internal/chain/evm/adapter.go', 'chain-parser-go/internal/chain/evm/receipt_integrity_test.go'],
    'tests': ['TestReceiptIntegrityUsesCanonicalTransactionOrder', 'TestReceiptIntegrityRejectsIncompleteOrAmbiguousPositions',
              'TestReadCompleteRejectsReceiptGapBeforeReadingPoolState', 'TestRPCBlockRequiresExplicitTransactionArray'],
    'evidence_refs': ['integration/artifacts/local/receipt-integrity-acceptance.json'],
}

BINDINGS['7.3.evm-receipt-witnesses'] = {
    'paths': ['chain-parser-go/internal/rpc/client.go', 'chain-parser-go/internal/rpc/providers_test.go',
              'chain-parser-go/internal/chain/evm/receipt_witness.go', 'chain-parser-go/internal/chain/evm/receipt_witness_test.go',
              'integration/tools/acceptance/receipt_witness_smoke.py'],
    'tests': ['TestProviderClientsAreDistinctHealthyAndEndpointBound', 'TestForkConfirmationUsesByteEquivalentBlockHashCase',
              'TestReadCompleteCrossProviderWitnessRejectsMissingTailAndDivergence',
              'TestReadCompleteWitnessCatchesPrimaryTailOmissionAndNeverVotes',
              'TestReadCompleteWitnessAcceptsExactBytesAcrossHexCaseAndEmptyBlocks',
              'TestReadCompleteSingleEndpointDoesNotPretendToBeTwoSources',
              'TestReadCompleteWitnessQuarantinedSecondSourceCannotSilentlyDegrade',
              'TestReceiptWitnessDigestNormalizesReceiptOrderWithoutMutation',
              'TestReadCompleteWitnessCancellationRetainsOuterDeadline',
              'TestReadCompleteWitnessAcceptsByteIdenticalHashAndTransactionCase',
              'TestReadCompleteReceiptAggregateBudgetStopsIndividuallySmallResponses',
              'TestValidateReceiptLogEncodingBeforeNormalization', 'TestReceiptWitnessDigestCanceledWorkStopsBeforeHashing'],
    'evidence_refs': ['integration/artifacts/local/receipt-witness-acceptance.json'],
}

BINDINGS['8.9.solana-rpc-source-wal'] = {
    'paths': ['chain-parser-rust/crates/solana-ingest/src/source_wal.rs',
              'chain-parser-rust/crates/solana-ingest/src/rpc.rs',
              'chain-parser-rust/crates/solana-ingest/src/service.rs',
              'chain-parser-rust/crates/solana-ingest/tests/source_wal.rs',
              'chain-parser-rust/crates/solana-ingest/tests/source_wal_rpc.rs'],
    'tests': ['raw_response_roundtrips_exact_bytes_identity_and_durable_session',
              'wal_limits_are_fail_closed_and_do_not_advance_sequence',
              'wal_rejects_wrong_chain_concurrent_writer_and_payload_corruption',
              'wal_rejects_missing_record_or_uncommitted_extra_record',
              'rpc_raw_account_response_is_durable_before_decoder_rejects_it',
              'wal_capacity_failure_cannot_fall_through_to_another_provider',
              'failed_sync_commit_poisons_dedup_and_new_writes_until_verified_reopen',
              'underlying_rocksdb_log_corruption_truncation_and_loss_never_accept_an_old_prefix',
              'timed_out_captures_do_not_accumulate_blocking_jobs_or_change_arrival_time',
              'durable_head_session_only_corruption_is_rejected_without_overwriting_evidence',
              'oversized_head_reads_only_limit_plus_one_before_rejecting'],
    'evidence_refs': ['integration/artifacts/local/source-wal-acceptance.json'],
}

BINDINGS['7.3.evm-receipt-root'] = {
    'paths': ['chain-parser-go/internal/chain/evm/receipt_root.go',
              'chain-parser-go/internal/chain/evm/adapter.go',
              'chain-parser-go/internal/chain/evm/receipt_witness.go',
              'chain-parser-go/test/fixtures/receipt-roots.json',
              'integration/tools/reference/receipt-root/oracle.cjs',
              'integration/tools/acceptance/receipt_root_smoke.py'],
    'tests': ['TestReadCompleteRequiresReceiptRootBeforePoolReads',
              'TestReceiptRootMatchesIndependentEthereumJSVectors',
              'TestReceiptRootRejectsConsensusFieldTampering',
              'TestReadCompleteRejectsMatchingTailOmissionsFromEveryProvider',
              'TestReceiptRootLegacyTypeOmissionAndGasOrder',
              'TestReceiptRootRejectsExplicitEmptyOrNullTypeFromJSON',
              'TestReceiptRootRejectsExplicitAmbiguousOutcomeFromJSON',
              'TestReadCompleteRootAnchorChangesNeverPublish'],
    'evidence_refs': ['integration/artifacts/local/receipt-root-acceptance.json'],
}

BINDINGS['8.15.solana-current-account-shards'] = {
    'paths': ['chain-parser-rust/crates/solana-ingest/src/rpc.rs',
              'chain-parser-rust/crates/solana-ingest/tests/rpc_shards.rs',
              'integration/tools/acceptance/account_shards_smoke.py',
              'integration/tools/acceptance/account_shards_evidence.py'],
    'tests': ['same_finalized_context_shards_decode_all_pools_without_cross_slot_stitching',
              'different_shard_context_fails_the_whole_snapshot',
              'shard_clock_completeness_and_bank_barriers_reject_partial_snapshots',
              'aggregate_account_wire_budget_is_not_multiplied_by_the_number_of_shards',
              'archive_only_scopes_cannot_bypass_shard_guards_and_exact_100_stays_single',
              'mixed_decoder_dependencies_and_decoder_specific_null_oracles_remain_exact',
              'maximum_shard_plan_is_32_requests_and_35_total_calls',
              'seed_rpc_decodes_all_custom_pools_across_same_context_shards',
              'provider_fallback_restarts_the_entire_frozen_manifest',
              'incomplete_providers_never_fill_each_others_missing_accounts',
              'rejected_shards_are_recorded_before_context_decode_and_wal_failure_never_falls_back'],
    'evidence_refs': ['integration/artifacts/local/account-shards-acceptance.json'],
}

# WeightedPool is an independent typed model; these mappings do not confer
# deployment, execution or full WP11 acceptance.
for clause, paths, tests in [
 ('11.3.balancer-weighted2-equal-state',
  ['common/protoc/proto/dex/v1/state.proto','chain-parser-go/internal/domain/weighted.go','dex-router/crates/domain/src/weighted.rs','chain-parser-go/internal/config/config.go','dex-router/crates/config/src/lib.rs'],
  ['TestWeightedProfileAndOwnStateRequired','TestWeightedRejectsModelMixingParametersAndMalformedWire','weighted_is_distinct_and_rejects_other_parameters','independent_protoc_weighted_field27_and_full_roundtrip','TestWeightedCloneDoesNotShareMutableEnvelope','TestWeightedCurrentSnapshotCannotMutateStoredState']),
 ('7.3.balancer-weighted2-equal-block',
  ['chain-parser-go/internal/chain/evm/weighted.go','chain-parser-go/internal/chain/evm/adapter.go','chain-parser-go/internal/service/service.go','integration/tools/acceptance/weighted_smoke.py'],
  ['TestWeightedFixedBlockAndUnsupportedDependencies','TestWeightedConfigExplicitTokenPolicyAndModelIsolation','TestLargeGapWeightedAdmissionFailureStillPublishesRevocation']),
 ('12.2.balancer-weighted2-equal-exact-in',
  ['dex-router/crates/protocol-math/src/balancer_weighted.rs','dex-router/crates/protocol-math/src/lib.rs'],
  ['weighted_rounds_as_native_fixed_point_not_v2_fraction','weighted_fee_cap_scaling_and_overflow_boundaries','original_weighted_pool_callback_and_vault_cash_match']),
 ('12.9.balancer-weighted2-equal-shared',
  ['dex-router/crates/optimizer/src/lib.rs','dex-router/crates/optimizer/tests/weighted_shared.rs'],
  ['weighted_shared_pool_replays_both_directions_and_recalculates_candidates']),
 ('12.14.balancer-weighted2-equal-reference',
  ['integration/tools/reference/weighted-model/Harness.sol','integration/tools/reference/weighted-model/source-metadata.json','integration/tools/ci/weighted-golden.cjs','integration/tools/acceptance/weighted_evidence.py'],
  ['original_weighted_pool_callback_and_vault_cash_match','test_new_binary_rejected','test_new_input_rejected','test_optimized_checks_are_not_assertions']),
 ('12.11.balancer-weighted2-equal-execution-boundary',
  ['dex-router/crates/execution-plan/src/lib.rs','integration/tools/acceptance/weighted_smoke.py'],
  ['weighted_quote_cannot_be_encoded_as_v2_router_calldata','test_outer_generation_and_amount_and_verification_fail']),
]:
 BINDINGS[clause]={'paths':paths,'tests':tests,'evidence_refs':['integration/artifacts/local/weighted-acceptance.json']}

for clause, paths, tests in [
 ('11.3.balancer-weighted2-static-state',
  ['common/protoc/proto/dex/v1/state.proto','chain-parser-go/internal/domain/weighted.go','dex-router/crates/domain/src/weighted.rs','chain-parser-go/internal/config/config.go','dex-router/crates/config/src/lib.rs'],
  ['TestWeightedStaticAdmissionAndAdapterMatrix','static_profiles_preserve_full_weights_wire_and_old_admission_matrix','independent_protoc_weighted_static_field27_and_full_roundtrip']),
 ('7.3.balancer-weighted2-static-block',
  ['chain-parser-go/internal/chain/evm/weighted.go','integration/tools/acceptance/weighted_static_smoke.py'],
  ['TestWeightedFixedBlockAndUnsupportedDependencies','TestWeightedStaticConfigExplicitTokenPolicyAndModelIsolation']),
 ('12.2.balancer-weighted2-static-exact-in',
  ['dex-router/crates/protocol-math/src/balancer_log_exp.rs','dex-router/crates/protocol-math/src/balancer_weighted.rs','dex-router/crates/protocol-math/src/lib.rs'],
  ['original_solidity_logexp_and_pow_up_match_branch_and_domain_vectors','original_arbitrary_weight_pool_callback_and_vault_cash_match','exact_in_can_withdraw_more_than_thirty_percent_for_asymmetric_weights']),
 ('12.9.balancer-weighted2-static-shared',
  ['dex-router/crates/optimizer/tests/weighted_static_shared.rs','dex-router/crates/optimizer/src/lib.rs'],
  ['weighted_static_shared_pool_replays_both_directions_and_recalculates_candidates','equal_and_static_labels_cannot_duplicate_one_physical_pool']),
 ('12.14.balancer-weighted2-static-reference',
  ['integration/tools/reference/weighted-model/StaticMathRef.sol','integration/tools/ci/weighted-static-golden.cjs','integration/tools/acceptance/weighted_static_evidence.py','integration/tools/acceptance/weighted_evidence.py'],
  ['original_solidity_logexp_and_pow_up_match_branch_and_domain_vectors','test_changed_generation_counts_and_fingerprints_are_rejected','test_changed_original_source_or_frozen_legacy_fixture_is_rejected']),
 ('12.11.balancer-weighted2-static-execution-boundary',
  ['dex-router/crates/execution-plan/src/lib.rs','integration/tools/acceptance/weighted_static_smoke.py','docs/TECHNICAL_REFERENCE.md'],
  ['weighted_static_quote_cannot_be_encoded_as_v2_router_calldata','test_old_and_new_protocol_labels_and_counts_cannot_be_interchanged']),
]:
 BINDINGS[clause]={'paths':paths,'tests':tests,'evidence_refs':['integration/artifacts/local/weighted-static-acceptance.json']}

for clause, paths, tests in [
 ('11.3.multi-asset-physical-pool',
  ['common/protoc/proto/dex/v1/state.proto','chain-parser-go/internal/domain/multi_asset.go','dex-router/crates/domain/src/multi_asset.rs','dex-router/crates/routing/src/lib.rs'],
  ['TestMultiAssetPhysicalStateWireAndClone','TestMultiAssetClonePreservesRequiredEmptyWeightVector','a_multicoin_pool_requires_one_typed_physical_state_and_all_asset_balances']),
 ('7.3.multi-asset-fixed-block',
  ['chain-parser-go/internal/chain/evm/multi_asset.go','integration/tools/acceptance/multi_model_smoke.py'],
  ['TestMultiTokenWeightedReadsAllAnchoredDependenciesAndRejectsIncompleteVectors','TestMultiAssetConfigRequiresFullOrderedTokenProof']),
 ('12.9.multi-asset-topology-and-capacity',
  ['dex-router/crates/routing/src/lib.rs','dex-router/crates/state-replica/src/lib.rs','dex-router/crates/optimizer/tests/multi_asset_capacity.rs'],
  ['a_changed_third_asset_rebuilds_the_complete_graph_without_mutating_old_handle','weighted_and_managed_capacity_failures_still_probe_smaller_allocations','two_individually_small_weighted_or_managed_pools_fill_the_complete_order']),
 ('12.2.balancer-multi-managed-composable-subsets',
  ['dex-router/crates/protocol-math/src/balancer_multi.rs','dex-router/crates/protocol-math/src/balancer_composable.rs','chain-parser-go/internal/chain/evm/multi_asset.go'],
  ['original_multi_weighted_callbacks_match_outputs_reverts_and_entire_poststate','original_managed_callback_matches_compressed_scheduled_snapshot_weights','original_stablemath_regular_and_bpt_kernels_match','no_rate_nonzero_fee_admission_vectors_match_original_kernel']),
 ('12.2.curve-extra-model-subsets',
  ['dex-router/crates/protocol-math/src/curve_multi.rs','dex-router/crates/protocol-math/src/curve_ng.rs','dex-router/crates/protocol-math/src/curve_crypto.rs'],
  ['original_vyper_exchange_and_poststate_720_cases','original_ng_full_exchange_and_erc4626_rates_522_cases','original_crypto_get_dy_exchange_oracle_and_profit_288_cases','crypto_adjustment_and_admin_claim_are_explicit_boundaries']),
 ('11.3.solana-reward-state',
  ['common/protoc/proto/dex/v1/state.proto','chain-parser-go/internal/domain/orca.go','chain-parser-go/internal/domain/raydium_clmm.go','dex-router/crates/domain/src/orca.rs','dex-router/crates/domain/src/raydium_clmm.rs'],
  ['TestIndependentFiveFamilyWirePreservesFullPhysicalAndRewardState','independently_encoded_multi_singleton_and_rewards_survive_full_wire']),
 ('7.3.solana-reward-accounts',
  ['chain-parser-rust/crates/solana-ingest/src/orca.rs','chain-parser-rust/crates/solana-ingest/src/raydium_clmm.rs','integration/tools/acceptance/solana_rewards_smoke.py'],
  ['original_native_packed_accounts_decode_to_complete_reward_and_tick_state','initialized_rewards_remain_rejected_by_legacy_profiles','token2022_and_initialized_adaptive_oracles_are_still_unsupported']),
 ('12.2.solana-reward-poststates',
  ['dex-router/crates/protocol-math/src/orca.rs','dex-router/crates/protocol-math/src/raydium_clmm.rs','integration/tools/ci/solana-advanced-golden.py'],
  ['full_swaps_match_original_native_output_and_entire_poststate','reward_kernels_match_native_overflow_cap_zero_liquidity_and_crossings']),
 ('11.3.singleton-advanced-state',
  ['common/protoc/proto/dex/v1/state.proto','chain-parser-go/internal/domain/singleton_advanced_wire.go','dex-router/crates/domain/src/singleton_pool.rs','dex-router/crates/domain/src/singleton.rs'],
  ['TestIndependentFiveFamilyWirePreservesFullPhysicalAndRewardState','independently_encoded_multi_singleton_and_rewards_survive_full_wire']),
 ('12.11.singleton-native-dynamic-subsets',
  ['chain-parser-go/internal/chain/evm/singleton.go','chain-parser-go/internal/chain/evm/adapter.go','dex-router/crates/protocol-math/src/singleton.rs'],
  ['TestSingletonAdvancedReadFrozenDynamicNativeSnapshot','original_contract_advanced_quotes_and_manager_fee_match','central_advanced_profile_dispatch_preserves_snapshot_identity','advanced_config_has_the_same_callback_capability_guards_as_runtime']),
 ('12.2.infinity-single-bin-kernel',
  ['dex-router/crates/protocol-math/src/infinity_bin.rs','integration/tools/reference/singleton-advanced/InfinityBinReference.sol'],
  ['original_infinity_bin_swap_matches_independent_single_bin_math']),
 ('12.14.five-family-bounded-reference',
  ['integration/tools/acceptance/five_family_evidence.py','integration/tools/acceptance/five_family_regression.py','integration/tools/reference/curve-models/sources.json','integration/tools/reference/singleton-advanced/source-metadata.json'],
  ['test_bounded_success_does_not_claim_all_user_requested_families_finished','test_current_original_sources_and_corpus_bounds','test_failed_singleton_generator_cannot_reuse_corpus','test_failed_validation_destroys_prior_success']),
]:
 BINDINGS[clause]={'paths':paths,'tests':tests,'evidence_refs':['integration/artifacts/local/five-family-acceptance.json']}

# Fixed original StableSwapNG standard-token service increment.
for clause, paths, tests in [
 ('11.3.curve-ng-full-state', ['common/protoc/proto/dex/v1/state.proto','chain-parser-go/internal/domain/curve_ng_state.go','dex-router/crates/domain/src/curve_ng_state.rs'], ['TestIndependentCurveNgStateProtoAndSourceBank','ng_state_strictly_matches_effective_balances_fee_oracle_and_wire']),
 ('7.3.curve-ng-fixed-block', ['chain-parser-go/internal/chain/evm/curve_ng.go','chain-parser-go/internal/config/config.go'], ['TestCurveNGAnchoredImmutableTypeBalancesAndOracleState','TestCurveNGConfigRestrictsIdentityTokensAndStandardRuntimeLayout']),
 ('12.2.curve-ng-full-exchange', ['dex-router/crates/protocol-math/src/curve_ng.rs','dex-router/crates/protocol-math/src/curve_ng_oracle.rs'], ['original_full_exchange_every_step_including_ema_raw_and_admin','normal_ng_profile_dispatch_matches_complete_original_exchange_postpool']),
 ('12.9.curve-ng-capacity', ['dex-router/crates/routing/src/lib.rs','dex-router/crates/protocol-math/src/multi_asset.rs','dex-router/crates/optimizer/tests/curve_ng_state.rs'], ['ng_capacity_overflow_keeps_smaller_allocations_discoverable','ng_shared_allocations_recompute_raw_admin_and_oracles_on_each_complete_poststate']),
 ('12.14.curve-ng-service-reference', ['integration/tools/acceptance/curve_ng_smoke.py','integration/tools/acceptance/curve_ng_evidence.py','integration/tools/ci/curve-ng-state-verify.py'], ['original_signed_exp_boundaries_and_wrapping_spot_intermediates','TestCurveNGAnchoredImmutableTypeBalancesAndOracleState']),
]:
 BINDINGS[clause]={'paths':paths,'tests':tests,'evidence_refs':['integration/artifacts/local/curve-ng-acceptance.json']}

# Managed fee math/reference is explicitly not online admission.
for clause, paths, tests in [
 ('11.3.managed-bpt-state', ['common/protoc/proto/dex/v1/state.proto','chain-parser-go/internal/domain/managed_fee_state.go','dex-router/crates/domain/src/managed_fee_state.rs'], ['TestManagedFeesWireCloneAndSnapshotIdentity','independent_managed_bpt_wire_preserves_source_and_rejects_foreign_bank']),
 ('7.3.managed-bpt-anchored-ledger', ['chain-parser-go/internal/chain/evm/managed_fee_ledger.go'], ['TestManagedFeeLedgerUsesOneBlockHashAndCoalescesRecipients']),
 ('12.2.managed-bpt-full-callback', ['dex-router/crates/protocol-math/src/balancer_managed_fees.rs','dex-router/crates/protocol-math/src/balancer_log_exp.rs'], ['native_zero_raw_output_still_mints_bpt_before_downscale','distinct_recipient_balances_cannot_exceed_total_supply_together']),
 ('12.14.managed-bpt-original-reference', ['integration/tools/ci/balancer-managed-fees.cjs','integration/tools/ci/balancer-managed-fees-verify.py'], ['original_initialized_managed_pool_transactions_match_math_and_mint_state','test_compiler_failure_invalidates_old_success']),
 ('12.11.managed-bpt-admission-boundary', ['chain-parser-go/internal/config/config.go','chain-parser-go/internal/chain/evm/multi_asset.go','dex-router/crates/config/src/lib.rs','integration/tools/ci/managed-fees-runtime-golden.py'], ['TestManagedReferenceRuntimeCannotEnterNormalParser','managed_reference_state_is_not_a_live_quote_capability','test_reference_only_compiler_output_is_never_projected']),
]:
 BINDINGS[clause]={'paths':paths,'tests':tests,'evidence_refs':['integration/artifacts/local/managed-reference-acceptance.json']}

# MetaUSD direct exchange has a read-only base pool dependency.
for clause, paths, tests in [
 ('11.3.metausd-full-state',['common/protoc/proto/dex/v1/state.proto','chain-parser-go/internal/domain/curve_meta_state.go','dex-router/crates/domain/src/curve_meta_state.rs'],['TestIndependentMetaProtoPreservesSourceAndRejectsForeignBank','meta_complete_accounting_source_and_independent_wire_roundtrip']),
 ('7.3.metausd-fixed-block',['chain-parser-go/internal/chain/evm/curve_meta.go','chain-parser-go/internal/config/config.go'],['TestMetaFixedBlockReadsOriginalCloneFullBalancesAndBaseDependency','TestMetaConfigRequiresPinnedCloneAndReadOnlyBaseProof']),
 ('12.2.metausd-direct-poststate',['dex-router/crates/protocol-math/src/curve_multi.rs','dex-router/crates/protocol-math/src/multi_asset.rs'],['meta_profile_matches_original_outputs_and_complete_physical_poststate','original_clone_identity_and_32_sequential_exchanges']),
 ('12.9.metausd-readonly-base',['dex-router/crates/routing/src/lib.rs','dex-router/crates/optimizer/src/lib.rs','dex-router/crates/domain/src/curve_meta_state.rs'],['graph_and_recomputation_deny_mutable_virtual_price_dependency','shared_meta_swaps_recompute_entire_raw_admin_state_and_keep_base_readonly']),
 ('12.14.metausd-service-reference',['integration/tools/ci/curve-meta-verify.py','integration/tools/acceptance/curve_meta_smoke.py','integration/tools/acceptance/curve_meta_evidence.py'],['TestMetaReadOnlyBaseDependencyAndSharedSourceAreEnforced','meta_config_requires_pinned_clone_base_proof_and_dependency_isolation']),
]:
 BINDINGS[clause]={'paths':paths,'tests':tests,'evidence_refs':['integration/artifacts/local/curve-meta-acceptance.json']}

# Three separately bounded priority extensions; kernel scope is not online support.
for clause,paths,tests in [
 ('11.3.crypto-complete-pricing-and-lp-state',['common/protoc/proto/dex/v1/state.proto','chain-parser-go/internal/domain/curve_crypto_state.go','dex-router/crates/domain/src/curve_crypto_state.rs'],['TestCryptoFullIndependentWireCloneAndSourceBank','crypto_complete_wire_ledger_and_scope_are_preserved']),
 ('7.3.crypto-original-runtime-and-fixed-bank',['chain-parser-go/internal/chain/evm/curve_crypto.go','chain-parser-go/internal/config/config.go'],['TestCryptoAnchoredOriginalRuntimeLPAndCompleteState','TestCryptoConfigRequiresPinnedLPAndDedicatedProfile']),
 ('12.2.crypto-adjustment-admin-mint-and-gulp',['dex-router/crates/protocol-math/src/curve_crypto.rs','dex-router/crates/protocol-math/src/multi_asset.rs'],['original_exchange_and_original_lp_full_poststates','complete_crypto_normal_dispatch_matches_original_exchange_and_lp_poststate']),
 ('12.14.crypto-service-and-source-evidence',['integration/tools/acceptance/curve_crypto_smoke.py','integration/tools/ci/curve-crypto-full-verify.py'],['test_complete_runtime_immutable_and_source_bank','test_infinity_failed_execution_receipt_is_rejected_even_when_optimized']),
 ('12.2.orca-adaptive-fee-state-manager',['dex-router/crates/protocol-math/src/orca_adaptive.rs','integration/tools/ci/orca-adaptive-golden.py'],['original_native_corpus_matches_every_state_and_group','reference_filter_decay_and_forced_reset_boundaries']),
 ('12.2.infinity-complete-initialized-bin-traversal',['dex-router/crates/protocol-math/src/infinity_bin_multi.rs','integration/tools/reference/infinity-bin-multi/InfinityBinMultiReference.sol'],['original_native_swap_differential','test_infinity_failed_execution_receipt_is_rejected_even_when_optimized']),
]:
 BINDINGS[clause]={'paths':paths,'tests':tests,'evidence_refs':['integration/artifacts/local/priority-extensions-acceptance.json']}
BINDINGS['12.2.orca-adaptive-full-swap-poststate']={
 'paths':['dex-router/crates/protocol-math/src/orca_adaptive_swap.rs','integration/tools/reference/orca-adaptive-swap/src/main.rs','integration/tools/ci/orca-adaptive-swap-golden.py'],
 'tests':['original_swap_manager_corpus_matches_full_post_state','rejects_unproven_or_unsupported_state_without_mutation','original_native_proof_exhaustion_is_an_atomic_error'],
 'evidence_refs':['integration/artifacts/local/priority-extensions-acceptance.json']}
BINDINGS['12.9.crypto-admin-lp-mutation-boundary']={
 'paths':['chain-parser-go/internal/domain/curve_crypto_state.go','dex-router/crates/domain/src/curve_crypto_state.rs','dex-router/crates/routing/src/lib.rs','dex-router/crates/optimizer/src/lib.rs'],
 'tests':['TestCryptoAdminMintCannotMutateAnotherQuotedPoolBalance','TestCryptoZeroAdminFeeDoesNotCreateMutableLPDependency','crypto_lp_mint_dependency_is_rejected_by_snapshot_installation','admin_lp_mint_cannot_silently_change_another_candidate_pool'],
 'evidence_refs':['integration/artifacts/local/priority-extensions-acceptance.json']}

# The adaptive online version remains distinct from both legacy Orca profiles.
for clause,paths,tests in [
 ('11.3.orca-adaptive-complete-oracle',['common/protoc/proto/dex/v1/state.proto','chain-parser-go/internal/domain/orca_adaptive.go','dex-router/crates/domain/src/orca_adaptive_state.rs'],['TestOrcaAdaptiveIndependentCompleteWireSnapshot','independent_adaptive_proto_binds_oracle_to_snapshot_bank']),
 ('7.3.orca-adaptive-finalized-bank',['chain-parser-rust/crates/solana-ingest/src/orca.rs','chain-parser-rust/crates/solana-ingest/src/rpc.rs'],['initialized_oracle_is_parsed_only_for_its_own_adaptive_version','adaptive_normal_rpc_uses_initialized_oracle_same_finalized_bank_and_reanchors']),
 ('12.2.orca-adaptive-online-poststate',['dex-router/crates/protocol-math/src/orca_adaptive_online.rs','dex-router/crates/protocol-math/src/orca_adaptive_swap.rs'],['normal_adaptive_quotes_match_original_packed_program_banks','adaptive_normal_dispatch_uses_native_complete_swap_and_all_poststate']),
 ('12.9.orca-adaptive-shared-pool',['dex-router/crates/optimizer/tests/orca_adaptive_shared.rs','dex-router/crates/protocol-math/src/orca_adaptive_online.rs'],['carried_adaptive_oracle_and_ticks_are_reused_by_shared_allocations']),
 ('12.14.orca-adaptive-normal-service-reference',['integration/tools/acceptance/orca_adaptive_smoke.py','integration/tools/reference/orca-adaptive-service/src/main.rs','integration/tools/ci/orca-adaptive-service-golden.py'],['original_program_packed_adaptive_banks_match_full_expected_service_state','test_original_banks_are_replayed_with_one_finalized_slot']),
]:
 BINDINGS[clause]={'paths':paths,'tests':tests,'evidence_refs':['integration/artifacts/local/orca-adaptive-acceptance.json']}

for clause,paths,tests in [
 ('11.3.infinity-bin-complete-state',['common/protoc/proto/dex/v1/state.proto','chain-parser-go/internal/domain/infinity_bin.go','dex-router/crates/domain/src/infinity_bin.rs'],['independent_bin_proto_has_complete_tree_balances_shares_and_source_bank','missing_tree_nodes_extra_bins_widths_or_identity_are_rejected']),
 ('7.3.infinity-bin-original-manager-tree',['chain-parser-go/internal/chain/evm/infinity_bin.go','integration/tools/reference/infinity-bin-service/compile-evidence.json'],['TestBinOriginalManagerFullTreeAnchoredReadAndRejectsCorruption','TestAllOriginalBinBanksMatchCompleteProjectedState']),
 ('12.2.infinity-bin-normal-poststate',['dex-router/crates/protocol-math/src/infinity_bin_online.rs','dex-router/crates/protocol-math/src/infinity_bin_multi.rs'],['normal_bin_quotes_match_original_manager_and_cross_bin_poststates','online_bin_state_reuses_original_sparse_tree_math_and_complete_balances']),
 ('12.9.infinity-bin-shared-manager-currencies',['dex-router/crates/domain/src/infinity_bin.rs','dex-router/crates/routing/src/lib.rs','chain-parser-go/internal/domain/infinity_bin_dependencies.go'],['manager_currency_accounting_propagates_to_other_physical_pools_and_sequences','manager_currency_dependencies_and_source_time_are_bound_across_physical_pools']),
 ('12.2.infinity-bin-detached-active-burn',['dex-router/crates/protocol-math/src/infinity_bin_multi.rs','chain-parser-go/internal/chain/evm/infinity_bin.go'],['tree_outside_active_is_consumed_before_strict_tree_successor','detached_locked_active_keeps_or_drops_its_state_and_never_enters_tree','TestBinDetachedActiveDustIsReadSeparatelyFromActualTree']),
 ('12.14.infinity-bin-normal-service-reference',['integration/tools/acceptance/infinity_bin_smoke.py','integration/tools/acceptance/infinity_bin_evidence.py','integration/tools/reference/infinity-bin-service/InfinityBinServiceMath.sol'],['test_original_callmaps_remain_hash_pinned_for_every_bank','test_reference_replays_carried_bins_and_checks_total_and_app_reserve']),
]:
 BINDINGS[clause]={'paths':paths,'tests':tests,'evidence_refs':['integration/artifacts/local/infinity-bin-acceptance.json']}

# Public ramp schedules use a separate version; the settled profile stays closed.
for clause, paths, tests in [
 ('11.3.crypto-ramp-complete-schedule', ['common/protoc/proto/dex/v1/state.proto','chain-parser-go/internal/domain/curve_crypto_state.go','dex-router/crates/domain/src/curve_crypto_state.rs'], ['TestCryptoRampPublicServiceAndIndependentWire','crypto_ramp_public_service_and_independent_wire_preserve_origin']),
 ('7.3.crypto-ramp-fixed-bank', ['chain-parser-go/internal/chain/evm/curve_crypto.go','chain-parser-go/internal/config/config.go'], ['TestCryptoAnchoredOriginalRuntimeLPAndCompleteState','TestCryptoOriginalServiceConfigurations']),
 ('12.2.crypto-ramp-normal-poststate', ['dex-router/crates/protocol-math/src/multi_asset.rs','dex-router/crates/protocol-math/src/curve_crypto.rs'], ['active_ramp_refreshes_cached_invariant_from_preswap_balances']),
 ('12.9.crypto-ramp-carried-sequences', ['dex-router/crates/protocol-math/tests/curve_crypto_ramp.rs','integration/tools/ci/curve-crypto-ramp-golden.cjs'], ['public_original_ramp_and_stop_normal_dispatch_and_carried_sequences','active_ramp_shared_allocations_keep_complete_schedule_and_recomputed_state']),
 ('12.14.crypto-ramp-normal-service-reference', ['integration/tools/acceptance/curve_crypto_ramp_smoke.py','integration/tools/acceptance/curve_crypto_ramp_evidence.py','integration/tools/ci/curve-crypto-ramp-verify.py'], ['test_current_ramp_process_requires_all_banks_faults_and_execution_inputs','test_public_ramp_and_complete_carried_native_state_cannot_be_truncated']),
]:
 BINDINGS[clause]={'paths':paths,'tests':tests,'evidence_refs':['integration/artifacts/local/curve-crypto-ramp-acceptance.json']}

BINDINGS['23.optimized-reference-rejection-guards']={
 'paths':['integration/tools/acceptance/rayclmm_dynamic_evidence.py','integration/tools/acceptance/rayclmm_seed_evidence.py'],
 'tests':['test_native_output_or_harness_fingerprint_drift_is_rejected','test_command_cannot_hide_an_extra_skip_or_change_final_target'],
 'evidence_refs':['integration/artifacts/local/curve-crypto-ramp-acceptance.json']}

BINDINGS['7.3.live-head-clock-admission']={
 'paths':['chain-parser-go/internal/service/service.go','chain-parser-go/internal/bootstrap/bootstrap.go'],
 'tests':['TestLiveHeadFreshnessRejectsBeforePoolReadsAndRevokesScope','test_future_head_fault_changes_header_clock_and_recovers_to_original_bank'],
 'evidence_refs':['integration/artifacts/local/curve-crypto-ramp-acceptance.json']}

# Adaptive reward growth is admitted only by its independent initialized-reward profile.
for clause, paths, tests in [
 ('11.3.orca-adaptive-rewards-state', ['chain-parser-go/internal/domain/orca.go','dex-router/crates/domain/src/orca.rs'], ['TestOrcaAdaptiveRewardsIndependentWire','adaptive_rewards_independent_wire_preserves_full_snapshot']),
 ('7.3.orca-adaptive-rewards-bank', ['chain-parser-rust/crates/solana-ingest/src/orca.rs','chain-parser-rust/crates/solana-ingest/src/rpc.rs'], ['adaptive_rewards_twenty_native_packed_banks_and_old_profile_rejections']),
 ('12.2.orca-adaptive-rewards-poststate', ['dex-router/crates/protocol-math/src/orca_adaptive_swap.rs','dex-router/crates/protocol-math/src/orca_adaptive_online.rs'], ['adaptive_rewards_kernel_checked_product_wrap_cross_and_preserve_uninitialized','adaptive_rewards_original_normal_dispatch_all_twenty_banks_and_full_sequences']),
 ('12.9.orca-adaptive-rewards-shared-pool', ['dex-router/crates/optimizer/tests/orca_adaptive_rewards_shared.rs'], ['carried_adaptive_rewards_oracle_and_ticks_are_reused_by_shared_allocations']),
 ('12.14.orca-adaptive-rewards-service-reference', ['integration/tools/reference/orca-adaptive-rewards-service/src/main.rs','integration/tools/ci/orca-adaptive-rewards-service-golden.py','integration/tools/acceptance/orca_adaptive_rewards_evidence.py'], ['test_all_current_inputs_and_normal_source_recovery_checks_are_required','test_rewards_reference_replays_selected_split_allocations_with_carried_accounts']),
]:
 BINDINGS[clause]={'paths':paths,'tests':tests,'evidence_refs':['integration/artifacts/local/orca-adaptive-rewards-acceptance.json']}

# Dynamic/mixed adaptive arrays are a separate version from both fixed profiles.
for clause, paths, tests in [
 ('11.3.orca-adaptive-dynamic-state', ['chain-parser-go/internal/domain/orca.go','dex-router/crates/domain/src/orca.rs'], ['adaptive_dynamic_profile_mixed_wire_and_old_profile_isolation','adaptive_dynamic_profile_rejects_incomplete_bitmap_identity_and_oracle']),
 ('7.3.orca-adaptive-dynamic-bank', ['chain-parser-rust/crates/solana-ingest/src/orca.rs','chain-parser-rust/crates/solana-ingest/src/rpc.rs'], ['adaptive_dynamic_twenty_native_packed_banks_and_old_profile_rejections','adaptive_dynamic_normal_rpc_exact_compact_layout_and_array_faults','test_original_dynamic_and_fixed_account_bytes_follow_exact_layouts']),
 ('12.2.orca-adaptive-dynamic-poststate', ['dex-router/crates/protocol-math/src/orca_adaptive_swap.rs','dex-router/crates/protocol-math/src/orca_adaptive_online.rs'], ['adaptive_dynamic_kernel_mixed_layouts_preserve_exact_math_and_poststate','adaptive_dynamic_kernel_requires_three_complete_arrays_and_known_layouts','adaptive_dynamic_kernel_original_twenty_cases_full_poststate','adaptive_dynamic_original_normal_dispatch_all_twenty_banks_and_full_sequences']),
 ('12.9.orca-adaptive-dynamic-shared-pool', ['dex-router/crates/optimizer/tests/orca_adaptive_dynamic_shared.rs'], ['carried_dynamic_mixed_arrays_rewards_and_oracle_are_reused_by_shared_allocations']),
 ('12.14.orca-adaptive-dynamic-reference', ['integration/tools/reference/orca-adaptive-dynamic-service/src/main.rs','integration/tools/ci/orca-adaptive-dynamic-service-golden.py','integration/tools/acceptance/orca_adaptive_dynamic_evidence.py'], ['test_dynamic_source_corruptions_change_original_account_bytes','test_all_current_inputs_and_normal_source_recovery_checks_are_required']),
]:
 BINDINGS[clause]={'paths':paths,'tests':tests,'evidence_refs':['integration/artifacts/local/orca-adaptive-dynamic-acceptance.json']}

# Independent execution retains explicit local-state authority.
BINDINGS['14.1.evm-bound-transaction']={'paths': ['dex-router/crates/execution-plan/src/bound_v2.rs'], 'tests': ['forward_and_reverse_bindings_preserve_distinct_caller_recipient_and_exact_calldata', 'bound_transaction_tampering_is_rejected_even_with_recomputed_binding_hash'], 'evidence_refs': ['integration/artifacts/local/evm-execution-acceptance.json']}
BINDINGS['14.2.evm-strict-local-state']={'paths': ['dex-router/crates/simulation/src/revm_backend/db.rs', 'dex-router/crates/simulation/src/evm_fixture.rs'], 'tests': ['declared_zero_preserved_and_undeclared_slot_unknown', 'requested_chain_block_and_sender_context_reach_actual_opcodes'], 'evidence_refs': ['integration/artifacts/local/evm-execution-acceptance.json']}
BINDINGS['14.3.evm-real-balance-deltas']={'paths': ['dex-router/crates/simulation/src/evm_fixture.rs', 'dex-router/crates/simulation/tests/evm_native.rs'], 'tests': ['original_ten_transactions_execute_captured_state_and_measure_real_token_deltas', 'bound_original_v2_cases_use_exact_calldata_and_actual_balance_evidence', 'real_swap_spending_gas_does_not_need_another_probe_prepayment', 'out_of_gas_receipt_survives_zero_native_balance_after_real_execution', 'observation_preserves_real_caller_balance_gasprice_nonce_without_prepayment'], 'evidence_refs': ['integration/artifacts/local/evm-execution-acceptance.json']}
BINDINGS['14.4.evm-bounded-worker']={'paths': ['dex-router/crates/simulation/src/worker.rs', 'dex-router/crates/quote-api/src/local_simulation.rs'], 'tests': ['supervisor_hard_deadline_reaps_child', 'http_drop_kills_worker_releases_owned_permit_and_keeps_cache_unlocked', 'nested_memory_ceiling_cannot_be_swallowed_by_parent_call'], 'evidence_refs': ['integration/artifacts/local/evm-execution-acceptance.json']}
BINDINGS['14.5.evm-execution-evidence']={'paths': ['dex-router/crates/quote-api/src/lib.rs', 'dex-router/crates/quote-api/src/local_simulation.rs'], 'tests': ['post_execution_freshness_is_rechecked_and_evidence_preserves_quote_data', 'simulation_rejects_expired_invalidated_and_mismatched_source_quotes'], 'evidence_refs': ['integration/artifacts/local/evm-execution-acceptance.json']}

# Full V2 Swap write set remains independent of reserve-only legacy profiles.
BINDINGS['11.3.v2-complete-state']={'paths': ['common/protoc/proto/dex/v1/state.proto', 'chain-parser-go/internal/domain/v2_swap.go', 'dex-router/crates/domain/src/v2_swap.rs'], 'tests': ['TestV2CompleteMandatoryJSONAndWireValuesCannotDefaultUnknownToZero', 'complete_v2_mandatory_zero_valued_json_and_wire_fields_have_presence'], 'evidence_refs': ['integration/artifacts/local/v2-complete-acceptance.json']}
BINDINGS['7.3.v2-complete-fixed-bank']={'paths': ['chain-parser-go/internal/chain/evm/v2_complete.go', 'chain-parser-go/internal/chain/evm/adapter.go', 'chain-parser-go/internal/chain/evm/facts.go'], 'tests': ['TestV2CompleteAllNativeBanksIncludingReverts', 'TestV2CompleteOriginalSwapFactsKeepBothDonatedInputs', 'TestV2CompleteTimestampConversionIndependentOfProfile'], 'evidence_refs': ['integration/artifacts/local/v2-complete-acceptance.json']}
BINDINGS['12.2.v2-complete-settlement']={'paths': ['dex-router/crates/protocol-math/src/v2_complete.rs', 'dex-router/crates/protocol-math/src/lib.rs'], 'tests': ['original_twenty_swaps_and_four_reverts_match_explicit_clock_settlement', 'original_seventeen_aligned_clock_swaps_match_complete_normal_dispatch_poststate'], 'evidence_refs': ['integration/artifacts/local/v2-complete-acceptance.json']}
BINDINGS['12.9.v2-complete-shared-state']={'paths': ['dex-router/crates/optimizer/tests/v2_complete_shared.rs'], 'tests': ['original_same_order_chunks_reuse_complete_donation_state_in_shared_allocation', 'original_eight_fixed_clock_path_steps_preserve_all_pool_writes'], 'evidence_refs': ['integration/artifacts/local/v2-complete-acceptance.json']}
BINDINGS['14.v2-complete-independent-execution']={'paths': ['dex-router/crates/execution-plan/src/bound_v2.rs', 'dex-router/crates/simulation/src/evm_fixture.rs', 'dex-router/crates/simulation/tests/v2_complete.rs'], 'tests': ['original_24_transactions_match_real_gas_eth_nonces_code_and_all_declared_slots', 'complete_bound_executes_16_service_cases_and_observes_real_pair_poststate'], 'evidence_refs': ['integration/artifacts/local/v2-complete-acceptance.json']}
BINDINGS['12.14.v2-complete-original-bank']={'paths': ['integration/tools/reference/v2-complete/generate.cjs', 'integration/tools/reference/v2-complete/validate.cjs', 'integration/tools/acceptance/v2_complete_bank.py'], 'tests': ['test_real_collector_contains_all_original_scripts_packages_traces_and_service_copies', 'test_actual_source_getters_include_raw_cumulative_lock_and_factory'], 'evidence_refs': ['integration/artifacts/local/v2-complete-acceptance.json']}

# Published Managed V2 development remains partial until the final frozen gate.
BINDINGS['11.3.managed-v2-published-state'] = {
    'paths': ['common/protoc/proto/dex/v1/state.proto', 'chain-parser-go/internal/domain/managed_v2_state.go', 'dex-router/crates/domain/src/managed_v2_state.rs'],
    'tests': ['TestManagedV2NativeWireGolden', 'TestManagedV2VaultHistoryUint32'],
    'evidence_refs': ['integration/artifacts/local/managed-v2-aum-wire-check.log'],
}
BINDINGS['7.3.managed-v2-fixed-bank'] = {
    'paths': ['chain-parser-go/internal/chain/evm/managed_v2.go', 'chain-parser-go/internal/chain/evm/managed_v2_identity.go', 'chain-parser-go/internal/chain/evm/managed_v2_projection_test.go'],
    'tests': ['TestManagedV2NativePublicProjectionMatrix', 'TestManagedV2EveryNativeSnapshot'],
    'evidence_refs': ['integration/artifacts/local/managed-v2-aum-reference-check.log'],
}
BINDINGS['12.14.managed-v2-original-replay'] = {
    'paths': ['integration/tools/reference/balancer-managed-v2-aum/generate.cjs', 'integration/tools/reference/balancer-managed-v2-aum/validate.cjs', 'integration/tools/ci/managed-v2-aum-guards.py'],
    'tests': ['test_all_original_cases_and_two_authorities', 'test_all_original_compiler_builds_dependencies_and_independent_copies_are_bound'],
    'evidence_refs': ['integration/artifacts/local/managed-v2-aum-guards.log', 'integration/artifacts/local/managed-v2-aum-guards-optimized.log'],
}
BINDINGS['12.9.managed-v2-derived-context'] = {
    'paths': ['dex-router/crates/protocol-math/src/managed_v2_pool.rs', 'dex-router/crates/optimizer/tests/managed_v2_shared.rs'],
    'tests': ['shared_managed_v2_allocations_carry_full_cash_managed_and_do_not_collect_bpt'],
    'evidence_refs': ['#plan-2026-11-03-managed-v2-aum'],
}
BINDINGS['23.managed-v2-normal-service'] = {
    'paths': ['integration/tools/acceptance/managed_v2_smoke.py', 'integration/tools/acceptance/weighted_evidence.py'],
    'tests': ['test_each_fault_changes_an_actual_recorded_dependency_response', 'test_every_read_requires_exact_canonical_hash'],
    'evidence_refs': ['integration/artifacts/local/managed-v2-aum-three-service-smoke.json'],
}

BINDINGS['12.11.managed-v2-distinct-pause'] = {'paths': ['chain-parser-go/internal/chain/evm/managed_v2.go', 'dex-router/crates/protocol-math/src/managed_v2_pool.rs'], 'tests': ['TestManagedV2VaultPauseNeverOverwritesPoolOwnPause', 'test_original_pool_and_vault_pauses_and_windows_remain_distinct'], 'evidence_refs': ['integration/artifacts/local/managed-v2-aum-vault-pause-native-probe.log']}

# Current-date catalog and shared physical Vault execution remain a distinct new verification scope.
BINDINGS['7.3.managed-v2-current-catalog']={'paths':['chain-parser-go/internal/chain/evm/managed_v2_catalog_test.go','integration/tools/ci/managed-v2-aum-catalog-golden.cjs','integration/tools/acceptance/managed_v2_catalog.py'],'tests':['TestManagedV2CatalogAllOriginalBanks','fifty_swap','fifty_collect'],'evidence_refs':['integration/test/contracts/managed-v2-aum-catalog.json','integration/artifacts/local/managed-v2-catalog-three-service-smoke.json']}
BINDINGS['11.3.managed-v2-shared-vault-facts']={'paths':['chain-parser-go/internal/domain/managed_v2_dependencies.go','dex-router/crates/domain/src/managed_v2_dependencies.rs'],'tests':['TestManagedSharedRolesAndProviderGetterMustAgreeAtSameSource','original_shared_wallet_backs_total_cash_in_every_declared_registration'],'evidence_refs':['integration/test/contracts/managed-v2-shared-vault.json','integration/artifacts/local/managed-v2-shared-role-cash-go.log']}
BINDINGS['12.9.managed-v2-shared-wallet-transition']={'paths':['dex-router/crates/routing/src/shared_vault.rs','dex-router/crates/routing/src/lib.rs'],'tests':['native_four_swaps_full_poststate','carried_four_steps_full_poststate_and_two_paths','whole_path_failure_rolls_back_first_success'],'evidence_refs':['integration/artifacts/local/managed-v2-shared-funded-scope-regressions.log']}
BINDINGS['13.managed-v2-executable-scope']={'paths':['dex-router/crates/state-replica/src/lib.rs','dex-router/crates/quote-api/src/lib.rs','dex-router/crates/routing/src/lib.rs'],'tests':['mixed_vault_is_explicitly_excluded_without_blocking_unrelated_routes','mixed_vault_publish_keeps_closed_bank_and_only_unrelated_edges_in_either_order'],'evidence_refs':['integration/artifacts/local/managed-v2-shared-funded-scope-regressions.log']}
BINDINGS['12.14.managed-v2-real-selected-split']={'paths':['integration/tools/acceptance/managed_v2_multi_smoke.py','integration/tools/ci/managed-v2-shared-plan-replay.cjs'],'tests':['test_two_ordered_routes_share_vault_and_carry_real_wallet_state','test_single_pool_a_and_b_at_common_h38_and_reverse_at_h40'],'evidence_refs':['integration/artifacts/local/managed-v2-shared-three-service-smoke.json']}

# T34.11 remains partial until the original source, API/VM and final gate share
# one frozen closure. These bindings identify local tests only.
BINDINGS['11.3.managed-v2-scheduled-state'] = {
    'paths': ['common/protoc/proto/dex/v1/state.proto', 'chain-parser-go/internal/domain/managed_v2_schedule.go', 'dex-router/crates/domain/src/managed_v2_schedule.rs'],
    'tests': ['all_234_original_getter_views_and_41_checkpoints_are_exact'],
    'evidence_refs': ['integration/artifacts/local/managed-v2-scheduled-wire-check.log'],
}
BINDINGS['7.3.managed-v2-scheduled-source'] = {
    'paths': ['chain-parser-go/internal/chain/evm/managed_v2_scheduled.go', 'chain-parser-go/internal/chain/evm/managed_v2_scheduled_native_test.go', 'integration/tools/acceptance/managed_v2_scheduled_projection.py'],
    'tests': ['TestManagedScheduledEveryOriginalSnapshot', 'TestManagedScheduledOriginalFaultsNeverPublish', 'TestManagedV2RejectsForgedFixedFeeIDContext'],
    'evidence_refs': ['integration/artifacts/local/managed-v2-scheduled-parser-test.log'],
}
BINDINGS['12.9.managed-v2-scheduled-transition'] = {
    'paths': ['dex-router/crates/protocol-math/src/managed_v2_pool.rs', 'dex-router/crates/protocol-math/src/managed_v2_swap.rs', 'dex-router/crates/routing/src/shared_vault.rs'],
    'tests': ['all_53_original_wallet_outputs_and_full_swap_poststates_match', 'both_original_nonzero_aum_collections_match_every_poststate_field', 'future_swap_reclocks_all_three_peers_with_own_getters_and_ledgers'],
    'evidence_refs': ['integration/artifacts/local/managed-v2-scheduled-math-test.log'],
}
BINDINGS['12.14.managed-v2-scheduled-original'] = {
    'paths': ['integration/tools/ci/managed-v2-scheduled-golden.cjs', 'integration/tools/reference/balancer-managed-v2-scheduled/generate.cjs', 'integration/tools/ci/managed-v2-scheduled-plan-replay.cjs'],
    'tests': ['test_exact_original_plan_accepts_and_mutations_fail_before_execution'],
    'evidence_refs': ['integration/artifacts/local/managed-v2-scheduled-reference-check.log'],
}

# T34.11.6 original two-physical-pool scheduled scope remains separate from
# single-pool evidence and from full protocol/design acceptance.
BINDINGS['7.3.managed-v2-scheduled-shared-source'] = {
    'paths': ['chain-parser-go/internal/chain/evm/managed_v2_scheduled_shared_native_test.go', 'integration/tools/reference/balancer-managed-v2-scheduled-shared/checker.cjs', 'integration/tools/acceptance/managed_v2_scheduled_shared_projection.py'],
    'tests': ['TestManagedScheduledSharedEveryOriginalCommonBank', 'TestManagedScheduledSharedMissingPeerIsAtomicAndLegacyCannotPromote', 'all_72_common_banks_preserve_full_sources_profiles_and_joint_cash_constraints'],
    'evidence_refs': ['integration/artifacts/local/managed-v2-scheduled-shared-parser-green.log', 'integration/artifacts/local/managed-v2-scheduled-shared-local-validation.json'],
}
BINDINGS['12.9.managed-v2-scheduled-shared-original-transitions'] = {
    'paths': ['dex-router/crates/routing/tests/managed_v2_scheduled_shared_native.rs', 'dex-router/crates/routing/src/shared_vault.rs', 'dex-router/crates/protocol-math/src/managed_v2_pool.rs'],
    'tests': ['all_48_swaps_and_four_collections_match_both_full_original_pool_poststates', 'twelve_carried_abab_and_final_collection_sequences_never_reset_source_or_wallets', 'all_twelve_native_clock_only_transitions_reclock_each_own_schedule_and_aum'],
    'evidence_refs': ['integration/artifacts/local/managed-v2-scheduled-shared-math-native.log', 'integration/artifacts/local/managed-v2-scheduled-shared-local-validation.json'],
}
BINDINGS['12.14.managed-v2-scheduled-shared-selected-split'] = {
    'paths': ['integration/tools/ci/managed-v2-scheduled-shared-plan-replay.cjs', 'integration/tools/acceptance/managed_v2_scheduled_shared_projection.py', 'dex-router/crates/routing/tests/managed_v2_scheduled_shared_native.rs'],
    'tests': ['test_real_validator_rejects_plan_mismatches_before_any_vm', 'actual_api_selected_two_route_full_poststates'],
    'evidence_refs': ['integration/artifacts/local/managed-v2-scheduled-shared-actual-selected-model.log', 'integration/artifacts/local/managed-v2-scheduled-shared-local-validation.json'],
}
BINDINGS['23.managed-v2-scheduled-shared-process'] = {
    'paths': ['integration/tools/acceptance/managed_v2_scheduled_shared_smoke.py', 'integration/tools/acceptance/weighted_evidence.py'],
    'tests': ['test_every_common_bank_has_exact_canonical_ancestry_and_all_original_reads', 'test_full_reference_and_two_physical_route_conservation_are_required', 'test_raw_word_fault_targets_an_explicit_scheduled_profile_in_each_mode'],
    'evidence_refs': ['integration/artifacts/local/managed-v2-scheduled-shared-three-service-smoke.json', 'integration/artifacts/local/managed-v2-scheduled-shared-local-validation.json'],
}

# Published BPT adapter: source/test bindings only; status stays in Markdown.
BINDINGS['11.3.managed-v2-bpt-state'] = {
    'paths': ['common/protoc/proto/dex/v1/state.proto', 'chain-parser-go/internal/domain/managed_v2_bpt_state.go', 'dex-router/crates/domain/src/managed_v2_bpt_state.rs'],
    'tests': ['TestManagedV2BptIndependentCPPWire', 'independent_bpt_cpp_snapshot_preserves_full_wire_and_known_zero_false'],
    'evidence_refs': ['dex-router/crates/domain/tests/managed_v2_bpt_state.rs'],
}
BINDINGS['7.3.managed-v2-bpt-source'] = {
    'paths': ['chain-parser-go/internal/chain/evm/managed_v2.go', 'chain-parser-go/internal/config/managed_v2.go'],
    'tests': ['TestManagedV2BptEverySupportedOriginalFixedBank', 'TestManagedV2BptSourceControlsFailAtomically'],
    'evidence_refs': ['chain-parser-go/internal/chain/evm/managed_v2_bpt_native_test.go'],
}
BINDINGS['12.2.managed-v2-bpt-transition'] = {
    'paths': ['dex-router/crates/protocol-math/src/managed_v2_bpt.rs', 'dex-router/crates/protocol-math/src/managed_v2_pool.rs'],
    'tests': ['original_bpt_swap_amounts_and_every_retained_poststate_field_match', 'recipient_wallet_aliases_move_preminted_bpt_once_without_extra_minting'],
    'evidence_refs': ['dex-router/crates/protocol-math/tests/managed_v2_bpt_pool.rs'],
}
BINDINGS['12.9.managed-v2-bpt-routing'] = {
    'paths': ['dex-router/crates/routing/src/lib.rs', 'dex-router/crates/routing/src/bpt_wallets.rs', 'dex-router/crates/optimizer/src/lib.rs', 'dex-router/crates/quote-api/src/lib.rs'],
    'tests': ['original_bpt_logical_edges_and_funded_execution_match_every_supported_swap', 'external_v2_bpt_credit_is_carried_into_manager_and_collector_exit_balances', 'bpt_http_caller_membership_is_not_lost_between_search_and_optimizer'],
    'evidence_refs': ['dex-router/crates/routing/tests/managed_v2_bpt.rs'],
}
BINDINGS['12.14.managed-v2-bpt-selected'] = {
    'paths': ['integration/tools/ci/managed-v2-bpt-plan-replay.cjs', 'integration/tools/acceptance/managed_v2_bpt_projection.py'],
    'tests': ['test_exact_api_plan_input_and_source_boundaries', 'actual_api_selected_bpt_native_full_poststate'],
    'evidence_refs': ['integration/tools/ci/managed-v2-bpt-plan-replay.cjs'],
}
BINDINGS['23.managed-v2-bpt-process'] = {
    'paths': ['integration/tools/acceptance/managed_v2_bpt_smoke.py', 'integration/tools/acceptance/weighted_evidence.py'],
    'tests': ['test_exact_original_control_getters_and_canonical_anchor', 'test_fault_serving_is_private_and_recovers'],
    'evidence_refs': ['integration/tools/acceptance/managed_v2_bpt_smoke.py'],
}

BINDINGS['11.3.managed-v2-breaker-state'] = {
    'paths': ['common/protoc/proto/dex/v1/state.proto', 'chain-parser-go/internal/domain/managed_v2_circuit_state.go', 'dex-router/crates/domain/src/managed_v2_circuit_state.rs'],
    'tests': ['TestManagedV2CircuitIndependentCPPField42', 'independent_breaker_cpp_field42_retains_full_word_and_every_required_getter'],
    'evidence_refs': ['dex-router/crates/domain/tests/managed_v2_circuit_state.rs'],
}
BINDINGS['7.3.managed-v2-breaker-source'] = {
    'paths': ['chain-parser-go/internal/chain/evm/managed_v2_circuit.go', 'chain-parser-go/internal/config/managed_v2.go'],
    'tests': ['TestManagedV2BreakerEveryOriginalCompleteFixedBank', 'TestManagedV2BreakerSourceContradictionsAreAtomic'],
    'evidence_refs': ['chain-parser-go/internal/chain/evm/managed_v2_breaker_native_test.go'],
}
BINDINGS['12.2.managed-v2-breaker-transition'] = {
    'paths': ['dex-router/crates/protocol-math/src/managed_v2_breaker.rs', 'dex-router/crates/protocol-math/src/managed_v2_pool.rs'],
    'tests': ['original_breaker_swaps_match_exact_amounts_complete_poststate_or_original_rejection', 'future_execution_aum_can_trip_untouched_token_even_when_bpt_output_is_zero'],
    'evidence_refs': ['dex-router/crates/protocol-math/tests/managed_v2_breaker_pool.rs'],
}
BINDINGS['12.9.managed-v2-breaker-routing'] = {
    'paths': ['dex-router/crates/routing/src/lib.rs', 'dex-router/crates/protocol-math/src/multi_asset.rs', 'dex-router/crates/quote-api/src/lib.rs'],
    'tests': ['candidate_search_keeps_currently_tripped_pool_for_recovering_amount', 'original_shared_order_reuses_both_complete_physical_pools_and_wallets'],
    'evidence_refs': ['dex-router/crates/routing/tests/managed_v2_breaker.rs'],
}
BINDINGS['12.14.managed-v2-breaker-reference'] = {
    'paths': ['integration/tools/reference/balancer-managed-v2-breaker/checker.cjs', 'integration/tools/reference/balancer-managed-v2-breaker/scenarios.cjs'],
    'tests': ['test_native_account_control_clock_and_scenario_deletion_fail_closed', 'test_invalid_manager_control_outcomes_are_fixed_independently_of_observations'],
    'evidence_refs': ['integration/tools/reference/balancer-managed-v2-breaker/checker.cjs'],
}
BINDINGS['23.managed-v2-breaker-process'] = {
    'paths': ['integration/tools/acceptance/managed_v2_breaker_smoke.py', 'integration/tools/ci/managed-v2-breaker-plan-replay.cjs'],
    'tests': ['test_late_503_is_rejected_even_if_query_started_within_deadline', 'test_selected_after_bindings_and_input_links_reject_offline_forgery'],
    'evidence_refs': ['integration/tools/acceptance/managed_v2_breaker_smoke.py'],
}

# Final evidence closure is part of CB5, including progress-count qualification.
BINDINGS['23.managed-v2-breaker-process']['paths'].extend([
    'integration/tools/acceptance/managed_v2_breaker_acceptance.py',
    'integration/tools/acceptance/managed_v2_breaker_final.py',
    'integration/tools/progress_document.py',
])
BINDINGS['23.managed-v2-breaker-process']['tests'].extend([
    'test_serial_binds_the_exact_current_freeze_commands_and_outputs',
    'test_review_requires_every_scope_file_and_independent_reviewer',
    'test_failed_or_interrupted_audit_never_leaves_an_old_success',
    'test_unverified_reports_keep_fingerprint_but_contribute_no_verified_counts',
    'test_exact_current_plan_and_nonempty_unique_cb_inventory_are_required',
    'test_every_breaker_design_mapping_is_required_exactly_once',
    'test_progress_change_after_any_stage_stops_and_invalidates_acceptance',
])
BINDINGS['23.managed-v2-breaker-process']['evidence_refs'].append(
    'integration/artifacts/local/managed-v2-breaker-local-acceptance.json')

# R01–R04 bindings; progress remains in the Markdown ledger.
BINDINGS['7.1.r01-deployment-evidence'] = {'paths': ['chain-parser-go/internal/provenance/evm.go', 'chain-parser-go/internal/provenance/capture.go', 'chain-parser-go/cmd/source-evidence/main.go'], 'tests': ['TestProofRejectsChangedHeaderTransactionsCodeAndAccountFields', 'TestCaptureBindsBothHeadersAndPinsEveryStateRead'], 'evidence_refs': ['chain-parser-go/internal/provenance/evm_test.go']}
BINDINGS['12.14.r01-deployed-source-rebuild'] = {'paths': ['integration/tools/acceptance/evm_deployment_rebuild.cjs'], 'tests': ['test_rebuilt_runtime_and_exact_source_inventory_must_both_match', 'test_entry_rejects_unproven_deployment_before_reporting_bound_address'], 'evidence_refs': ['integration/tools/acceptance/tests/test_evm_deployment_rebuild.py']}
BINDINGS['7.1.r02-provider-ws'] = {'paths': ['chain-parser-go/internal/rpc/client.go', 'chain-parser-go/internal/rpc/capabilities.go', 'chain-parser-go/internal/rpc/heads_ws.go'], 'tests': ['TestProbeCapabilitiesUsesHistoricalHashAndDoesNotInventRange', 'TestWSRealHandshakeUntrustedHeadsReconnectAndCancel'], 'evidence_refs': ['chain-parser-go/internal/rpc/policy_test.go']}
BINDINGS['7.2.r02-staging-commit'] = {'paths': ['chain-parser-go/internal/ingest/pipeline.go'], 'tests': ['TestOrderedActorStagesHPlusOneAndBoundsWindow', 'TestCriticalFailureNeverCrossesGapAndKeepsRawDLQ'], 'evidence_refs': ['chain-parser-go/internal/ingest/pipeline_test.go']}
BINDINGS['7.3.r02-raw-dlq-discovery'] = {'paths': ['chain-parser-go/internal/ingest/archive.go', 'chain-parser-go/internal/ingest/factory.go', 'chain-parser-go/internal/service/raw_evidence.go'], 'tests': ['TestArchiveDurableOwnershipDLQAndCapacity', 'TestNormalRPCResponseIsArchivedBeforeDecodeFailureAndSurvivesReopen', 'TestFactoryFailedCurrentReadsAndHistoryCannotOverwriteOnline'], 'evidence_refs': ['chain-parser-go/internal/ingest/archive_test.go']}
BINDINGS['7.r03-yellowstone-wal'] = {'paths': ['chain-parser-rust/crates/solana-ingest/src/yellowstone.rs', 'chain-parser-rust/crates/solana-ingest/src/stream.rs'], 'tests': ['real_grpc_subscription_retains_exact_invalid_protobuf_before_decode_and_rejects_end', 'capture_is_durable_before_decoder_and_decode_failure_is_replayable'], 'evidence_refs': ['chain-parser-rust/crates/solana-ingest/tests/stream_wal.rs']}
BINDINGS['28.22.r03-stream-bank-barrier'] = {'paths': ['chain-parser-rust/crates/solana-ingest/src/stream.rs'], 'tests': ['native_full_block_is_an_atomic_provider_event_set_without_a_bank_scope_proof', 'manifest_binds_exact_account_state_not_only_a_write_version_number'], 'evidence_refs': ['chain-parser-rust/crates/solana-ingest/tests/stream.rs']}
BINDINGS['28.26.r04-ensure-pool-scope'] = {'paths': ['chain-parser-go/internal/service/scope_control.go', 'chain-parser-rust/crates/solana-ingest/src/service.rs'], 'tests': ['TestEnsurePoolScopeUsesCurrentCompleteGenerationAndRejectsInvalidScopes', 'authenticated_scope_control_uses_current_complete_generation'], 'evidence_refs': ['chain-parser-go/internal/service/scope_control_test.go']}
BINDINGS['28.10.r04-current-baseline-recovery'] = {'paths': ['chain-parser-go/internal/bootstrap/scope.go', 'chain-parser-go/internal/bootstrap/scope_store.go', 'chain-parser-rust/crates/solana-ingest/src/scope.rs', 'chain-parser-rust/crates/solana-ingest/src/scope_store.rs'], 'tests': ['TestScopeRecoveryAtomic', 'file_store_killed_process_reopens_without_old_ready'], 'evidence_refs': ['chain-parser-rust/crates/solana-ingest/tests/scope_recovery.rs']}
BINDINGS['28.26.r04-history-isolation'] = {'paths': ['chain-parser-go/internal/bootstrap/scope_store.go', 'chain-parser-rust/crates/solana-ingest/src/scope_store.rs'], 'tests': ['file_history_is_durable_separate_and_bounded'], 'evidence_refs': ['chain-parser-rust/crates/solana-ingest/tests/scope_recovery.rs']}

BINDINGS['7.1.r02-provider-ws']['paths'].extend([
    'chain-parser-go/internal/service/ws_lifecycle.go',
    'chain-parser-go/internal/app/app.go',
    'chain-parser-go/internal/config/config.go',
])
BINDINGS['7.1.r02-provider-ws']['tests'].extend([
    'TestAuditNormalWSHasHTTPRateBudget',
    'TestWebsocketConfigHasExplicitTransportAndRateResourceBounds',
])
BINDINGS['7.2.r02-staging-commit']['paths'].append('chain-parser-go/internal/service/staging.go')
BINDINGS['7.2.r02-staging-commit']['tests'].append('TestNormalServiceStagingCommitsCompleteBlocksInOrder')
BINDINGS['7.r03-yellowstone-wal']['paths'].extend([
    'chain-parser-rust/crates/solana-ingest/src/async_frame_wal.rs',
    'chain-parser-rust/cmd/source-evidence/main.rs',
])
BINDINGS['28.26.r04-ensure-pool-scope']['tests'].append(
    'cancelled_http_waiter_cannot_release_actor_before_commit_result_settles')

BINDINGS['28.r01-r04-validation'] = {
    'paths': ['integration/Makefile', 'integration/tools/ci/r01_r04_traceability.py'],
    'tests': ['test_current_plan_maps_all_requested_work_without_claiming_completion',
              'test_deleting_or_duplicate_task_or_design_fails',
              'test_unknown_related_design_is_not_silently_ignored'],
    'evidence_refs': ['integration/tools/ci/tests/test_r01_r04_traceability.py'],
}

# R01–R04 follow-up: executable design targets and normal Ethereum commitments.
BINDINGS['7.1.r01-deployment-evidence']['paths'].extend([
    'integration/config/chain-targets.json',
    'integration/tools/configuration/chain_target_config.py',
])
BINDINGS['7.1.r01-deployment-evidence']['tests'].extend([
    'test_design_target_matrix_is_exact_and_no_target_grants_protocol_admission',
    'test_solana_pins_complete_genesis_hash_instead_of_caip_prefix',
    'test_existing_same_chain_genesis_pin_cannot_be_replaced_by_new_provider',
])
BINDINGS['7.2.r02-staging-commit']['paths'].extend([
    'chain-parser-go/internal/chain/evm/block_integrity.go',
    'chain-parser-go/internal/config/config.go',
])
BINDINGS['7.2.r02-staging-commit']['tests'].extend([
    'TestOnlineEthereumBlockVerifiesRetainedRealHeaderAndTransactionRoot',
    'TestEthereumBlockIntegrityConfigRejectsUnknownOrWrongChainProfiles',
    'TestNormalEthereumServiceIntegrityFailureRevokesScopeAndRetainsRawDLQ',
])

BINDINGS['28.r01-r04-validation']['tests'].extend([
    'test_gap_audit_covers_every_task_and_distinguishes_engineering_from_verification',
    'test_missing_duplicate_unknown_or_empty_gap_audit_rows_fail',
])

BINDINGS['7.1.r01-deployment-evidence']['paths'].append(
    'integration/test/source/chain-genesis-observations.json')
BINDINGS['7.1.r01-deployment-evidence']['tests'].extend([
    'test_every_mainnet_target_rejects_same_chain_id_with_another_genesis',
    'test_frozen_genesis_pins_match_retained_identity_observations',
    'test_retained_real_genesis_evidence_can_generate_every_target_without_live_rpc',
    'test_retained_identity_evidence_requires_the_frozen_full_file_digest',
])
BINDINGS['7.1.r02-provider-ws']['paths'].append(
    'integration/tools/configuration/chain_target_config.py')
BINDINGS['7.1.r02-provider-ws']['tests'].extend([
    'test_same_chain_rpc_infrastructure_precedes_public_defaults',
    'test_probe_worker_has_an_absolute_deadline_even_while_making_progress',
    'test_automatic_raw_evidence_limits_cannot_emit_an_invalid_config',
])

# Public-data R01–R04 normal-service integration (2026-11-07).
BINDINGS['7.1.r01-deployment-evidence']['paths'].extend([
 'integration/tools/acceptance/r01_public_catalog.py',
 'integration/test/source/deployment-catalog/catalog.json',
])
BINDINGS['7.1.r01-deployment-evidence']['tests'].extend([
 'test_missing_catalog_is_explicit_failure',
 'test_scope_duplicate_and_omission_fail_closed',
 'test_minimal_proxy_target_is_decoded_from_runtime',
])
BINDINGS['7.1.r02-provider-ws']['paths'].extend([
 'chain-parser-go/internal/service/provider_runtime.go',
 'chain-parser-go/internal/service/runtime_sources.go',
 'chain-parser-go/internal/config/provider_catalog.go',
])
BINDINGS['7.1.r02-provider-ws']['tests'].extend([
 'TestRuntimeSourcesFastHeadAdvanceDoesNotProbeHistoryOrExtendCapabilities',
 'TestRuntimeSourcesStateReadsUseQualifiedEndpointWithoutRemovingWitnesses',
])
BINDINGS['7.2.r02-staging-commit']['paths'].extend([
 'chain-parser-go/internal/chain/evm/chain_receipts.go',
 'chain-parser-go/internal/chain/evm/chain_receipts_body.go',
 'chain-parser-go/internal/chain/evm/receipt_bulk.go',
])
BINDINGS['7.2.r02-staging-commit']['tests'].extend([
 'TestChainReceiptsPublicVectors',
 'TestChainReceiptsRejectPublicMutations',
 'TestCurrentScopeStateSharesOneRetainedPoolBudget',
])
BINDINGS['7.3.r02-raw-dlq-discovery']['paths'].extend([
 'chain-parser-go/internal/service/catalog_runtime.go',
 'chain-parser-go/internal/service/raw_maintenance.go',
 'chain-parser-go/internal/ingest/dlq_incident.go',
 'chain-parser-go/internal/ingest/archive_failure_safety.go',
 'chain-parser-go/internal/reducer/owner_records.go',
])
BINDINGS['7.3.r02-raw-dlq-discovery']['tests'].extend([
 'TestRuntimeSourcesContinuousQueueCoversEveryCommittedBlockInsideProbeInterval',
 'TestRawMaintenanceUnknownReopenReadsImmutableOwnerReceipt',
 'TestOwnerRecordCapacityIsReservedDurablyBeforeKafkaAndCannotBeStolenByHistory',
])
BINDINGS['7.r03-yellowstone-wal']['paths'].extend([
 'chain-parser-rust/crates/solana-ingest/src/normal_stream.rs',
 'chain-parser-rust/crates/solana-ingest/src/bank_manifest.rs',
])
BINDINGS['7.r03-yellowstone-wal']['tests'].append(
 'normal_public_grpc_publishes_only_complete_bank_then_revokes_and_restarts_head_first')
BINDINGS['28.22.r03-stream-bank-barrier']['paths'].append(
 'chain-parser-rust/crates/solana-ingest/src/manifest_recovery.rs')
BINDINGS['28.22.r03-stream-bank-barrier']['tests'].append(
 'independent_manifest_version_dependency_digest_and_freshness_are_required')
BINDINGS['28.26.r04-ensure-pool-scope']['paths'].append(
 'chain-parser-go/internal/service/scope_owner_recovery.go')
BINDINGS['28.26.r04-ensure-pool-scope']['tests'].extend([
 'TestNormalStartupRestoresBoundedFirstScopeAndEnsureExpandsAdmittedPools',
 'TestFailedExpansionRetainsUnchangedFreshQualifiedScope',
])
BINDINGS['28.10.r04-current-baseline-recovery']['paths'].extend([
 'chain-parser-go/internal/bootstrap/embedded_scope.go',
 'chain-parser-go/internal/reducer/scope_commit.go',
 'chain-parser-go/internal/reducer/owner_transaction.go',
])
BINDINGS['28.10.r04-current-baseline-recovery']['tests'].extend([
 'TestNormalBaselineRecoveryCommitsScopeLedgerWithOwnerAndRestartReanchors',
 'TestScopeAmbiguousLocalCommitRequiresReopenBeforeAnyRetry',
])
BINDINGS['28.26.r04-history-isolation']['paths'].extend([
 'chain-parser-go/internal/reducer/history_queue.go',
 'chain-parser-go/internal/service/history_runtime.go',
 'chain-parser-go/internal/service/history_lifecycle.go',
 'chain-parser-go/internal/chain/evm/history_facts.go',
])
BINDINGS['28.26.r04-history-isolation']['tests'].extend([
 'TestHistoryReviewCanonicalRevisionInvalidatesDoneAndClaim',
 'TestHistoryReviewDatasetRebindRevokesClaimAndPreservesOldOwnership',
 'TestNormalHistoryLifecycleRegistersGapsWithoutBlockingFirstScope',
])
BINDINGS['28.r01-r04-validation']['paths'].extend([
 'chain-parser-go/internal/service/recovery_metrics.go',
 'dex-router/crates/quote-api/src/recovery_metrics.rs',
])
BINDINGS['28.r01-r04-validation']['tests'].extend([
 'TestRecoveryMetricsRecordFirstQualifiedScopeAndDoNotWaitForHistory',
 'recovery_metrics_require_qualified_scope_and_a_real_successful_quote',
])

BINDINGS['28.10.r04-current-baseline-recovery']['tests'].extend([
 'TestHeadFirstRecoveryWorkDoesNotScaleWithOneHourOneDayOrThirtyDaysGap',
 'TestScopeRecoveryCapacityFailureNeverPublishesPartialReady',
])

BINDINGS['7.3.r02-raw-dlq-discovery']['paths'].extend([
 'chain-parser-go/internal/service/catalog_runtime_v3_template.go',
 'chain-parser-go/internal/service/raw_maintenance_responsibility.go',
 'chain-parser-go/internal/service/raw_maintenance_owner_unit.go',
])
BINDINGS['7.3.r02-raw-dlq-discovery']['tests'].extend([
 'TestRuntimeSourcesCompleteHookPreservesCloneAndVerifierBarrier',
 'TestRawMaintenanceActualProviderRefreshContinuousSmallSink',
])

BINDINGS['28.10.r04-current-baseline-recovery']['paths'].extend([
 'chain-parser-rust/crates/solana-ingest/src/owner_scope.rs',
 'chain-parser-rust/crates/solana-ingest/src/service_scope_runtime.rs',
 'chain-parser-rust/crates/solana-ingest/src/wal_segments.rs',
])
BINDINGS['28.22.r03-stream-bank-barrier']['paths'].extend([
 'chain-parser-rust/crates/bank-source/src/lib.rs',
 'chain-parser-rust/crates/bank-source/agave-4.3.0/native-observer.patch',
])

BINDINGS['7.r03-yellowstone-wal']['paths'].extend([
 'chain-parser-rust/crates/solana-ingest/src/owner_scope.rs',
 'chain-parser-rust/crates/bank-source/src/lib.rs',
 'integration/tools/acceptance/r03_authority_process.py',
])
BINDINGS['7.r03-yellowstone-wal']['tests'].append(
 'actual_postgres_kafka_scope_publish_revoke_restart_and_fence')
BINDINGS['28.r01-r04-validation']['paths'].extend([
 'integration/tools/acceptance/r01_r04_recovery_process.py',
 'integration/tools/acceptance/r03_authority_process.py',
])
BINDINGS['7.2.r02-staging-commit']['paths'].extend([
 'chain-parser-go/internal/config/native_proof.go',
 'chain-parser-go/internal/chain/evm/native_runtime.go',
])
BINDINGS['7.2.r02-staging-commit']['tests'].append(
 'TestNativeProofConfigurationRequiresKnownChainAndFiniteExplicitLimits')
BINDINGS['7.2.r02-staging-commit']['tests'].extend([
 'TestNormalNativeReadCompleteUsesActualPublicCommitments',
 'TestNormalNativeBudgetCannotResetBetweenBodyReceiptsAndFinalAnchor',
])
BINDINGS['28.26.r04-history-isolation']['paths'].append(
 'chain-parser-go/internal/service/native_history_proof.go')
BINDINGS['28.26.r04-history-isolation']['tests'].append(
 'TestNormalHistoryRuntimeInstallsNativeEvidenceResponsibilitySink')

BINDINGS['28.r01-r04-validation']['tests'].extend([
 'test_missing_duplicate_or_non_feature_artifacts_fail_closed',
 'test_non_test_artifact_cannot_satisfy_authority_scenario',
])
BINDINGS['7.3.r02-raw-dlq-discovery']['tests'].append(
 'TestRuntimeFactoryNormalArchivedNativeProofRetainsSettlingAuthority')

BINDINGS['7.1.r01-deployment-evidence']['paths'].append(
 'integration/tools/acceptance/r01_public_catalog_solana_process.py')
BINDINGS['28.22.r03-stream-bank-barrier']['paths'].append(
 'chain-parser-rust/crates/solana-ingest/src/token_program.rs')
BINDINGS['28.22.r03-stream-bank-barrier']['tests'].extend([
 'declared_upgradeable_classic_token_program_is_bound_in_normal_decoder',
 'token_upgrade_configuration_is_explicit_pinned_and_preserves_immutable_defaults',
])

BINDINGS['28.26.r04-history-isolation']['paths'].extend([
 'chain-parser-rust/crates/solana-ingest/src/history_queue.rs',
 'chain-parser-rust/crates/solana-ingest/src/history_runtime.rs',
])
BINDINGS['28.26.r04-history-isolation']['tests'].extend([
 'normal_tonic_history_http_commits_facts_without_changing_online_reference',
 'billion_slot_gap_uses_bounded_produced_slot_pages_after_online_is_live',
 'paused_disabled_and_slow_history_do_not_hold_online_actor_or_close',
 'huge_interval_is_one_durable_job_and_old_scope_claim_is_fenced',
 'trailing_absence_without_a_successor_header_is_unknown_not_skipped',
])
BINDINGS['7.2.r02-staging-commit']['paths'].append(
 'chain-parser-go/internal/chain/evm/chain_receipts_state_proof.go')
BINDINGS['7.2.r02-staging-commit']['tests'].extend([
 'TestContractStoragePublicNativeReplayAndSessionConsumer',
 'TestContractStorageOldBadPhysicalProofCannotBeReinterpreted',
 'TestContractStorageSessionCancellationAndRecorderFailureStayTerminal',
])

BINDINGS['7.1.r01-deployment-evidence']['paths'].extend([
 'integration/tools/acceptance/r01_getter_proofs.py',
 'integration/tools/acceptance/r01_getter_slots.cjs',
])
BINDINGS['7.1.r01-deployment-evidence']['tests'].extend([
 'test_complete_retained_original_vm_sload_set',
 'test_complete_replay_plan_matches_authenticated_read_set',
 'test_complete_new_replay_never_reacquires_public_state',
])
BINDINGS['28.r01-r04-validation']['paths'].append(
 'integration/tools/acceptance/r01_public_catalog_history.py')

BINDINGS.update({
 '12.10.multi-route-grid': {
  'paths':['dex-router/crates/optimizer/src/lib.rs','dex-router/crates/optimizer/src/search.rs','dex-router/crates/quote-api/src/lib.rs'],
  'tests':['three_and_four_way_splits_match_independent_integer_oracle_with_remainders_and_costs','eight_disjoint_routes_are_solved_on_the_complete_grid_with_finite_work','shared_eight_way_scope_seeds_larger_allocations_before_pairs_spend_the_budget','actual_quote_uses_three_physical_routes_and_reports_grid_search'],
  'evidence_refs':['integration/artifacts/local/core-quote-current-audit.json'],
 },
 '13.split-probe-ranking': {
  'paths':['dex-router/crates/routing/src/lib.rs','dex-router/crates/optimizer/tests/grid_oracle.rs'],
  'tests':['split_size_ranking_keeps_a_small_pool_that_full_order_ranking_discards','candidate_limit_retains_a_confirmed_full_order_fallback','allocation_work_limit_is_visible_even_when_candidate_search_finishes'],
  'evidence_refs':['integration/artifacts/local/core-quote-current-audit.json'],
 },
 '12.14.velodrome-v2': {
  'paths':['chain-parser-go/internal/chain/evm/stable.go','chain-parser-go/internal/domain/protocols.go','dex-router/crates/protocol-math/src/aerodrome.rs','integration/tools/ci/velodrome-v2-golden.cjs','integration/tools/acceptance/velodrome_smoke.py'],
  'tests':['TestVelodromeOriginalReferenceStatesPreserveVersionAndWire','TestVelodromeProfilesRequirePinnedFactoryAndImplementationIdentity','velodrome_both_models_match_original_solidity_outputs_and_post_balances','velodrome_original_sequences_preserve_fees_and_shared_pool_state','sequential_velodrome_allocations_match_original_post_state_in_both_directions'],
  'evidence_refs':['integration/artifacts/local/core-velodrome-process.json','integration/artifacts/local/core-quote-current-audit.json'],
 },
})

BINDINGS['12.14.lfj-v21']={
 'paths':['common/protoc/proto/dex/v1/state.proto','chain-parser-go/internal/chain/evm/lfj.go','chain-parser-go/internal/domain/lfj.go','dex-router/crates/domain/src/lfj.rs','dex-router/crates/protocol-math/src/lfj.rs','integration/tools/acceptance/lfj_smoke.py','integration/tools/ci/lfj-v21-plan-replay.cjs'],
 'tests':['TestLfjOriginalBanksReadEveryFeeBinOracleAtOneHash','TestLfjMissingBinsUnsyncedTokensAndBadSourcesNeverPublish','TestLfjIndependentCppWireCloneAndSourceBank','TestLfjJSONRequiredObservedZerosRemainDistinctFromMissingNullAndDuplicates','lfj_complete_original_swap_matches_outputs_parameters_fees_bins_and_oracle','lfj_shared_routes_keep_original_oracle_fees_and_all_bin_transitions','lfj_split_capacity_and_best_grid_match_independent_original_getter_curve'],
 'evidence_refs':['integration/artifacts/local/core-lfj-normal-process.json'],
}

BINDINGS['12.10.multi-route-grid']['evidence_refs'].extend(['integration/artifacts/local/core-quote-q1-local-463896a2b08e.json'])
BINDINGS['13.split-probe-ranking']['evidence_refs'].extend(['integration/artifacts/local/core-quote-q1-local-463896a2b08e.json'])
BINDINGS['12.14.velodrome-v2']['evidence_refs'].extend(['integration/artifacts/local/core-quote-q1-local-463896a2b08e.json', 'integration/artifacts/local/core-velodrome-local-q1-463896a2b08e.json'])
BINDINGS['12.14.lfj-v21']['evidence_refs'].extend(['integration/artifacts/local/core-lfj-local-q1-463896a2b08e.json'])

BINDINGS['12.14.slipstream-context'] = {
 'paths':['common/protoc/proto/dex/v1/state.proto','chain-parser-go/internal/chain/evm/slipstream_context.go','chain-parser-go/internal/chain/evm/slipstream_templates.go','chain-parser-rust/crates/domain/src/slipstream_context.rs','dex-router/crates/domain/src/slipstream_context.rs','dex-router/crates/protocol-math/src/slipstream_context.rs','integration/tools/acceptance/slipstream_context_smoke.py','integration/tools/ci/slipstream-context-plan-replay.cjs'],
 'tests':['TestSlipstreamContextOriginalBanksRetainEveryLedgerOracleAndCallerInput','TestSlipstreamContextBadSourcesAndRehashedTemplatesNeverPublish','independent_contextual_cpp_wire_preserves_every_origin_ledger_and_explicit_zero','required_contextual_wire_fields_and_canonical_source_flags_fail_closed','contradictory_readonly_module_factory_and_origin_facts_reject_shared_banks','contextual_original_swaps_match_full_curve_fee_gauge_reward_and_oracle_state','same_block_initial_fee_changes_and_twelve_original_carried_steps_are_exact','ordered_shared_allocations_use_actual_new_fee_after_each_original_oracle_write','contextual_failures_and_foreign_pool_wallets_never_install_partial_state'],
 'evidence_refs':['integration/artifacts/local/core-slipstream-context-normal-process.json','integration/artifacts/local/core-slipstream-context-negative-controls.json','integration/artifacts/local/core-quote-current-audit.json'],
}


BINDINGS['12.14.pancake-lm']={
 'paths':['common/protoc/proto/dex/v1/state.proto','chain-parser-go/internal/chain/evm/pancake_lm.go','chain-parser-go/internal/domain/pancake_types.go','chain-parser-go/internal/domain/pancake_validation.go','chain-parser-go/internal/domain/pancake_wire.go','chain-parser-go/internal/domain/pancake_json.go','dex-router/crates/domain/src/pancake_lm.rs','dex-router/crates/protocol-math/src/pancake_lm.rs','dex-router/crates/routing/src/lib.rs','dex-router/crates/optimizer/tests/pancake_lm_shared.rs','integration/tools/acceptance/pancake_lm_smoke.py','integration/tools/ci/pancake-v3-lm-plan-replay.cjs'],
 'tests':['TestPancakeLmOriginalGetterBanksPreserveFullLedgerAndRewards','TestPancakeLmMissingSourcesAndRehashedUnknownCodeNeverPublish','TestPancakeLmIndependentCppWirePreservesAllFieldsAndObservedZero','TestPancakeLmMissingNullDuplicateOrDerivedSourceNeverBecomesObservedZero','complete_original_pancake_lm_swaps_match_full_reward_fee_tick_oracle_state','original_pancake_lm_same_block_sequences_carry_every_accounting_field','shared_lm_routes_carry_original_rewards_fee_and_oracle_through_every_step','lm_internal_wallet_alias_and_failed_amount_leave_all_pool_state_unchanged'],
 'evidence_refs':['integration/artifacts/local/core-lm-local-q1-20261109.json'],
}

BINDINGS['12.14.pancake-lm']['tests'].append('lm_output_token_must_match_pool_direction_without_mutation')
BINDINGS['12.14.slipstream-context']['tests'].append('contextual_output_token_must_match_pool_direction_without_mutation')

BINDINGS['12.14.pancake-bsc4']={
 'paths':['common/protoc/proto/dex/v1/state.proto','chain-parser-go/internal/domain/pancake_bsc_lm.go','chain-parser-go/internal/chain/evm/pancake_bsc_lm.go','chain-parser-go/internal/chain/evm/pancake_bsc_templates.go','dex-router/crates/domain/src/pancake_bsc_lm.rs','dex-router/crates/protocol-math/src/pancake_bsc_lm.rs','dex-router/crates/routing/src/lib.rs','integration/tools/acceptance/pancake_bsc4_smoke.py','integration/tools/ci/pancake-bsc4-plan-replay.cjs'],
 'tests':['TestPancakeBsc4OriginalGetterBanksPreserveFullLedgerAndRewards','TestPancakeBsc4IndependentCppWireKeepsRawAndInheritedTickState','bsc4_independent_cpp_field34_preserves_local_and_predecessor_states','actual_bsc4_original_swaps_and_lazy_migration_match_every_poststate','actual_bsc4_migration_flags_and_local_slots_carry_over_fifteen_original_steps','shared_bsc4_lm_routes_carry_original_rewards_fee_and_oracle_through_every_step','bsc4_lm_output_token_must_match_pool_direction_without_mutation'],
 'evidence_refs':['integration/artifacts/local/core-pancake-bsc4-normal-process.json'],
}

BINDINGS['13.graph-admission-refresh']={
 'paths':['dex-router/crates/state-replica/src/lib.rs','dex-router/crates/state-replica/tests/managed_v2_scope.rs','dex-router/crates/quote-api/tests/managed_v2_scope.rs','integration/tools/acceptance/graph_admission_acceptance.py'],
 'tests':['block_commit_refreshes_admission_even_when_tokens_and_protocols_are_unchanged','admitted_bank_without_quote_edges_advances_watermark_revokes_then_recovers','changing_admission_cannot_bypass_an_explicit_scope_suspension','live_http_admission_refresh_revokes_cached_quotes_and_restores_both_directions','empty_admission_scope_http_is_unready_then_recovers_without_losing_cursor'],
 'evidence_refs':['integration/artifacts/local/core-graph-admission-local-20261109.json'],
}

BINDINGS['13.candidate-capability-disclosure']={
 'paths':['dex-router/crates/domain/src/lib.rs','chain-parser-rust/crates/domain/src/lib.rs','dex-router/crates/routing/tests/search.rs','dex-router/crates/state-replica/tests/replica.rs','dex-router/crates/quote-api/tests/api.rs','integration/tools/acceptance/candidate_scope_acceptance.py'],
 'tests':['unsupported_math_and_token_behavior_are_excluded_from_public_candidate_scope','every_unsearchable_source_row_has_an_explicit_candidate_exclusion','token_capability_exclusion_is_visible_and_restores_without_deleting_source'],
 'evidence_refs':['integration/artifacts/local/core-candidate-scope-local-20261109.json'],
}

BINDINGS['12.10.exact-out-v2']={
 'paths':['dex-router/crates/protocol-math/src/exact_out.rs','dex-router/crates/routing/src/exact_out.rs','dex-router/crates/optimizer/src/exact_out.rs','dex-router/crates/execution-plan/src/exact_out.rs','dex-router/crates/quote-api/src/exact_out.rs','dex-router/test/tools/exact_out_plan_projector.rs','integration/tools/acceptance/exact_out_reference.py','integration/tools/acceptance/exact_out_smoke.py','integration/tools/acceptance/validate_exact_out.py','integration/tools/acceptance/exact_out_acceptance.py','integration/tools/ci/v2-exact-out-reference.cjs','integration/tools/ci/v2-exact-out-plan-replay.cjs'],
 'tests':['original_router_exact_outputs_match_complete_pair_transitions','reverse_targets_then_forward_state_replay_match_every_original_router_path','ordered_grid_allocations_match_original_router_transactions_and_all_pair_ledgers','exact_out_solver_matches_exhaustive_original_grid_and_preserves_a_bounded_fallback','exact_out_http_matches_original_grid_and_has_distinct_amount_and_plan_contracts','exact_out_http_rejects_invalid_targets_budgets_aliases_and_old_endpoint_conversion','exact_out_single_flight_reorg_invalidation_and_unsupported_scope_are_explicit','test_missing_or_changed_raw_factory_pair_cannot_be_invented_from_pool_rows'],
 'evidence_refs':['integration/artifacts/local/core-exact-out-local-q1-20261109.json'],
}

BINDINGS['12.10.exact-out-v2']['tests'].extend(['full_order_fallback_survives_an_infeasible_small_probe','exact_out_http_retains_a_feasible_whole_order_when_split_probe_fails'])

BINDINGS['12.10.exact-out-bsc4']={
 'paths':['dex-router/crates/protocol-math/src/v3.rs','dex-router/crates/protocol-math/src/pancake_lm.rs','dex-router/crates/protocol-math/src/pancake_bsc_lm.rs','dex-router/crates/protocol-math/src/exact_out.rs','dex-router/crates/routing/src/exact_out.rs','dex-router/crates/execution-plan/src/exact_out.rs','dex-router/crates/quote-api/src/exact_out.rs','dex-router/test/tools/pancake_bsc_exact_out_plan_projector.rs','integration/tools/ci/pancake-bsc4-exact-out-reference.cjs','integration/tools/ci/pancake-bsc4-exact-out-state.cjs','integration/tools/ci/pancake-bsc4-exact-out-plan-replay.cjs','integration/tools/acceptance/pancake_bsc_exact_out_reference.py','integration/tools/acceptance/pancake_bsc_exact_out_smoke.py'],
 'tests':['original_bsc4_exact_output_preserves_all_ledgers_and_rejects_partial_targets','original_bsc4_exact_output_sequences_carry_lazy_migration_fee_reward_and_oracle','bsc4_ordered_exact_output_allocations_recompute_reused_physical_pool','bsc4_exact_output_routes_match_original_sequences_and_full_ledger_transitions','bsc4_exact_output_search_rejects_internal_funds_partial_targets_and_cancellation','bsc4_exact_output_http_matches_original_input_and_complete_plan','bsc4_exact_output_http_rejects_partial_and_internal_funds_and_revokes_old_plan'],
 'evidence_refs':['integration/artifacts/local/core-bsc4-exact-out-local-q1-20261110.json'],
}

BINDINGS['12.10.exact-out-legacy-lm']={
 'paths':['dex-router/crates/protocol-math/src/exact_out.rs','dex-router/crates/protocol-math/src/pancake_lm.rs','dex-router/crates/routing/src/exact_out.rs','dex-router/crates/execution-plan/src/exact_out.rs','dex-router/crates/quote-api/src/exact_out.rs','dex-router/test/tools/pancake_lm_exact_out_plan_projector.rs','integration/tools/ci/pancake-lm-exact-out-reference.cjs','integration/tools/ci/pancake-lm-exact-out-state.cjs','integration/tools/ci/pancake-lm-exact-out-plan-replay.cjs','integration/tools/acceptance/pancake_lm_exact_out_reference.py','integration/tools/acceptance/pancake_lm_exact_out_smoke.py','integration/tools/acceptance/validate_pancake_lm_exact_out.py'],
 'tests':['original_legacy_lm_exact_output_preserves_all_ledgers_and_rejects_partial_targets','original_legacy_lm_exact_output_sequences_carry_fee_reward_and_oracle','legacy_lm_ordered_exact_output_allocations_recompute_reused_physical_pool','legacy_lm_exact_output_routes_match_original_sequences_and_full_ledger_transitions','legacy_lm_exact_output_search_rejects_internal_funds_partial_targets_and_cancellation','legacy_lm_exact_output_http_matches_original_input_and_complete_plan','legacy_lm_exact_output_http_rejects_partial_and_internal_funds_and_revokes_old_plan','legacy_lm_rehashed_multihop_plan_is_rejected_by_its_single_pool_template'],
 'evidence_refs':['integration/artifacts/local/core-legacy-lm-exact-out-local-q1-20261110.json'],
}

BINDINGS['12.10.exact-out-slipstream']={'paths': ['dex-router/crates/protocol-math/src/slipstream_context.rs', 'dex-router/crates/protocol-math/src/exact_out.rs', 'dex-router/crates/routing/src/exact_out.rs', 'dex-router/crates/execution-plan/src/exact_out.rs', 'dex-router/crates/quote-api/src/exact_out.rs', 'dex-router/test/tools/slipstream_exact_out_plan_projector.rs', 'integration/tools/ci/slipstream-exact-out-reference.cjs', 'integration/tools/ci/slipstream-exact-out-state.py', 'integration/tools/ci/slipstream-exact-out-getters.cjs', 'integration/tools/ci/slipstream-exact-out-plan-replay.cjs', 'integration/tools/acceptance/slipstream_exact_out_reference.py', 'integration/tools/acceptance/slipstream_exact_out_smoke.py', 'integration/tools/acceptance/validate_slipstream_exact_out.py', 'integration/tools/acceptance/slipstream_exact_out_acceptance.py'], 'tests': ['original_slipstream_exact_output_preserves_all_ledgers_and_rejects_partial_targets', 'original_slipstream_exact_output_sequences_carry_fee_reward_and_oracle', 'slipstream_exact_output_requires_origin_scope_and_complete_curve_scope', 'slipstream_exact_output_routes_match_original_sequences_and_full_ledger_transitions', 'slipstream_exact_output_search_rejects_internal_funds_partial_targets_and_cancellation', 'slipstream_ordered_exact_output_allocations_recompute_reused_physical_pool', 'slipstream_exact_output_http_matches_original_input_and_complete_plan', 'slipstream_exact_output_http_rejects_partial_and_internal_funds_and_revokes_old_plan', 'slipstream_rehashed_multihop_plan_is_rejected_by_its_single_pool_template', 'reverse_candidate_search_keeps_direct_and_shared_prefix_paths_and_full_fallback', 'test_raw_abi_validator_rejects_changed_missing_duplicate_and_reanchored_reads'], 'evidence_refs': ['integration/artifacts/local/core-slipstream-exact-out-local-q1-20261110.json']}

BINDINGS['12.10.exact-out-lfj']={'paths': ['dex-router/crates/protocol-math/src/lfj.rs', 'dex-router/crates/protocol-math/src/exact_out.rs', 'dex-router/crates/routing/src/exact_out.rs', 'dex-router/crates/optimizer/src/exact_out.rs', 'dex-router/crates/execution-plan/src/exact_out.rs', 'dex-router/crates/quote-api/src/exact_out.rs', 'dex-router/test/tools/lfj_exact_out_plan_projector.rs', 'integration/tools/ci/lfj-v21-exact-out-reference.cjs', 'integration/tools/ci/lfj-v21-exact-out-state.py', 'integration/tools/ci/lfj-v21-exact-out-getters.cjs', 'integration/tools/ci/lfj-v21-exact-out-plan-replay.cjs', 'integration/tools/acceptance/lfj_exact_out_reference.py', 'integration/tools/acceptance/lfj_exact_out_smoke.py', 'integration/tools/acceptance/validate_lfj_exact_out.py', 'integration/tools/acceptance/lfj_exact_out_acceptance.py'], 'tests': ['lfj_original_target_inputs_preserve_actual_surplus_and_every_declared_ledger', 'lfj_original_target_sequences_carry_real_output_and_oracle_state', 'lfj_output_targets_reject_zero_width_and_incomplete_step_budgets', 'lfj_routes_separate_target_actual_and_surplus_without_clamping_state', 'lfj_target_search_keeps_funded_minimums_and_rejects_internal_or_repeated_paths', 'lfj_minimum_input_scores_actual_surplus_and_keeps_target_conservation', 'lfj_ordered_targets_recompute_shared_bins_and_preserve_every_actual_output', 'lfj_http_reports_original_minimum_input_actual_output_and_surplus', 'lfj_http_rejects_partial_targets_short_budget_internal_funds_and_false_execution', 'lfj_rehashed_plan_cannot_hide_surplus_or_move_it_to_an_unmodeled_template', 'test_original_targets_keep_real_surplus_and_input_minus_one_controls', 'test_raw_getter_mutations_missing_reads_duplicates_and_anchor_changes_fail'], 'evidence_refs': ['integration/artifacts/local/core-lfj-exact-out-local-q1-20261110.json']}

BINDINGS['12.11.meteora-mm']={'paths':['common/protoc/proto/dex/v1/state.proto','chain-parser-rust/crates/domain/src/meteora.rs','chain-parser-rust/crates/config/src/meteora.rs','chain-parser-rust/crates/solana-ingest/src/meteora.rs','chain-parser-rust/crates/solana-ingest/src/rpc.rs','dex-router/crates/protocol-math/src/meteora.rs','dex-router/crates/routing/src/lib.rs','dex-router/test/tools/meteora_plan_projector.rs','integration/tools/ci/meteora-mm-state.py','integration/tools/ci/meteora-reference-fetch.py','integration/tools/acceptance/meteora_mm_reference.py','integration/tools/acceptance/meteora_mm_smoke.py','integration/tools/acceptance/validate_meteora_mm.py'],'tests':['all_recorded_bank_layouts_decode_with_explicit_executable_and_array_closure','ordinary_rpc_preserves_complete_bank_anchor_and_refuses_changed_anchor','altered_program_clock_arrays_orders_rewards_tokens_and_backing_fail_closed','original_meteora_swap2_complete_account_bytes_and_consecutive_swaps','amount_specific_candidates_match_original_swaps_in_both_directions','failed_path_is_atomic_and_pool_funding_aliases_are_rejected_case_sensitively','independent_pool_graph_rejects_overlapping_mutable_accounts','shared_bin_ledger_replays_original_consecutive_amounts_in_order','optimization_recomputes_stale_proposal_output_and_conserves_integer_input','http_quote_amounts_plans_and_source_scope_match_original_meteora_banks','pool_funding_and_reorg_cache_invalidation_fail_closed','TestMeteoraWireCannotBeSilentlyDroppedByEVMDecoder','test_rehashed_output_amount_and_bank_coverage_changes_fail'],'evidence_refs':['integration/artifacts/local/core-meteora-mm-validation-20261110.json','integration/artifacts/local/core-meteora-mm-normal-process.json']}

BINDINGS['12.11.meteora-mm']['paths'].extend(['integration/tools/ci/meteora-mm-edge-reference.py','integration/tools/acceptance/meteora_mm_edge_reference.py'])
BINDINGS['12.11.meteora-mm']['tests'].append('original_oracle_lifetime_ring_growth_wrap_and_fee_boundaries')

BINDINGS['12.11.meteora-mm']['tests'].extend(['test_valid_additional_original_boundary_banks','test_rehashed_edge_oracle_counters_and_real_transfers_fail'])

BINDINGS['12.10.exact-out-meteora']={'paths':['dex-router/crates/protocol-math/src/meteora.rs','dex-router/crates/protocol-math/src/exact_out.rs','dex-router/crates/routing/src/exact_out.rs','dex-router/crates/optimizer/src/exact_out.rs','dex-router/crates/execution-plan/src/exact_out.rs','dex-router/crates/quote-api/src/exact_out.rs','dex-router/test/tools/meteora_exact_out_plan_projector.rs','integration/tools/ci/meteora-exact-out-reference.py','integration/tools/acceptance/meteora_exact_out_reference.py','integration/tools/acceptance/meteora_exact_out_smoke.py','integration/tools/acceptance/validate_meteora_exact_out.py'],'tests':['original_swap_exact_out2_matches_exact_credit_bin_rounding_and_complete_ledgers','meteora_output_paths_and_shared_sequences_match_original_complete_state','meteora_ordered_output_allocations_reuse_original_bins_fees_and_vault_dust','meteora_exact_output_http_matches_original_inputs_and_exact_recipient_credits','meteora_rehashed_multihop_program_namespace_and_cross_family_plans_fail','test_rehashed_exact_output_dust_and_hard_limit_controls'],'evidence_refs':['integration/artifacts/local/core-meteora-exact-out-original-20261110.json']}

BINDINGS['12.10.exact-out-meteora']['paths'].append('integration/tools/acceptance/meteora_exact_out_acceptance.py')

BINDINGS['12.10.exact-out-meteora']['evidence_refs']=['integration/artifacts/local/core-meteora-exact-out-local-q1-20261111.json']

BINDINGS['12.11.dlmm-only-y-pump']={'paths':['chain-parser-rust/crates/domain/src/pump.rs','chain-parser-rust/crates/config/src/pump.rs','chain-parser-rust/crates/solana-ingest/src/pump.rs','dex-router/crates/protocol-math/src/pump.rs','dex-router/crates/protocol-math/src/meteora.rs','integration/tools/ci/meteora-only-y-reference.py','integration/tools/ci/pump-bonding-reference.py','integration/tools/ci/pump-bonding-decoder.py','integration/tools/acceptance/family_reference.py','integration/tools/acceptance/pump_bonding_smoke.py'],'tests':['only_y_original_sbf_exact_in_out_fee_token_and_all_state_bytes','only_y_exact_input_and_output_http_use_the_declared_fee_policy','pump_original_classic_sell_and_exact_sol_input_preserve_curve_vault_and_fee_accounts','pump_fixed_programdata_fee_configuration_and_curve_decode_without_native_wsol_alias','pump_original_classic_sol_quotes_are_exact_and_complete_curves_are_excluded','native_pump_scope_can_coexist_with_wrapped_sol_dex_without_inventing_a_wrap_edge','seven_first_wave_evm_chain_ids_reuse_pinned_dex_math_and_reject_cross_chain_composition'],'evidence_refs':['integration/artifacts/local/core-family-reference-reexecution-20261111.log']}

BINDINGS['12.11.dlmm-only-y-pump']['tests'].append('shared_pump_fee_balances_are_consistent_and_both_pools_are_excluded_until_a_joint_ledger_exists')

BINDINGS['12.11.dlmm-only-y-pump']['paths'].extend(['integration/tools/ci/pump-reference-fetch.py','integration/tools/acceptance/meteora_only_y_smoke.py','integration/tools/acceptance/validate_family_expansion.py'])

BINDINGS['12.11.dlmm-only-y-pump']['paths'].append('integration/tools/acceptance/family_expansion_acceptance.py')

BINDINGS['12.11.dlmm-only-y-pump']['paths'].append('integration/tools/acceptance/tests/test_family_expansion_acceptance.py')

BINDINGS['12.11.dlmm-only-y-pump']['tests'].extend(['test_rehashed_pump_fee_ledger_change_is_rejected','test_rehashed_only_y_original_account_change_is_rejected','test_same_count_duplicate_bank_cannot_replace_missing_coverage'])

BINDINGS['12.11.dlmm-only-y-pump']['tests'].append('test_reported_pump_income_cannot_override_actual_user_balance')

BINDINGS['12.11.dlmm-only-y-pump']['evidence_refs'].extend(['integration/artifacts/local/core-family-expansion-acceptance-20261111.json','integration/artifacts/local/core-family-expansion-validation-20261111.json','integration/artifacts/local/core-pump-bonding-normal-process.json','integration/artifacts/local/core-meteora-only-y-normal-process.json'])

BINDINGS['12.11.solana-expanded']={'paths':['dex-router/crates/domain/src/meteora.rs','dex-router/crates/domain/src/meteora_token2022.rs','dex-router/crates/domain/src/pump_amm.rs','dex-router/crates/protocol-math/src/meteora.rs','dex-router/crates/protocol-math/src/pump.rs','dex-router/crates/protocol-math/src/pump_amm.rs','chain-parser-rust/crates/solana-ingest/src/meteora.rs','chain-parser-rust/crates/solana-ingest/src/pump.rs','integration/tools/acceptance/solana_expanded_reference.py','integration/tools/acceptance/solana_expanded_smoke.py','integration/tools/acceptance/validate_solana_expanded.py','integration/tools/acceptance/solana_expanded_acceptance.py'],'tests':['bitmap_boundary_swaps_preserve_original_all_pair_array_extension_and_oracle_bytes','initialized_rewards_accrue_per_visited_bin_and_original_clock_before_full_settlement','limit_order_layers_fills_fee_split_rollover_and_full_state_match_original','token2022_epoch_transfer_fee_cap_withheld_and_exact_output_match_original','pump_sell_target_inverts_both_ceil_fees_and_reports_actual_native_surplus','original_pump_amm_v1_templates_preserve_all_vault_fee_and_native_backing_state','actual_migrate_created_pool_handoff_replaces_completed_curve_without_a_wrap_edge','extended_dlmm_profiles_http_use_original_net_credits_and_complete_amounts','test_token2022_withheld_amount_tamper_is_rejected','test_order_filled_counter_tamper_is_rejected','test_pump_required_input_tamper_is_rejected','test_migration_handoff_cannot_substitute_an_unrelated_vault'],'evidence_refs':['integration/artifacts/local/solana-expanded-reference-reexecution-20261111.log']}

BINDINGS['12.11.solana-expanded']['evidence_refs'].extend(['integration/artifacts/local/core-solana-expanded-acceptance-20261111.json','integration/artifacts/local/core-solana-expanded-validation-20261111.json'])
BINDINGS['12.11.solana-expanded']['tests'].extend(['active_curve_sol_fee_wallets_do_not_alias_another_markets_wsol_fee_atas','actual_migrated_boost_pool_http_prices_virtual_inventory_but_limits_real_payout'])


BINDINGS['12.11.dlmm-orders-exact-out']={
 'paths':['dex-router/crates/protocol-math/src/meteora.rs','dex-router/crates/protocol-math/src/exact_out.rs','dex-router/crates/routing/src/exact_out.rs','dex-router/crates/execution-plan/src/exact_out.rs','chain-parser-rust/crates/solana-ingest/tests/meteora_orders.rs','dex-router/test/tools/meteora_exact_out_plan_projector.rs','integration/tools/ci/meteora-orders-exact-out-reference.py','integration/tools/acceptance/meteora_orders_exact_out_reference.py','integration/tools/acceptance/solana_expanded_smoke.py','integration/tools/acceptance/validate_meteora_orders_exact_out.py'],
 'tests':['orders_exact_out_original_rounding_full_drains_and_complete_state','orders_exact_out_banks_decode_all_raw_layers','orders_exact_out_routes_replay_full_ledgers_and_fail_atomically','orders_exact_out_excludes_internal_funds_and_composed_hops','orders_exact_out_optimizer_reprices_candidates_and_shared_pool_allocations','extended_dlmm_profiles_http_use_original_net_credits_and_complete_amounts','test_original_transfers_projections_and_budget_controls','test_fabricated_required_input_rejected','test_changed_complete_order_state_rejected','test_missing_budget_failure_rejected','test_rejected_transaction_with_mutation_rejected'],
 'evidence_refs':['integration/artifacts/local/core-meteora-orders-exact-out-validation-20261111.json'],
}

BINDINGS['12.11.dlmm-orders-exact-out']['tests'].append('orders_exact_out_rehashed_composed_plans_and_stale_views_are_rejected')


BINDINGS['12.12.frontend-quote-playground']={
 'paths':['frontend/index.html','frontend/src/app.js','frontend/src/styles.css','frontend/README.md','dex-router/crates/quote-api/src/lib.rs','dex-router/crates/quote-api/Cargo.toml','dex-router/crates/quote-api/tests/meteora_extended.rs'],
 'tests':['quote_options_exposes_current_chain_pool_tokens_and_protocols'],
 'evidence_refs':['integration/artifacts/local/phase54-frontend-quote-playground.json','integration/artifacts/local/phase54-quote-api-check-locked.log','integration/artifacts/local/phase54-quote-options-test.log'],
}


BINDINGS['12.12.frontend-live-quote']={
 'paths':['frontend/index.html','frontend/src/app.js','frontend/src/styles.css','dex-router/crates/quote-api/src/lib.rs','dex-router/crates/quote-api/tests/meteora_extended.rs'],
 'tests':['quote_options_exposes_current_chain_pool_tokens_and_protocols'],
 'evidence_refs':['integration/artifacts/local/phase55-frontend-live-quote.json','integration/artifacts/local/phase55-quote-api-check.log','integration/artifacts/local/phase55-quote-options-test.log'],
}


BINDINGS['12.12.protocol-capabilities']={
 'paths':['dex-router/crates/quote-api/Cargo.toml','dex-router/crates/quote-api/src/lib.rs','dex-router/crates/quote-api/tests/meteora_extended.rs','frontend/index.html','frontend/src/app.js','frontend/src/styles.css','frontend/README.md'],
 'tests':['quote_options_exposes_current_chain_pool_tokens_and_protocols'],
 'evidence_refs':['integration/artifacts/local/phase56-protocol-capabilities.json','integration/artifacts/local/phase56-quote-api-check.log','integration/artifacts/local/phase56-quote-options-test.log'],
}


BINDINGS['12.12.quote-execution']={
 'paths':['dex-router/crates/protocol-math/src/pump_amm.rs','dex-router/crates/routing/src/exact_out.rs','dex-router/crates/execution-plan/src/exact_out.rs','dex-router/crates/execution-plan/src/solana_unsigned.rs','dex-router/crates/quote-api/src/preparation.rs','dex-router/crates/quote-api/src/lib.rs','dex-router/crates/quote-api/examples/frontend_harness.rs','frontend/src/app.js','frontend/src/model.js','frontend/test/browser.mjs','integration/tools/acceptance/validate_quote_execution.py','integration/tools/acceptance/pump_amm_sell_target_reference.py','integration/tools/acceptance/meteora_unsigned_reference.py'],
 'tests':['sell_target_three_ceil_fees_minimum_input_and_all_native_post_state','original_pump_swap_sell_targets_flow_through_router_optimizer_api_and_plan','preparation_checks_current_program_accounts_funds_and_compiles_unsigned_transaction','cors_accepts_configured_local_frontend_and_rejects_unknown_origins','cors_origins_are_explicit_bounded_and_cannot_contain_path_or_credentials','TestCORSOriginsAreBoundedAndRejectCredentialsAndPaths','test_reported_output_cannot_override_actual_wallet_credit','test_budget_failure_cannot_be_marked_success','test_duplicate_targets_cannot_replace_matrix_coverage'],
 'evidence_refs':['integration/artifacts/local/core-quote-execution-validation-20261112.json'],
}
