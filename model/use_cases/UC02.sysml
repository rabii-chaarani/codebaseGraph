package CBGUC02 {
    private import CBGStakeholders::*;
    private import CBGSystem::*;
    private import CBGNeeds::*;

    use case def UnderstandCodebase {
        doc /* UC2. An AI agent explores the codebase at symbol and
               implementation level through symbol searches, natural-language
               searches and relationship traversal. codebaseGraph provides
               compact, structured, source-traceable information at predefined
               levels of detail, enabling accurate and rapid codebase
               understanding while reducing token consumption.
               Out of scope: automated implementation comparison, external
               dependency implementation analysis, dynamic runtime tracing and
               change comparisons. */

        subject system : CodebaseGraph;
        actor agent : CodingAgent;

        objective {
            doc /* The agent achieves accurate, task-relevant understanding
                   faster and with fewer tokens than direct file exploration. */
        }
    }

    use case uc2 : UnderstandCodebase {
        subject redefines system = design.codebaseGraph;
    }

    dependency uc2Needs from uc2 to
        codeUnderstandingNeed, graphStatusNeed, structuredContextNeed,
        rapidUnderstandingNeed;
}
