package CBGUC01Requirements {
    private import CBGSystem::*;
    private import CBGNeeds::*;
    private import CBGUC01::*;

    requirement def LiveGraphRefresh {
        doc /* R-UC1-01. During an active repository session, codebaseGraph
               shall automatically make saved changes to one edited source
               file queryable. This shall occur within 1 second of the write
               completing when the graph was already built and current
               before the edit and no queries run during that interval,
               under the agreed operating conditions. */

        subject system : CodebaseGraph;
    }

    requirement liveGraphRefresh : LiveGraphRefresh {
        subject redefines system = design.codebaseGraph;
    }

    // Stakeholder decisions, 24 November 2026:
    // Saved repository changes only; maximum delay is 1 second.
    // One edited source file is one small edit covered by this limit.
    // Initial verification benchmark: rabii-chaarani/codebaseGraph.
    // Benchmark choice does not restrict the product's supported repositories.
    // Measurement: completed write to updated context becoming queryable;
    // the interval includes change detection and refresh.
    // Initial state: the graph is already built and current before the edit.
    // Query load: no queries run throughout the measured refresh interval.
    // Automatic refresh remains required during query activity; its latency
    // target under that workload has not been agreed.
    // Status: draft; the selected limit has not been benchmarked or verified.
    // Open profile: benchmark revision, hardware and remaining operating load.
    // Initial-build and branch-switch timing remain separate open requirements,
    // outside the single-file-edit target.

    dependency liveGraphRefreshUseCase from liveGraphRefresh to uc1;
    dependency liveGraphRefreshNeed from liveGraphRefresh to reliableOperationNeed;

    requirement def QueryDuringRefresh {
        doc /* R-UC1-02. During graph refresh, codebaseGraph shall return
               available context for the requested repository, branch and
               worktree without waiting for the refresh to complete. */

        subject system : CodebaseGraph;
    }

    requirement queryDuringRefresh : QueryDuringRefresh {
        subject redefines system = design.codebaseGraph;
    }

    requirement def SymbolFreshnessStatus {
        doc /* R-UC1-03. codebaseGraph shall report the freshness of the
               information returned for each symbol individually, relative
               to the requested repository, branch and worktree's saved
               source state, reporting separate freshness statuses for
               its definition and its relationships when returned. Each
               status shall be 'unknown' when freshness cannot be established. */

        subject system : CodebaseGraph;
    }

    requirement symbolFreshnessStatus : SymbolFreshnessStatus {
        subject redefines system = design.codebaseGraph;
    }

    // Stakeholder decisions, 24 November 2026:
    // Return existing results during refresh; mark stale information per symbol.
    // One response may contain symbols with different freshness statuses.
    // Use 'unknown' when a returned symbol's freshness cannot yet be
    // established; this status does not assert that it is current or stale.
    // Freshness concerns each symbol's returned information, not merely whether
    // its file changed or whether the graph as a whole is being refreshed.
    // "Immediately" means no wait for refresh completion; query latency remains
    // to be quantified. These are draft requirements, not verified behaviour.
    // Open: precise criteria for current and stale.

    dependency queryDuringRefreshUseCase from queryDuringRefresh to uc1;
    dependency queryDuringRefreshNeeds from queryDuringRefresh to
        reliableOperationNeed, concurrentQueryNeed;
    dependency symbolFreshnessUseCase from symbolFreshnessStatus to uc1;
    dependency symbolFreshnessNeeds from symbolFreshnessStatus to
        graphStatusNeed, reliableOperationNeed;

    requirement def InitialBuildQueryStatus {
        doc /* R-UC1-04. When no graph is available for the requested
               repository, branch and worktree and its initial build is
               in progress, codebaseGraph shall respond to queries with
               status 'building' and no results, without waiting for the
               initial build to finish. */

        subject system : CodebaseGraph;
    }

    requirement initialBuildQueryStatus : InitialBuildQueryStatus {
        subject redefines system = design.codebaseGraph;
    }

    // Stakeholder decision, 24 November 2026: return without waiting for
    // the initial build. 'building' describes query readiness, not the
    // freshness of a symbol. This is a draft requirement.
    // Open: query latency. Automatic recovery is covered by R-UC1-05.
    // Automatic build initiation is covered by R-UC1-22.
    // Deferred performance target TBD-UC1-01 (24 November 2026):
    // Initial-build duration cannot be defined reliably at this stage.
    // No numeric acceptance threshold has been agreed.
    // Proposed resolution: review initial-build measurements on the codebaseGraph
    // repository under a recorded operating profile before agreeing the target.

    dependency initialBuildQueryUseCase from initialBuildQueryStatus to uc1;
    dependency initialBuildQueryNeeds from initialBuildQueryStatus to
        graphStatusNeed, automaticSessionUseNeed, reliableOperationNeed;

    requirement def AutomaticGraphRecovery {
        doc /* R-UC1-05. Following a graph build or refresh failure,
               codebaseGraph shall initiate recovery automatically for
               the affected repository, branch and worktree, without
               requiring the agent or developer to request a retry,
               within the retry limit specified by R-UC1-14. */

        subject system : CodebaseGraph;
    }

    requirement automaticGraphRecovery : AutomaticGraphRecovery {
        subject redefines system = design.codebaseGraph;
    }

    // Stakeholder decision, 24 November 2026: automatic recovery after a
    // build or refresh failure. This draft specifies automatic initiation;
    // successful recovery from every possible failure is not established.
    // The retry limit is specified by R-UC1-14.
    // Recovery restart triggers are specified by R-UC1-15.
    // Overlapping restart triggers are covered by R-UC1-18.
    // Open: recoverable failure scope, retry timing, query behaviour during
    // recovery and further escalation.

    dependency automaticGraphRecoveryUseCase from automaticGraphRecovery to uc1;
    dependency automaticGraphRecoveryNeeds from automaticGraphRecovery to
        reliableOperationNeed, automaticSessionUseNeed;

    requirement def QueryAfterFailedRecovery {
        doc /* R-UC1-06. If automatic recovery cannot restore graph
               operation and usable graph data remains available for the
               requested repository, branch and worktree, codebaseGraph
               shall respond to queries using that data, with freshness
               reported for each returned symbol and an explicit indication
               of the unresolved failure. */

        subject system : CodebaseGraph;
    }

    requirement queryAfterFailedRecovery : QueryAfterFailedRecovery {
        subject redefines system = design.codebaseGraph;
    }

    // Stakeholder decision, 24 November 2026: keep usable results available
    // after failed recovery, with per-symbol freshness and a failure indication.
    // The failure indication is separate from the freshness of each symbol.
    // A valid query finding no matches is distinct from having no usable data.
    // Open: criteria for usable graph data. The response when usable graph
    // data is absent is covered by R-UC1-07. This is a draft requirement.

    dependency queryAfterFailedRecoveryUseCase from queryAfterFailedRecovery to uc1;
    dependency queryAfterFailedRecoveryNeeds from queryAfterFailedRecovery to
        reliableOperationNeed, graphStatusNeed;
    dependency queryAfterFailedRecoveryFreshness from queryAfterFailedRecovery to
        symbolFreshnessStatus;

    requirement def UnavailableQueryAfterRecovery {
        doc /* R-UC1-07. If automatic recovery cannot restore graph
               operation and no usable graph data is available for the
               requested repository, branch and worktree, codebaseGraph
               shall respond to queries with status 'unavailable', no graph
               results and a brief explanation of the failure. */

        subject system : CodebaseGraph;
    }

    requirement unavailableQueryAfterRecovery : UnavailableQueryAfterRecovery {
        subject redefines system = design.codebaseGraph;
    }

    // Stakeholder decision, 24 November 2026: return 'unavailable' and a
    // brief failure explanation after unsuccessful recovery when usable
    // graph data is absent. 'unavailable' describes query readiness.
    // This is a draft requirement. The retry limit is specified by R-UC1-14;
    // the required contents of failure explanations remain open.

    dependency unavailableQueryUseCase from unavailableQueryAfterRecovery to uc1;
    dependency unavailableQueryNeeds from unavailableQueryAfterRecovery to
        reliableOperationNeed, graphStatusNeed;

    requirement def AutomaticSessionInstructions {
        doc /* R-UC1-08. After repository setup, at the start of each
               repository task session in Codex CLI, Codex in the ChatGPT
               Desktop app, Claude Code, GitHub Copilot CLI or GitHub
               Copilot in VS Code,
               codebaseGraph shall automatically provide the agent with
               instructions for using codebaseGraph tools in the selected
               repository, branch and worktree. */

        subject system : CodebaseGraph;
    }

    requirement automaticSessionInstructions : AutomaticSessionInstructions {
        subject redefines system = design.codebaseGraph;
    }

    // Stakeholder decisions, 24 November 2026: required environments are
    // Codex CLI, Codex in the ChatGPT Desktop app, Claude Code, GitHub
    // Copilot CLI and GitHub Copilot in VS Code.
    // One explicit repository setup is covered by R-UC1-12.
    // Guidance to consult the graph first is covered by R-UC1-21.
    // Open: supported versions, remaining setup prerequisites and other
    // required instruction content. This requirement covers instruction
    // delivery and remains draft.

    dependency automaticSessionInstructionsUseCase from automaticSessionInstructions to uc1;
    dependency automaticSessionInstructionsNeed from automaticSessionInstructions to
        automaticSessionUseNeed;

    requirement def AutomaticSessionGraphStatus {
        doc /* R-UC1-09. After repository setup, at the start of each
               repository task session in the environments covered by
               R-UC1-08, codebaseGraph shall
               automatically provide the agent with graph availability
               and build or refresh status for the selected repository,
               branch and worktree. */

        subject system : CodebaseGraph;
    }

    requirement automaticSessionGraphStatus : AutomaticSessionGraphStatus {
        subject redefines system = design.codebaseGraph;
    }

    // Stakeholder decision, 24 November 2026: automatically supply
    // instructions on using codebaseGraph plus graph status at session start.
    // This decision establishes no automatic overview or task-context retrieval
    // requirement. Per-symbol freshness remains governed by R-UC1-03.
    // Context identifiers are covered by R-UC1-20. This is a draft
    // requirement; the remaining status contents are open.

    dependency automaticSessionGraphStatusUseCase from automaticSessionGraphStatus to uc1;
    dependency automaticSessionGraphStatusNeeds from automaticSessionGraphStatus to
        automaticSessionUseNeed, graphStatusNeed;
    dependency automaticSessionGraphStatusEnvironments from automaticSessionGraphStatus to
        automaticSessionInstructions;

    requirement def ConcurrentWorktreeQueries {
        doc /* R-UC1-10. codebaseGraph shall support concurrent graph
               queries from multiple agents both for the same worktree
               and for different worktrees of the same repository. */

        subject system : CodebaseGraph;
    }

    requirement concurrentWorktreeQueries : ConcurrentWorktreeQueries {
        subject redefines system = design.codebaseGraph;
    }

    // Stakeholder decision, 24 November 2026: support both concurrent-query
    // scenarios. This is a draft requirement.
    // Response consistency during graph updates is covered by R-UC1-17.
    // Open: maximum concurrency and query latency under concurrent load.

    dependency concurrentWorktreeQueriesUseCase from concurrentWorktreeQueries to uc1;
    dependency concurrentWorktreeQueriesNeeds from concurrentWorktreeQueries to
        concurrentQueryNeed, reliableOperationNeed;

    requirement def LocalPlatformSupport {
        doc /* R-UC1-11. codebaseGraph shall support local operation on
               macOS with Apple Silicon processors and natively on Windows
               with x86-64 (x64) processors. */

        subject system : CodebaseGraph;
    }

    requirement localPlatformSupport : LocalPlatformSupport {
        subject redefines system = design.codebaseGraph;
    }

    // Stakeholder decisions, 24 November 2026: initial platform scope is
    // macOS on Apple Silicon and native Windows on x86-64 (x64).
    // This is a draft compatibility requirement.
    // Open: OS versions and supported combinations of agent environment
    // and operating system.

    dependency localPlatformSupportUseCase from localPlatformSupport to uc1;
    dependency localPlatformSupportNeeds from localPlatformSupport to
        automaticSessionUseNeed, reliableOperationNeed;

    requirement def OneTimeRepositorySetup {
        doc /* R-UC1-12. After one explicit setup of a repository,
               codebaseGraph shall provide the specified session-start
               behaviour for all existing and newly created worktrees of
               that repository without requiring setup to be repeated
               for each session or worktree. */

        subject system : CodebaseGraph;
    }

    requirement oneTimeRepositorySetup : OneTimeRepositorySetup {
        subject redefines system = design.codebaseGraph;
    }

    // Stakeholder decisions, 24 November 2026: one explicit setup per
    // repository covers subsequent sessions and all existing and newly
    // created worktrees automatically.
    // Open: repository identity across clones or moves and setup across
    // installations.
    // This is a draft requirement.

    dependency oneTimeRepositorySetupUseCase from oneTimeRepositorySetup to uc1;
    dependency oneTimeRepositorySetupNeeds from oneTimeRepositorySetup to
        automaticSessionUseNeed, reliableOperationNeed;
    dependency automaticSessionInstructionsSetup from automaticSessionInstructions to
        oneTimeRepositorySetup;
    dependency automaticSessionGraphStatusSetup from automaticSessionGraphStatus to
        oneTimeRepositorySetup;

    requirement def DeletedWorktreeGraphCleanup {
        doc /* R-UC1-13. codebaseGraph shall automatically delete graph
               data associated with a deleted worktree once that data is
               no longer required by any remaining worktree, session or
               active query. */

        subject system : CodebaseGraph;
    }

    requirement deletedWorktreeGraphCleanup : DeletedWorktreeGraphCleanup {
        subject redefines system = design.codebaseGraph;
    }

    // Stakeholder decision, 24 November 2026: automatically delete unused
    // graph data from deleted worktrees. Shared or actively needed data
    // remains outside the deletion scope. This is a draft requirement.
    // Open: cleanup delay, how deletion and lack of use are established,
    // and cleanup-failure handling.
    // Deferred policy TBD-UC1-02 (24 November 2026): whether configurable
    // disk limits are required, their scope and their values remain
    // undecided. Automatic deleted-worktree cleanup remains required.

    dependency deletedWorktreeGraphCleanupUseCase from deletedWorktreeGraphCleanup to uc1;
    dependency deletedWorktreeGraphCleanupNeed from deletedWorktreeGraphCleanup to
        reliableOperationNeed;

    requirement def RecoveryRetryLimit {
        doc /* R-UC1-14. Following a failed graph build or refresh,
               codebaseGraph shall limit each automatic recovery sequence
               to three retry attempts. If all three retries fail,
               codebaseGraph shall stop that recovery sequence and report
               the persistent failure. */

        subject system : CodebaseGraph;
    }

    requirement recoveryRetryLimit : RecoveryRetryLimit {
        subject redefines system = design.codebaseGraph;
    }

    // Stakeholder decision, 24 November 2026: stop after three retries.
    // Counting convention: the original attempt plus the first recovery
    // sequence permits up to four attempts. Each later recovery sequence
    // permitted by R-UC1-15 has its own limit of three retry attempts.
    // This is a draft requirement.
    // Query responses after unsuccessful recovery follow R-UC1-06/07.
    // Overlapping restart triggers are covered by R-UC1-18.
    // Open: retry timing and earlier stop conditions.

    dependency recoveryRetryLimitUseCase from recoveryRetryLimit to uc1;
    dependency recoveryRetryLimitNeeds from recoveryRetryLimit to
        reliableOperationNeed, graphStatusNeed;
    dependency recoveryRetryLimitScope from recoveryRetryLimit to
        automaticGraphRecovery;
    dependency recoveryRetryLimitResponses from recoveryRetryLimit to
        queryAfterFailedRecovery, unavailableQueryAfterRecovery;

    requirement def RecoveryRestartTriggers {
        doc /* R-UC1-15. After a recovery sequence exhausts its retry
               allowance, codebaseGraph shall start a new recovery sequence
               for the affected repository, branch and worktree when a new
               session for that context starts, a relevant source change
               occurs, or an explicit retry is requested. Each new sequence
               shall obey the retry limit in R-UC1-14. */

        subject system : CodebaseGraph;
    }

    requirement recoveryRestartTriggers : RecoveryRestartTriggers {
        subject redefines system = design.codebaseGraph;
    }

    // Stakeholder decision, 24 November 2026: all three restart triggers
    // are permitted after exhausted recovery. This is a draft requirement.
    // Overlapping triggers are covered by R-UC1-18.
    // Open: what makes a source change relevant.

    dependency recoveryRestartTriggersUseCase from recoveryRestartTriggers to uc1;
    dependency recoveryRestartTriggersNeeds from recoveryRestartTriggers to
        reliableOperationNeed, automaticSessionUseNeed;
    dependency recoveryRestartTriggersLimit from recoveryRestartTriggers to
        recoveryRetryLimit;

    requirement def WorktreeFailureIsolation {
        doc /* R-UC1-16. When a failure is confined to one worktree,
               codebaseGraph shall continue normal graph query and refresh
               operation for other worktrees. */

        subject system : CodebaseGraph;
    }

    requirement worktreeFailureIsolation : WorktreeFailureIsolation {
        subject redefines system = design.codebaseGraph;
    }

    // Stakeholder decision, 24 November 2026: a worktree-confined failure
    // leaves queries and refreshes for other worktrees operational.
    // Shared-resource and machine-level failure behaviour remains open.
    // This is a draft requirement; no isolation mechanism is selected.

    dependency worktreeFailureIsolationUseCase from worktreeFailureIsolation to uc1;
    dependency worktreeFailureIsolationNeeds from worktreeFailureIsolation to
        reliableOperationNeed, concurrentQueryNeed;

    requirement def QueryResponseConsistency {
        doc /* R-UC1-17. For each query response containing graph results,
               codebaseGraph shall return symbols, relationships and other
               graph context that together reflect one consistent graph
               state for the requested repository, branch and worktree,
               including when a refresh occurs during the query. */

        subject system : CodebaseGraph;
    }

    requirement queryResponseConsistency : QueryResponseConsistency {
        subject redefines system = design.codebaseGraph;
    }

    // Stakeholder decision, 24 November 2026: one consistent graph state
    // per response. Per-symbol freshness remains governed by R-UC1-03 and
    // may differ between symbols within that graph state.
    // This draft specifies an observable consistency property. State
    // selection, the freshness reference time and verification criteria
    // remain open; no implementation mechanism is selected.

    dependency queryResponseConsistencyUseCase from queryResponseConsistency to uc1;
    dependency queryResponseConsistencyNeeds from queryResponseConsistency to
        reliableOperationNeed, concurrentQueryNeed;
    dependency queryResponseConsistencyQueries from queryResponseConsistency to
        queryDuringRefresh, concurrentWorktreeQueries;
    dependency queryResponseConsistencyFreshness from queryResponseConsistency to
        symbolFreshnessStatus;

    requirement def RecoveryTriggerOverlap {
        doc /* R-UC1-18. When a recovery restart trigger occurs while a
               recovery sequence is already running for the affected
               context, codebaseGraph shall continue that sequence with
               its remaining retry allowance, without starting a replacement
               or additional sequence for that failure. */

        subject system : CodebaseGraph;
    }

    requirement recoveryTriggerOverlap : RecoveryTriggerOverlap {
        subject redefines system = design.codebaseGraph;
    }

    // Stakeholder decision, 24 November 2026: overlapping triggers retain
    // the current recovery sequence and its remaining allowance.
    // Relevant source changes remain subject to the refresh requirements;
    // retaining the retry allowance does not discard those changes.
    // This is a draft requirement.

    dependency recoveryTriggerOverlapUseCase from recoveryTriggerOverlap to uc1;
    dependency recoveryTriggerOverlapNeed from recoveryTriggerOverlap to
        reliableOperationNeed;
    dependency recoveryTriggerOverlapRules from recoveryTriggerOverlap to
        recoveryRestartTriggers, recoveryRetryLimit;

    requirement def QueryBranchContinuity {
        doc /* R-UC1-19. If the active branch changes while a query is
               running, codebaseGraph shall complete that query using its
               original branch context and clearly identify that original
               branch in the response. */

        subject system : CodebaseGraph;
    }

    requirement queryBranchContinuity : QueryBranchContinuity {
        subject redefines system = design.codebaseGraph;
    }

    // Stakeholder decision, 24 November 2026: a running query finishes
    // for its original branch and identifies that branch in its response.
    // Results and freshness assessments remain tied to the original
    // requested source context under R-UC1-03 and R-UC1-17.
    // This is a draft requirement.

    dependency queryBranchContinuityUseCase from queryBranchContinuity to uc1;
    dependency queryBranchContinuityNeeds from queryBranchContinuity to
        reliableOperationNeed, graphStatusNeed;
    dependency queryBranchContinuityRules from queryBranchContinuity to
        queryResponseConsistency, symbolFreshnessStatus;

    requirement def ContextIdentifierReporting {
        doc /* R-UC1-20. codebaseGraph shall include repository, branch
               and worktree identifiers in session-start information and
               graph-status responses, and omit routine repetition of these
               context identifiers in ordinary query responses, except for
               the branch identification required by R-UC1-19. */

        subject system : CodebaseGraph;
    }

    requirement contextIdentifierReporting : ContextIdentifierReporting {
        subject redefines system = design.codebaseGraph;
    }

    // Stakeholder decisions, 24 November 2026: context identifiers appear
    // in session-start and status responses. The selected branch-switch
    // query behaviour explicitly requires the original branch to be labelled.
    // This scope concerns repeated context metadata; result context binding,
    // response consistency and symbol/source identifiers are preserved.
    // This is a draft requirement; identifier formats remain open.

    dependency contextIdentifierReportingUseCase from contextIdentifierReporting to uc1;
    dependency contextIdentifierReportingNeeds from contextIdentifierReporting to
        graphStatusNeed, structuredContextNeed;
    dependency contextIdentifierReportingRules from contextIdentifierReporting to
        automaticSessionGraphStatus, queryBranchContinuity;

    requirement def GraphUseSessionGuidance {
        doc /* R-UC1-21. codebaseGraph shall provide session instructions
               directing agents to consult codebaseGraph first when seeking
               codebase context and usable graph information is available,
               then open source files as needed. */

        subject system : CodebaseGraph;
    }

    requirement graphUseSessionGuidance : GraphUseSessionGuidance {
        subject redefines system = design.codebaseGraph;
    }

    // Stakeholder decision, 24 November 2026: consult codebaseGraph first
    // for available codebase context, then open source files as needed.
    // This draft defines instruction content. Automatic delivery is covered
    // by R-UC1-08; startup information remains instructions plus graph status.
    // File exploration remains permitted when graph information is unavailable
    // or insufficient. Agent adherence requires separate validation.

    dependency graphUseSessionGuidanceUseCase from graphUseSessionGuidance to uc1;
    dependency graphUseSessionGuidanceNeeds from graphUseSessionGuidance to
        automaticSessionUseNeed, structuredContextNeed, rapidUnderstandingNeed;
    dependency graphUseSessionGuidanceDelivery from graphUseSessionGuidance to
        automaticSessionInstructions;

    requirement def AutomaticGraphPreparation {
        doc /* R-UC1-22. After repository setup, codebaseGraph shall
               automatically build missing graph context or refresh stale
               graph context for the selected repository, branch and worktree
               at session start and whenever the active branch or worktree
               changes. */

        subject system : CodebaseGraph;
    }

    requirement automaticGraphPreparation : AutomaticGraphPreparation {
        subject redefines system = design.codebaseGraph;
    }

    // Consolidation, 24 November 2026: this requirement records the
    // automatic preparation behaviour already agreed in UC1.
    // Query behaviour during preparation follows R-UC1-02/04; recovery and
    // retries follow R-UC1-05/14/15/18. Per-symbol freshness remains distinct
    // from deciding that some graph context requires refresh.
    // This is a draft requirement. Initial-build and branch-switch timing
    // remain open; this requirement introduces no new timing limit.

    dependency automaticGraphPreparationUseCase from automaticGraphPreparation to uc1;
    dependency automaticGraphPreparationNeeds from automaticGraphPreparation to
        automaticSessionUseNeed, reliableOperationNeed;
    dependency automaticGraphPreparationQueries from automaticGraphPreparation to
        queryDuringRefresh, initialBuildQueryStatus;
    dependency automaticGraphPreparationRecovery from automaticGraphPreparation to
        automaticGraphRecovery, recoveryRetryLimit, recoveryRestartTriggers,
        recoveryTriggerOverlap;

    requirement def ExistingLanguageCoverage {
        doc /* R-UC1-23. codebaseGraph shall preserve existing supported
               language coverage from the repository baseline. */
        subject system : CodebaseGraph;
    }
    requirement existingLanguageCoverage : ExistingLanguageCoverage {
        subject redefines system = design.codebaseGraph;
    }
    dependency existingLanguageCoverageUseCase from existingLanguageCoverage to uc1;
    dependency existingLanguageCoverageNeed from existingLanguageCoverage to codeUnderstandingNeed;

    requirement def PartialAnalysisContext {
        doc /* R-UC1-24. When some saved source cannot be analysed,
               codebaseGraph shall return available analysable new information
               alongside usable earlier information, explicitly identifying
               coverage gaps and reporting separate definition and relationship
               freshness for each returned symbol. Each response shall reflect
               one consistent graph state, not a mixture of independently
               selected graph states. */
        subject system : CodebaseGraph;
    }
    requirement partialAnalysisContext : PartialAnalysisContext {
        subject redefines system = design.codebaseGraph;
    }
    dependency partialAnalysisContextUseCase from partialAnalysisContext to uc1;
    dependency partialAnalysisContextNeeds from partialAnalysisContext to graphStatusNeed, reliableOperationNeed;
    dependency partialAnalysisContextRules from partialAnalysisContext to symbolFreshnessStatus, queryResponseConsistency;

    // Remaining requirements will define readiness, availability, host
    // compatibility, operating workloads and resource limits.

    requirement def DeletedSourceSymbolExclusion {
        doc /* R-UC1-25. Upon detecting deletion of a source file,
               codebaseGraph shall exclude symbols defined in that file
               from results for the affected source context, without
               waiting for graph refresh completion. */
        subject system : CodebaseGraph;
    }
    requirement deletedSourceSymbolExclusion : DeletedSourceSymbolExclusion {
        subject redefines system = design.codebaseGraph;
    }
    dependency deletedSourceSymbolExclusionUseCase from deletedSourceSymbolExclusion to uc1;
    dependency deletedSourceSymbolExclusionNeeds from deletedSourceSymbolExclusion to graphStatusNeed, reliableOperationNeed;
    dependency deletedSourceSymbolExclusionRules from deletedSourceSymbolExclusion to queryDuringRefresh, queryResponseConsistency;
    // Draft: treatment of in-flight queries remains open; related edges follow R-UC1-29.

    requirement def ContinuousRepositoryMonitoring {
        doc /* R-UC1-26. While codebaseGraph is running, it shall continue
               monitoring and automatically refreshing configured repository
               context even when no agent sessions are active. */
        subject system : CodebaseGraph;
    }
    requirement continuousRepositoryMonitoring : ContinuousRepositoryMonitoring {
        subject redefines system = design.codebaseGraph;
    }
    dependency continuousRepositoryMonitoringUseCase from continuousRepositoryMonitoring to uc1;
    dependency continuousRepositoryMonitoringNeeds from continuousRepositoryMonitoring to reliableOperationNeed, automaticSessionUseNeed;
    // Draft: idle refresh timing remains open; startup follows R-UC1-28.

    requirement def ObsoleteBranchGraphCleanup {
        doc /* R-UC1-27. After a branch switch, codebaseGraph shall
               automatically remove obsolete branch graph data once no
               remaining worktree, session or active query requires it. */
        subject system : CodebaseGraph;
    }
    requirement obsoleteBranchGraphCleanup : ObsoleteBranchGraphCleanup {
        subject redefines system = design.codebaseGraph;
    }
    dependency obsoleteBranchGraphCleanupUseCase from obsoleteBranchGraphCleanup to uc1;
    dependency obsoleteBranchGraphCleanupNeed from obsoleteBranchGraphCleanup to reliableOperationNeed;
    dependency obsoleteBranchGraphCleanupRules from obsoleteBranchGraphCleanup to deletedWorktreeGraphCleanup, queryBranchContinuity;
    // Draft: cleanup timing and failure handling remain open.

    requirement def FirstSessionStartup {
        doc /* R-UC1-28. After a computer restart, codebaseGraph shall
               start automatically when the first supported agent session
               begins for a configured repository. Continuous monitoring
               thereafter follows R-UC1-26. */
        subject system : CodebaseGraph;
    }
    requirement firstSessionStartup : FirstSessionStartup {
        subject redefines system = design.codebaseGraph;
    }
    dependency firstSessionStartupUseCase from firstSessionStartup to uc1;
    dependency firstSessionStartupNeeds from firstSessionStartup to automaticSessionUseNeed, reliableOperationNeed;
    dependency firstSessionStartupMonitoring from firstSessionStartup to continuousRepositoryMonitoring;

    requirement def DeletedSymbolRelationshipExclusion {
        doc /* R-UC1-29. Upon detecting deletion of a symbol, codebaseGraph
               shall exclude all relationships connected to that symbol
               from results for the affected source context without waiting
               for refresh completion. */
        subject system : CodebaseGraph;
    }
    requirement deletedSymbolRelationshipExclusion : DeletedSymbolRelationshipExclusion {
        subject redefines system = design.codebaseGraph;
    }
    dependency deletedSymbolRelationshipExclusionUseCase from deletedSymbolRelationshipExclusion to uc1;
    dependency deletedSymbolRelationshipExclusionNeeds from deletedSymbolRelationshipExclusion to graphStatusNeed, reliableOperationNeed;
    dependency deletedSymbolRelationshipExclusionRules from deletedSymbolRelationshipExclusion to deletedSourceSymbolExclusion, queryResponseConsistency;

    requirement def CliGraphQueryInterface {
        doc /* R-UC1-30. codebaseGraph shall expose its graph query interface
               only through the command line. */
        subject system : CodebaseGraph;
    }
    requirement cliGraphQueryInterface : CliGraphQueryInterface {
        subject redefines system = design.codebaseGraph;
    }
    dependency cliGraphQueryInterfaceUseCase from cliGraphQueryInterface to uc1;
    dependency cliGraphQueryInterfaceNeeds from cliGraphQueryInterface to
        automaticSessionUseNeed, structuredContextNeed;

    requirement def GraphSpecificExclusions {
        doc /* R-UC1-31. codebaseGraph shall apply only codebaseGraph-specific
               exclusions, independently of .gitignore. */
        subject system : CodebaseGraph;
    }
    requirement graphSpecificExclusions : GraphSpecificExclusions {
        subject redefines system = design.codebaseGraph;
    }
    dependency graphSpecificExclusionsUseCase from graphSpecificExclusions to uc1;
    dependency graphSpecificExclusionsNeeds from graphSpecificExclusions to
        codeUnderstandingNeed, reliableOperationNeed;

    requirement def ActiveSessionCrashRecovery {
        doc /* R-UC1-32. If codebaseGraph crashes during an active session, it
               shall automatically restart and recover graph operation. */
        subject system : CodebaseGraph;
    }
    requirement activeSessionCrashRecovery : ActiveSessionCrashRecovery {
        subject redefines system = design.codebaseGraph;
    }
    dependency activeSessionCrashRecoveryUseCase from activeSessionCrashRecovery to uc1;
    dependency activeSessionCrashRecoveryNeeds from activeSessionCrashRecovery to
        automaticSessionUseNeed, reliableOperationNeed;
    dependency activeSessionCrashRecoveryRules from activeSessionCrashRecovery to
        automaticGraphRecovery, existingGraphValidationAndReuse;

    requirement def ExistingGraphValidationAndReuse {
        doc /* R-UC1-33. codebaseGraph shall validate existing graph data, reuse
               valid data, and rebuild only when needed. */
        subject system : CodebaseGraph;
    }
    requirement existingGraphValidationAndReuse : ExistingGraphValidationAndReuse {
        subject redefines system = design.codebaseGraph;
    }
    dependency existingGraphValidationAndReuseUseCase from existingGraphValidationAndReuse to uc1;
    dependency existingGraphValidationAndReuseNeeds from existingGraphValidationAndReuse to
        graphStatusNeed, reliableOperationNeed;
    dependency existingGraphValidationAndReuseRules from existingGraphValidationAndReuse to
        automaticGraphPreparation;

    // Open UC1 targets: query latency (TBD, including under concurrent
    // load) and supported concurrent query capacity (TBD).
}
