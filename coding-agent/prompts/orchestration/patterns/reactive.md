## Pattern: Reactive coordination with documents + py_repl

Use this when a downstream thread must wait for shared artifacts or readiness signals before it can do useful work.

- Launch producers with `tau.launch(...)` so they can run in the background.
- Producers should publish intermediate findings to documents early and often.
- Poll document readiness or use short `tau.wait(..., timeout=...)` barriers from `py_repl`.
- Launch the dependent critic/reviewer/synthesizer only after the readiness gate passes.
- Keep document semantics simple: use polling first, not subscriptions.

```python
pro = tau.launch("position-for",
                 "Build the FOR case and append anchor facts to document 'pro_case_notes'",
                 tools=["read"], max_turns=50)
con = tau.launch("position-against",
                 "Build the AGAINST case and append anchor facts to document 'con_case_notes'",
                 tools=["read"], max_turns=50)

while True:
    pro_notes = tau.document("read", name="pro_case_notes")
    con_notes = tau.document("read", name="con_case_notes")
    if "PRO_" in pro_notes and "CON_" in con_notes:
        break
    tau.wait([pro, con], timeout=1)

critic = tau.thread("critic",
                    "Critique both sides using documents 'pro_case_notes' and 'con_case_notes'",
                    tools=["read"], max_turns=50)
```

This is the default control plane for reactive workflows. Do not launch the dependent thread in the same batch just because you know its alias ahead of time.
