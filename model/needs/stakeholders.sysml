package CBGStakeholders {
    part def SourceRepository;
    part def RepositoryWorktree {
        // External working tree; graph storage/sharing is not specified here.
        ref part repository : SourceRepository;
    }
    part def Developer;
    part def CodingAgent;
    part def AgentHost {
        doc /* The external application or harness that runs the coding
               agent and can supply session context through its configured
               codebaseGraph integration. */
    }
    part def Maintainer;
}
