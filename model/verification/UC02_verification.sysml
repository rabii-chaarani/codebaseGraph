package CBGUC02Verification {
    private import CBGSystem::*;
    private import CBGUC02Requirements::*;

    doc /* Planned verification for UC2. Benchmark thresholds remain open
           where the requirements record them as TBD. */

    verification def SymbolExtractionAndImplementationFidelity {
        doc /* Compare extracted symbols against manually validated source
               samples and reference parser syntax output. Check identifiers,
               qualified names, signatures, source locations and all three
               detail levels. Review the compact and detailed representations
               against source examples covering parameters, returns, control
               flow, calls, data operations, error handling, state mutations,
               side effects and language-specific extensions. Check that
               compaction preserves the represented implementation logic. */
        subject system : CodebaseGraph;
        objective {
            verify languageSpecificSymbolExtraction;
            verify symbolExtractionAccuracy;
            verify symbolIdentification;
            verify implementationRepresentation;
            verify implementationCoverage;
            verify implementationFidelity;
            verify selectableDetailLevels;
        }
    }

    verification def RelationshipAndSourceEvidence {
        doc /* Use fixtures with known same-language and cross-language links,
               unresolved references, external dependencies, symbol documentation
               and declared and resolvable types. Check relationship kinds, source
               paths and line ranges, supporting syntax and static resolution
               evidence where applicable. Confirm that uncertain references
               stay unresolved, external dependencies appear as references, and
               repository documentation associations have explicit links. */
        subject system : CodebaseGraph;
        objective {
            verify languageSpecificRelationshipTypes;
            verify directRelationshipEvidence;
            verify crossLanguageRelationships;
            verify unresolvedReferences;
            verify externalDependencyReferences;
            verify symbolDocumentation;
            verify resolvableTypeInformation;
            verify sourceTraceability;
        }
    }

    verification def SearchAndTraversal {
        doc /* Search fixtures by name, qualified name, signature, identifier
               and natural language. Check ranking against query relevance and
               graph relationships. Exercise each requested filter. Traverse
               fixtures with known intermediate paths and vary depth, relationship
               types and token budget to check controls and returned details. */
        subject system : CodebaseGraph;
        objective {
            verify symbolSearch;
            verify naturalLanguageSearch;
            verify searchRanking;
            verify searchFiltering;
            verify relationshipTraversal;
            verify traversalControls;
        }
    }

    verification def StaticAnalysisResults {
        doc /* Compare results with annotated fixtures for direct and indirect
               change impact and test associations, statically supported paths
               through conditional and error-handling branches, and data flows
               within and across functions and methods. Check supporting code
               evidence for the returned paths and relationships. */
        subject system : CodebaseGraph;
        objective {
            verify changeImpact;
            verify potentialExecutionPaths;
            verify staticDataFlow;
            verify associatedTestDiscovery;
        }
    }

    verification def BudgetAndResponseFormat {
        doc /* Exercise default structured text and explicitly selected JSON.
               In each format, cover omitted results, building and unavailable
               responses, and a query that spans a branch switch. Use the
               agreed token-counting method to calculate the minimum valid
               response size for each scenario. Try budgets one token below,
               equal to and above that minimum. Below the minimum, check for
               a nonzero exit, empty stdout and a concise stderr diagnostic
               giving the minimum. At and above the minimum, require a valid
               response in the selected format. Count complete stdout and
               check that it fits the requested budget. Check required status,
               original branch identification, freshness and omission
               information where applicable. Parse the JSON when selected.
               Check relevance ordering and the reduction of optional detail
               when only some results fit. */
        subject system : CodebaseGraph;
        objective {
            verify queryTokenBudget;
            verify budgetOptimisation;
            verify selectableOutputFormat;
        }
    }

    verification def UnderstandingEfficiencyBenchmark {
        doc /* Compare graph-assisted and direct-file exploration on matched
               tasks, using the same repository snapshot and agent/model
               conditions. Assess both against the same accuracy and completeness
               criteria. Measure total tokens and elapsed time to an accurate
               answer separately. Include startup context, tool exchanges,
               follow-up reads and retries in token accounting. Record caching
               and context reuse in both conditions. Acceptance targets are TBD. */
        subject system : CodebaseGraph;
        objective {
            verify tokenEfficiency;
            verify understandingSpeed;
        }
    }

    verification def RetrievalAndRepresentationBenchmark {
        doc /* Score relevance, precision and recall against labelled queries.
               Compare the accuracy and completeness of symbols, implementation
               representations and relationships against reviewed source fixtures.
               Record results by language and capability. Acceptance thresholds
               for both evaluations are TBD. */
        subject system : CodebaseGraph;
        objective {
            verify retrievalQuality;
            verify representationAccuracy;
        }
    }
}
