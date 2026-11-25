package CBGUC01Verification {
    private import CBGSystem::*;
    private import CBGUC01Requirements::*;

    doc /* Planned verification for UC1. Each case describes observations
           to collect during a future test, inspection or benchmark. */

    verification def SessionStartupAndGraphReuse {
        doc /* After a restart, open the first repository task session in each
               supported agent environment. Exercise existing and new worktrees
               after one setup. Observe automatic startup, instructions, graph
               status and context identifiers. Check graph-first guidance where
               usable context exists. Exercise missing, stale, invalid and valid
               graph data to check when the system builds, refreshes or reuses it. */
        subject system : CodebaseGraph;
        objective {
            verify automaticSessionInstructions;
            verify automaticSessionGraphStatus;
            verify oneTimeRepositorySetup;
            verify contextIdentifierReporting;
            verify graphUseSessionGuidance;
            verify automaticGraphPreparation;
            verify firstSessionStartup;
            verify existingGraphValidationAndReuse;
        }
    }

    verification def LiveRefreshTiming {
        doc /* On the agreed codebaseGraph benchmark repository, start an active
               session with a built and current graph. Save an edit to one source
               file. Measure from write completion until updated context is
               queryable, including change detection and refresh. Use no query
               load during the measured interval and compare with the one-second
               limit. Record the benchmark revision, hardware and operating load.
               The timing target under query load remains open. */
        subject system : CodebaseGraph;
        objective {
            verify liveGraphRefresh;
        }
    }

    verification def QueryReadinessAndResultContinuity {
        doc /* Query during initial build and refresh. Observe building status
               when no graph exists and immediate access to available context
               during a refresh. Compare each result with a single graph state,
               including separate definition and relationship freshness by symbol.
               Include a partially analysable edit and a branch switch during an
               in-flight query. Check coverage gaps, retained usable information
               and the original branch context and label. Record initial-build
               duration and query latency under a stated operating profile.
               Acceptance targets for those measurements remain open. */
        subject system : CodebaseGraph;
        objective {
            verify queryDuringRefresh;
            verify symbolFreshnessStatus;
            verify initialBuildQueryStatus;
            verify queryResponseConsistency;
            verify queryBranchContinuity;
            verify partialAnalysisContext;
        }
    }

    verification def MonitoringAndSourceDeletion {
        doc /* While the system is running with no agent sessions, save an edit
               and observe monitoring and refresh. During a session, delete a
               source file and delete an individual symbol in a separate edit.
               After detection and before refresh completes, query the affected
               source context and check removal of the deleted symbols and the
               relationships connected to them. */
        subject system : CodebaseGraph;
        objective {
            verify deletedSourceSymbolExclusion;
            verify continuousRepositoryMonitoring;
            verify deletedSymbolRelationshipExclusion;
        }
    }

    verification def RecoveryAndFailureStatus {
        doc /* Induce failures during both build and refresh, and terminate the
               process during an active session. Observe automatic recovery or
               restart as applicable. Exhaust three retries and observe the
               persistent failure. Exercise a new session, a relevant source
               change and an explicit retry after exhaustion and during recovery.
               Query with and without usable graph data and check the result,
               freshness and failure status appropriate to each case. */
        subject system : CodebaseGraph;
        objective {
            verify automaticGraphRecovery;
            verify queryAfterFailedRecovery;
            verify unavailableQueryAfterRecovery;
            verify recoveryRetryLimit;
            verify recoveryRestartTriggers;
            verify recoveryTriggerOverlap;
            verify activeSessionCrashRecovery;
        }
    }

    verification def ConcurrentWorktreesAndGraphCleanup {
        doc /* Run concurrent agent queries against the same and different
               worktrees. Check query and refresh continuity in other worktrees
               during a worktree-local failure. Delete a worktree and switch a
               branch while another worktree, session or query still needs its
               graph data. Observe cleanup after the final dependency ends.
               Concurrent query capacity, query latency and cleanup timing
               targets remain open. */
        subject system : CodebaseGraph;
        objective {
            verify concurrentWorktreeQueries;
            verify deletedWorktreeGraphCleanup;
            verify worktreeFailureIsolation;
            verify obsoleteBranchGraphCleanup;
        }
    }

    verification def PlatformAndGraphQueryScope {
        doc /* Exercise local operation on Apple Silicon macOS and native
               x64 Windows, covering the languages supported by the pinned
               baseline. Inspect exposed graph query entry points and invoke the
               CLI. Compare indexing of files listed only in .gitignore, files
               listed only in graph exclusions and files listed in neither. */
        subject system : CodebaseGraph;
        objective {
            verify localPlatformSupport;
            verify existingLanguageCoverage;
            verify cliGraphQueryInterface;
            verify graphSpecificExclusions;
        }
    }
}
