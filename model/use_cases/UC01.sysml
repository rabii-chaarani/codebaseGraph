package CBGUC01 {
    private import CBGStakeholders::*;
    private import CBGSystem::*;
    private import CBGNeeds::*;

    use case def BeginAndMaintainAgentSession {
        doc /* UC1. At session start after repository setup, codebaseGraph
               supplies tool-use instructions and graph status for the selected
               repository, branch and worktree. It provides structured context
               when queried, automatically builds or refreshes missing or stale
               context, and updates the graph live on saved edits and branch or
               worktree switches, with clear freshness and readiness status. */

        subject system : CodebaseGraph;
        actor agent : CodingAgent;
        actor host : AgentHost;
        actor worktree : RepositoryWorktree;

        objective {
            doc /* The agent completes its task using correct graph context,
                   without manual graph maintenance, service interruptions,
                   or unreported or prolonged staleness. */
        }
    }

    use case uc1 : BeginAndMaintainAgentSession {
        subject redefines system = design.codebaseGraph;
    }

    // Traceability to needs; these dependencies assert no satisfaction.
    dependency uc1PrimaryNeeds from uc1 to
        automaticSessionUseNeed, graphStatusNeed, reliableOperationNeed;
    dependency uc1ContextNeeds from uc1 to
        codeUnderstandingNeed, structuredContextNeed, rapidUnderstandingNeed;
    dependency uc1ConcurrencyVariant from uc1 to concurrentQueryNeed;
}
