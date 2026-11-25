package CBGSystem {
    private import CBGStakeholders::*;

    part def CodebaseGraph {
        doc /* Build and maintain a queryable model of source code and its
               relationships for developers and coding agents. */
    }

    part def SystemContext {
        part codebaseGraph : CodebaseGraph;

        ref part repository : SourceRepository;
        ref part worktrees : RepositoryWorktree[0..*];
        ref part developers : Developer[0..*];
        ref part codingAgents : CodingAgent[0..*];
        ref part agentHosts : AgentHost[0..*];
        ref part maintainers : Maintainer[0..*];
    }

    part design : SystemContext;
}
