## Pattern: Pipeline (phased execution)

<!-- TODO: Write this pattern. Key idea: threads with data dependencies run in
     phases. Phase 1 threads complete, their episodes are injected into Phase 2
     threads via the `episodes` parameter. The orchestrator must plan the
     dependency graph before dispatching. -->
