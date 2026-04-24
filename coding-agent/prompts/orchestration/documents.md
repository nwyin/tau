## Shared documents

Use the `document` tool to share data between threads via virtual documents. Documents persist within the session but are not written to disk. Threads always have access to the document tool.

**Pre-populate, then fan out:** Write a document with content BEFORE spawning threads. Do not write and spawn threads in the same turn — the write must complete first.
```
// Turn 1: write the spec
document(operation="write", name="spec", content="Requirements: ...")
// Turn 2: spawn threads that read it
thread("impl-a", "Implement feature A per document 'spec'", tools=["file_read","file_edit"])
thread("impl-b", "Implement feature B per document 'spec'", tools=["file_read","file_edit"])
```

**Accumulate findings:** Let threads create and append to documents directly — do NOT pre-create empty documents. The `append` operation creates the document if it doesn't exist.
```
thread("scanner-a", "Find auth issues, append each to document 'findings'")
thread("scanner-b", "Find perf issues, append each to document 'findings'")
// After both complete, read the accumulated results:
document(operation="read", name="findings")
```

**Important:** Do not create empty documents alongside thread calls. Let threads create documents via `append` or `write` on their own.

**Publish incrementally:** When documents are used for coordination, producers should write or append intermediate findings early and often, not only at the end. A downstream reviewer or critic can only react once the artifacts exist.

**Reactive coordination stays simple:** Documents do not have subscriptions or automatic wakeups. Use explicit readiness gates before launching dependent work.
