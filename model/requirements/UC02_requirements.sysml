package CBGUC02Requirements {
    private import CBGSystem::*;
    private import CBGNeeds::*;
    private import CBGUC02::*;

    requirement def LanguageSpecificSymbolExtraction {
        doc /* R-UC2-01. Extract symbols according to language-specific syntax
               profiles rather than the current code ontology. */
        subject system : CodebaseGraph;
    }
    requirement languageSpecificSymbolExtraction : LanguageSpecificSymbolExtraction {
        subject redefines system = design.codebaseGraph;
    }
    dependency languageSpecificSymbolExtractionUseCase from languageSpecificSymbolExtraction to uc2;
    dependency languageSpecificSymbolExtractionNeeds from languageSpecificSymbolExtraction to
        codeUnderstandingNeed;

    requirement def SymbolExtractionAccuracy {
        doc /* R-UC2-02. Verify symbol extraction against manually validated
               source samples and reference parser syntax output. */
        subject system : CodebaseGraph;
    }
    requirement symbolExtractionAccuracy : SymbolExtractionAccuracy {
        subject redefines system = design.codebaseGraph;
    }
    dependency symbolExtractionAccuracyUseCase from symbolExtractionAccuracy to uc2;
    dependency symbolExtractionAccuracyNeeds from symbolExtractionAccuracy to
        codeUnderstandingNeed, graphStatusNeed;

    requirement def SymbolIdentification {
        doc /* R-UC2-03. Provide unique symbol identifiers, fully qualified
               names, signatures and source locations, where applicable. */
        subject system : CodebaseGraph;
    }
    requirement symbolIdentification : SymbolIdentification {
        subject redefines system = design.codebaseGraph;
    }
    dependency symbolIdentificationUseCase from symbolIdentification to uc2;
    dependency symbolIdentificationNeeds from symbolIdentification to
        codeUnderstandingNeed;

    requirement def ImplementationRepresentation {
        doc /* R-UC2-04. Provide a unified, compact, structured representation
               of implementation logic with language-specific extensions. */
        subject system : CodebaseGraph;
    }
    requirement implementationRepresentation : ImplementationRepresentation {
        subject redefines system = design.codebaseGraph;
    }
    dependency implementationRepresentationUseCase from implementationRepresentation to uc2;
    dependency implementationRepresentationNeeds from implementationRepresentation to
        codeUnderstandingNeed, structuredContextNeed, rapidUnderstandingNeed;

    requirement def ImplementationCoverage {
        doc /* R-UC2-05. Represent signatures, parameters, return types, control
               flow, function calls, data operations, error handling, state
               mutations and side effects. */
        subject system : CodebaseGraph;
    }
    requirement implementationCoverage : ImplementationCoverage {
        subject redefines system = design.codebaseGraph;
    }
    dependency implementationCoverageUseCase from implementationCoverage to uc2;
    dependency implementationCoverageNeeds from implementationCoverage to
        codeUnderstandingNeed, structuredContextNeed;

    requirement def ImplementationFidelity {
        doc /* R-UC2-06. Omit syntactic details during compaction only where
               their omission does not alter the represented implementation
               logic. */
        subject system : CodebaseGraph;
    }
    requirement implementationFidelity : ImplementationFidelity {
        subject redefines system = design.codebaseGraph;
    }
    dependency implementationFidelityUseCase from implementationFidelity to uc2;
    dependency implementationFidelityNeeds from implementationFidelity to
        codeUnderstandingNeed, graphStatusNeed;

    requirement def SelectableDetailLevels {
        doc /* R-UC2-07. Provide three selectable levels: signature, compact
               implementation and detailed implementation. */
        subject system : CodebaseGraph;
    }
    requirement selectableDetailLevels : SelectableDetailLevels {
        subject redefines system = design.codebaseGraph;
    }
    dependency selectableDetailLevelsUseCase from selectableDetailLevels to uc2;
    dependency selectableDetailLevelsNeeds from selectableDetailLevels to
        codeUnderstandingNeed, structuredContextNeed;

    requirement def LanguageSpecificRelationshipTypes {
        doc /* R-UC2-08. Represent relationships using language-specific
               relationship types. */
        subject system : CodebaseGraph;
    }
    requirement languageSpecificRelationshipTypes : LanguageSpecificRelationshipTypes {
        subject redefines system = design.codebaseGraph;
    }
    dependency languageSpecificRelationshipTypesUseCase from languageSpecificRelationshipTypes to uc2;
    dependency languageSpecificRelationshipTypesNeeds from languageSpecificRelationshipTypes to
        codeUnderstandingNeed;

    requirement def DirectRelationshipEvidence {
        doc /* R-UC2-09. Establish relationships only where supported by direct
               code evidence, including source location, relevant syntax
               constructs and static resolution evidence where applicable. */
        subject system : CodebaseGraph;
    }
    requirement directRelationshipEvidence : DirectRelationshipEvidence {
        subject redefines system = design.codebaseGraph;
    }
    dependency directRelationshipEvidenceUseCase from directRelationshipEvidence to uc2;
    dependency directRelationshipEvidenceNeeds from directRelationshipEvidence to
        codeUnderstandingNeed, graphStatusNeed;

    requirement def CrossLanguageRelationships {
        doc /* R-UC2-10. Represent cross-language relationships where supported
               by direct code evidence. */
        subject system : CodebaseGraph;
    }
    requirement crossLanguageRelationships : CrossLanguageRelationships {
        subject redefines system = design.codebaseGraph;
    }
    dependency crossLanguageRelationshipsUseCase from crossLanguageRelationships to uc2;
    dependency crossLanguageRelationshipsNeeds from crossLanguageRelationships to
        codeUnderstandingNeed, graphStatusNeed;

    requirement def UnresolvedReferences {
        doc /* R-UC2-11. Explicitly expose unresolved references without
               creating unsupported relationships. */
        subject system : CodebaseGraph;
    }
    requirement unresolvedReferences : UnresolvedReferences {
        subject redefines system = design.codebaseGraph;
    }
    dependency unresolvedReferencesUseCase from unresolvedReferences to uc2;
    dependency unresolvedReferencesNeeds from unresolvedReferences to
        codeUnderstandingNeed, graphStatusNeed;

    requirement def SymbolSearch {
        doc /* R-UC2-12. Support symbol searches using names, qualified names,
               signatures and identifiers. */
        subject system : CodebaseGraph;
    }
    requirement symbolSearch : SymbolSearch {
        subject redefines system = design.codebaseGraph;
    }
    dependency symbolSearchUseCase from symbolSearch to uc2;
    dependency symbolSearchNeeds from symbolSearch to
        codeUnderstandingNeed, rapidUnderstandingNeed;

    requirement def NaturalLanguageSearch {
        doc /* R-UC2-13. Support natural-language searches across symbol names,
               signatures, documentation and compact implementation
               representations. */
        subject system : CodebaseGraph;
    }
    requirement naturalLanguageSearch : NaturalLanguageSearch {
        subject redefines system = design.codebaseGraph;
    }
    dependency naturalLanguageSearchUseCase from naturalLanguageSearch to uc2;
    dependency naturalLanguageSearchNeeds from naturalLanguageSearch to
        codeUnderstandingNeed, rapidUnderstandingNeed;

    requirement def SearchRanking {
        doc /* R-UC2-14. Rank search results using a combination of query
               relevance and graph relationships. */
        subject system : CodebaseGraph;
    }
    requirement searchRanking : SearchRanking {
        subject redefines system = design.codebaseGraph;
    }
    dependency searchRankingUseCase from searchRanking to uc2;
    dependency searchRankingNeeds from searchRanking to
        codeUnderstandingNeed, rapidUnderstandingNeed;

    requirement def SearchFiltering {
        doc /* R-UC2-15. Allow searches to be restricted by directories, files,
               languages and symbol types. */
        subject system : CodebaseGraph;
    }
    requirement searchFiltering : SearchFiltering {
        subject redefines system = design.codebaseGraph;
    }
    dependency searchFilteringUseCase from searchFiltering to uc2;
    dependency searchFilteringNeeds from searchFiltering to
        codeUnderstandingNeed, rapidUnderstandingNeed;

    requirement def RelationshipTraversal {
        doc /* R-UC2-16. Return connected symbols, relationship types,
               intermediate paths and compact implementation information. */
        subject system : CodebaseGraph;
    }
    requirement relationshipTraversal : RelationshipTraversal {
        subject redefines system = design.codebaseGraph;
    }
    dependency relationshipTraversalUseCase from relationshipTraversal to uc2;
    dependency relationshipTraversalNeeds from relationshipTraversal to
        codeUnderstandingNeed, structuredContextNeed, rapidUnderstandingNeed;

    requirement def TraversalControls {
        doc /* R-UC2-17. Allow agents to specify traversal depth, relationship
               types and token budgets. */
        subject system : CodebaseGraph;
    }
    requirement traversalControls : TraversalControls {
        subject redefines system = design.codebaseGraph;
    }
    dependency traversalControlsUseCase from traversalControls to uc2;
    dependency traversalControlsNeeds from traversalControls to
        codeUnderstandingNeed, structuredContextNeed, rapidUnderstandingNeed;

    requirement def ChangeImpact {
        doc /* R-UC2-18. Identify potentially affected symbols through direct
               and indirect evidence-supported relationships. */
        subject system : CodebaseGraph;
    }
    requirement changeImpact : ChangeImpact {
        subject redefines system = design.codebaseGraph;
    }
    dependency changeImpactUseCase from changeImpact to uc2;
    dependency changeImpactNeeds from changeImpact to
        codeUnderstandingNeed;

    requirement def PotentialExecutionPaths {
        doc /* R-UC2-19. Trace statically supported potential execution paths
               across symbols, including conditional branches and error-handling
               paths. */
        subject system : CodebaseGraph;
    }
    requirement potentialExecutionPaths : PotentialExecutionPaths {
        subject redefines system = design.codebaseGraph;
    }
    dependency potentialExecutionPathsUseCase from potentialExecutionPaths to uc2;
    dependency potentialExecutionPathsNeeds from potentialExecutionPaths to
        codeUnderstandingNeed, graphStatusNeed;

    requirement def StaticDataFlow {
        doc /* R-UC2-20. Trace statically supported data flows within and across
               functions and methods. */
        subject system : CodebaseGraph;
    }
    requirement staticDataFlow : StaticDataFlow {
        subject redefines system = design.codebaseGraph;
    }
    dependency staticDataFlowUseCase from staticDataFlow to uc2;
    dependency staticDataFlowNeeds from staticDataFlow to
        codeUnderstandingNeed, graphStatusNeed;

    requirement def AssociatedTestDiscovery {
        doc /* R-UC2-21. Identify tests directly and indirectly associated with
               symbols through statically traceable relationships. */
        subject system : CodebaseGraph;
    }
    requirement associatedTestDiscovery : AssociatedTestDiscovery {
        subject redefines system = design.codebaseGraph;
    }
    dependency associatedTestDiscoveryUseCase from associatedTestDiscovery to uc2;
    dependency associatedTestDiscoveryNeeds from associatedTestDiscovery to
        codeUnderstandingNeed;

    requirement def ExternalDependencyReferences {
        doc /* R-UC2-22. Represent external dependency references without
               analysing their implementations. */
        subject system : CodebaseGraph;
    }
    requirement externalDependencyReferences : ExternalDependencyReferences {
        subject redefines system = design.codebaseGraph;
    }
    dependency externalDependencyReferencesUseCase from externalDependencyReferences to uc2;
    dependency externalDependencyReferencesNeeds from externalDependencyReferences to
        codeUnderstandingNeed;

    requirement def SymbolDocumentation {
        doc /* R-UC2-23. Associate symbols with docstrings, documentation
               comments and explicitly linked repository documentation. */
        subject system : CodebaseGraph;
    }
    requirement symbolDocumentation : SymbolDocumentation {
        subject redefines system = design.codebaseGraph;
    }
    dependency symbolDocumentationUseCase from symbolDocumentation to uc2;
    dependency symbolDocumentationNeeds from symbolDocumentation to
        codeUnderstandingNeed;

    requirement def ResolvableTypeInformation {
        doc /* R-UC2-24. Expose explicitly declared and statically resolvable
               types without inventing unresolved types. */
        subject system : CodebaseGraph;
    }
    requirement resolvableTypeInformation : ResolvableTypeInformation {
        subject redefines system = design.codebaseGraph;
    }
    dependency resolvableTypeInformationUseCase from resolvableTypeInformation to uc2;
    dependency resolvableTypeInformationNeeds from resolvableTypeInformation to
        codeUnderstandingNeed, graphStatusNeed;

    requirement def SourceTraceability {
        doc /* R-UC2-25. Provide source file paths and line ranges for returned
               symbols and their supporting evidence. */
        subject system : CodebaseGraph;
    }
    requirement sourceTraceability : SourceTraceability {
        subject redefines system = design.codebaseGraph;
    }
    dependency sourceTraceabilityUseCase from sourceTraceability to uc2;
    dependency sourceTraceabilityNeeds from sourceTraceability to
        codeUnderstandingNeed, graphStatusNeed;

    requirement def QueryTokenBudget {
        doc /* R-UC2-26. Keep query responses within the agent-defined token
               budget, using an agreed token-counting method. */
        subject system : CodebaseGraph;
    }
    requirement queryTokenBudget : QueryTokenBudget {
        subject redefines system = design.codebaseGraph;
    }
    dependency queryTokenBudgetUseCase from queryTokenBudget to uc2;
    dependency queryTokenBudgetNeeds from queryTokenBudget to
        structuredContextNeed;

    requirement def BudgetOptimisation {
        doc /* R-UC2-27. Prioritise relevant results, reduce detail where
               necessary and explicitly indicate omitted information. */
        subject system : CodebaseGraph;
    }
    requirement budgetOptimisation : BudgetOptimisation {
        subject redefines system = design.codebaseGraph;
    }
    dependency budgetOptimisationUseCase from budgetOptimisation to uc2;
    dependency budgetOptimisationNeeds from budgetOptimisation to
        graphStatusNeed, structuredContextNeed;

    requirement def SelectableOutputFormat {
        doc /* R-UC2-28. Return compact structured text by default, with
               optional JSON output selectable by the agent. */
        subject system : CodebaseGraph;
    }
    requirement selectableOutputFormat : SelectableOutputFormat {
        subject redefines system = design.codebaseGraph;
    }
    dependency selectableOutputFormatUseCase from selectableOutputFormat to uc2;
    dependency selectableOutputFormatNeeds from selectableOutputFormat to
        structuredContextNeed;

    requirement def TokenEfficiency {
        doc /* R-UC2-29. Reduce total token consumption compared with direct
               file exploration for equivalent tasks and comparable accuracy.
               Target: TBD through benchmarking. */
        subject system : CodebaseGraph;
    }
    requirement tokenEfficiency : TokenEfficiency {
        subject redefines system = design.codebaseGraph;
    }
    dependency tokenEfficiencyUseCase from tokenEfficiency to uc2;
    dependency tokenEfficiencyNeeds from tokenEfficiency to
        structuredContextNeed;

    requirement def UnderstandingSpeed {
        doc /* R-UC2-30. Reduce the time agents require to achieve accurate,
               task-relevant codebase understanding compared with direct file
               exploration. Target: TBD through benchmarking. */
        subject system : CodebaseGraph;
    }
    requirement understandingSpeed : UnderstandingSpeed {
        subject redefines system = design.codebaseGraph;
    }
    dependency understandingSpeedUseCase from understandingSpeed to uc2;
    dependency understandingSpeedNeeds from understandingSpeed to
        rapidUnderstandingNeed;

    requirement def RetrievalQuality {
        doc /* R-UC2-31. Meet agreed search relevance, precision and recall
               criteria. Targets: TBD through benchmarking. */
        subject system : CodebaseGraph;
    }
    requirement retrievalQuality : RetrievalQuality {
        subject redefines system = design.codebaseGraph;
    }
    dependency retrievalQualityUseCase from retrievalQuality to uc2;
    dependency retrievalQualityNeeds from retrievalQuality to
        codeUnderstandingNeed, rapidUnderstandingNeed;

    requirement def RepresentationAccuracy {
        doc /* R-UC2-32. Meet agreed accuracy and completeness criteria for
               symbols, implementation representations and relationships.
               Targets: TBD through benchmarking. */
        subject system : CodebaseGraph;
    }
    requirement representationAccuracy : RepresentationAccuracy {
        subject redefines system = design.codebaseGraph;
    }
    dependency representationAccuracyUseCase from representationAccuracy to uc2;
    dependency representationAccuracyNeeds from representationAccuracy to
        codeUnderstandingNeed, graphStatusNeed;
}
