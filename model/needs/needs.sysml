package CBGNeeds {
    private import CBGStakeholders::*;
    private import CBGSystem::*;

    concern def UsefulCodeUnderstanding {
        doc /* N1. Candidate stakeholder need.
               Scenario: a developer or coding agent investigates an
               unfamiliar repository before changing code. */

        subject system : CodebaseGraph;
        stakeholder developer : Developer;

        require constraint {
            doc /* The developer needs relevant code, relationships and
                   source evidence to understand a task and assess the
                   likely impact of a proposed change. */
        }
    }

    concern def InterpretableGraphStatus {
        doc /* N2. Candidate stakeholder need.
               Scenario: source changes or indexing fails while users
               continue requesting code context. */

        subject system : CodebaseGraph;
        stakeholder developer : Developer;

        require constraint {
            doc /* The developer needs to recognise whether returned
                   information is current, stale, incomplete or uncertain
                   so that it can be used appropriately. */
        }
    }

    concern def MaintainableEvolution {
        doc /* N3. Candidate stakeholder need.
               Scenario: the maintainer adds a supported language or
               changes a retrieval capability. */

        subject system : CodebaseGraph;
        stakeholder maintainer : Maintainer;

        require constraint {
            doc /* The maintainer needs to evolve codebaseGraph with
                   understandable change impact and confidence that
                   established behaviour continues to work. */
        }
    }

    concern def TokenEfficientStructuredContext {
        doc /* N4. Stakeholder-provided need, 23 November 2026.
               AI agents need structured information about the codebase,
               with reduced token consumption compared with exploring
               source files directly.
               Scenario: an agent gathers context for a repository task. */

        subject system : CodebaseGraph;
        stakeholder developer : Developer;
        actor codingAgent : CodingAgent;

        require constraint {
            doc /* The coding agent needs structured codebase information
                   that reduces the total token consumption of codebase
                   exploration compared with exploring files directly. */
        }

        // Proposed evaluation, not an agreed acceptance target:
        // Compare matched tasks on the same repository snapshot, using the
        // same agent/model conditions and equivalent answer quality.
        // Include relevant input/output tokens, tool exchanges, automatic
        // startup context, follow-up file reads and retries across the task.
        // Record context reuse and caching consistently in both conditions.
        // The size of one graph response alone does not establish savings.
    }

    concern def AutomaticSessionUse {
        doc /* N5. Stakeholder-provided need, 23 November 2026.
               The developer needs AI agents to use codebaseGraph
               automatically in every session.
               Scope covers repository tasks that need codebase context on
               supported client and platform combinations under R-UC1-08
               and R-UC1-11. */

        subject system : CodebaseGraph;
        stakeholder developer : Developer;
        actor codingAgent : CodingAgent;

        require constraint {
            doc /* When a supported session first needs repository context
                   and the graph is usable, the coding agent shall query
                   the graph before opening source files for that task,
                   without a manual reminder. */
        }

        // Assess graph-first use from session traces. Record attempts,
        // successful use, query failures and unavailable sessions. Report
        // totals for all sessions exercised, including unavailable sessions.
    }

    concern def RapidCodebaseUnderstanding {
        doc /* N6. Stakeholder-provided need, 23 November 2026.
               Using codebaseGraph should enable AI agents to understand
               the codebase quickly.
               Scenario: an agent establishes enough understanding to begin
               a repository task or assess a proposed change. */

        subject system : CodebaseGraph;
        stakeholder developer : Developer;
        actor codingAgent : CodingAgent;

        require constraint {
            doc /* The coding agent needs to develop accurate, task-relevant
                   understanding of the codebase quickly through its use
                   of codebaseGraph. */
        }

        // Proposed evaluation, with no numeric threshold selected:
        // Measure elapsed time and interactions needed to answer agreed
        // code-understanding questions correctly and sufficiently completely.
        // Example tasks locate behaviour, explain dependencies, or assess
        // likely change impact. Query latency alone is not understanding.
        // Evaluate time separately from token consumption in N4.
    }

    concern def ReliableOperationAndDiskUse {
        doc /* N7. Stakeholder-provided need, 23 November 2026.
               codebaseGraph should be reliable and stable to use between
               sessions and worktrees without overloading disk space.
               Scenarios: sessions restart; operations are interrupted;
               worktrees are created, used, switched or removed. */

        subject system : CodebaseGraph;
        stakeholder developer : Developer;
        actor codingAgent : CodingAgent;
        actor worktrees : RepositoryWorktree[0..*];

        require constraint {
            doc /* The developer needs codebaseGraph to operate reliably
                   and consistently across sessions and worktrees while
                   keeping disk consumption under control. */
        }

        // Proposed evaluation; thresholds and lifecycle policies remain open:
        // Assess usability and recovery across normal session restarts and
        // interrupted work. Check that each query uses the intended worktree.
        // Exercise divergent worktrees, including creation and removal, while
        // other sessions continue to use their own repository context.
        // Measure both peak disk use during work and retained disk use after
        // work settles, across repeated update/rebuild/interruption cycles.
        // Account for graph state, indexes, caches, temporary data and logs.
        // Check that cleanup does not invalidate resources still in use.
        // Storage budgets, low-disk behaviour and worktree-sharing strategy
        // are future requirements/design decisions, not selected mechanisms.
    }

    concern def ConcurrentGraphQuerying {
        doc /* N8. Stakeholder-provided need, 23 November 2026.
               The codebase graph can be queried concurrently.
               Scenario: multiple overlapping requests query a graph from
               one or more coding agents, sessions or developer clients. */

        subject system : CodebaseGraph;
        stakeholder developer : Developer;
        actor codingAgents : CodingAgent[0..*];
        actor queryingDevelopers : Developer[0..*];

        require constraint {
            doc /* Coding agents and developers need to query codebaseGraph
                   concurrently and receive correct results for the intended
                   repository and worktree. */
        }

        // Proposed evaluation, without a selected concurrency limit:
        // Submit overlapping queries for the same worktree and for different
        // worktrees; assess correctness, completed throughput, latency and
        // failed/rejected requests as concurrent demand increases.
        // Also examine queries during graph updates and cleanup, linking
        // consistency, availability and freshness questions to N2 and N7.
        // Concurrency/overload limits and read-versus-update guarantees will
        // be derived later; this need does not prescribe concurrent writers,
        // a locking strategy, a database backend or a process arrangement.
    }

    concern codeUnderstandingNeed : UsefulCodeUnderstanding;
    concern graphStatusNeed : InterpretableGraphStatus;
    concern maintainabilityNeed : MaintainableEvolution;
    concern structuredContextNeed : TokenEfficientStructuredContext;
    concern automaticSessionUseNeed : AutomaticSessionUse;
    concern rapidUnderstandingNeed : RapidCodebaseUnderstanding;
    concern reliableOperationNeed : ReliableOperationAndDiskUse;
    concern concurrentQueryNeed : ConcurrentGraphQuerying;
}
