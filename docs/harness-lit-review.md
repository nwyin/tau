# Coding Agent Harnesses: Literature Review

Who is thinking about this problem, what have they figured out, and where can we make progress?

---

## Part 1: The people and organizations

### Academic labs

**Princeton NLP / Princeton Language and Intelligence** — the epicenter. Created SWE-bench (ICLR 2024 Oral), SWE-agent (NeurIPS 2024), and the Agent-Computer Interface (ACI) concept. Key people:
- **Shunyu Yao** — ReAct (25K+ citations), Tree of Thoughts, Reflexion, tau-bench, SWE-agent. His PhD thesis "Language Agents: From Next-Token Prediction to Digital Automation" is a landmark.
- **Karthik Narasimhan** — Associate Prof, co-Director Princeton NLP. Co-created SWE-agent and SWE-bench.
- **Ofir Press** — co-created SWE-agent. Postdoc at Princeton.
- **Kilian Lieret** — lead developer of SWE-agent since March 2024. Also created SWE-smith (training data generation) and SWE-ReX (parallel execution).

**CMU / All Hands AI** — the open-source platform play.
- **Graham Neubig** — Professor at LTI, Chief Scientist at All Hands AI. Created OpenHands (60K+ stars, 300+ contributors), which holds top ranks on SWE-bench Full.
- **Xingyao Wang** — CodeAct paper (ICML 2024): executable Python as unified action space outperforms alternatives by 20%.

**UC Berkeley BAIR** — "Compound AI Systems" framing (Feb 2024), arguing SOTA comes from multi-component systems, not monolithic models. Offers CS294 "Agentic AI" course.

**JetBrains Research** — "Cutting Through the Noise" (NeurIPS 2025 DL4C workshop): observation masking beats LLM summarization for context management. Practical, empirically grounded work.

### Industry teams

**Anthropic (Claude Code)** — Released Feb 2025. Architecture: straightforward agent loop, ~14 tools. Design pillars: primitives over integrations (bash, grep, edit compose anything), context economy (auto-compact at 95%, sub-agents, semantic search), tool search as meta-tool. Created MCP (Model Context Protocol) — now the emerging standard for tool integration across the ecosystem.

**OpenAI (Codex CLI)** — Rust-based, open source. Published the "Harness Engineering" blog post (Aug 2025) describing 1M+ LOC built with zero manually-written code. Codex App Server decouples agent logic from client surfaces via bidirectional protocol.

**Cursor (Anysphere)** — Trains models internally. Notable team: Jacob Jackson (invented Tabnine/Supermaven, independently invented speculative decoding), Sasha Rush (Cornell professor, efficiency/long-context), Lukas Moller (built inference engine in 2 weeks). Two-phase edit: primary LLM generates intent, custom-trained "Apply" model handles file integration.

**Cognition (Devin)** — Multi-agent operation, self-assessed confidence, automatic repo indexing. ARR grew from ~$1M (Sep 2024) to ~$73M (Jun 2025). Acquired Windsurf/Codeium for ~$250M (Dec 2025).

**Replit** — Evolved from ReAct-style single agent to multi-agent with manager + specialized editors. Does NOT use traditional function calling — wrote a restricted Python DSL for 30+ tools.

**Google (Jules)** — Perceive-Plan-Execute-Evaluate loop in cloud VMs. Async, concurrent task execution.

**Sourcegraph (Amp)** — Built from scratch as autonomous agent, not retrofitted from autocomplete. Leverages global code graph and search infrastructure.

### Independent builders

**Paul Gauthier (Aider)** — 36K+ stars. Key innovations: repository map (function signatures for whole-codebase context), multiple edit formats (whole, diff, search/replace, udiff), architect/editor two-model approach. His blog posts contain some of the most practical empirical work in the field. Showed udiff raised GPT-4 Turbo from 20% to 61%.

**Can Boluk (oh-my-pi)** — Created hashline editing. Content-hash-based line addressing that achieves 10x improvement for weak models and +8% average across 16 models. Being adopted by other projects.

**Saoud Rizwan (Cline)** — Created at an Anthropic hackathon. Plan+Act workflow, AST analysis, MCP support. Apache 2.0, model-agnostic.

---

## Part 2: Key papers and ideas

### Foundational agent architecture

1. **ReAct: Synergizing Reasoning and Acting** — Yao et al., ICLR 2023. Interleaving reasoning traces with actions. 25K+ citations. The paradigm every coding agent builds on.

2. **SWE-agent: Agent-Computer Interfaces Enable Automated Software Engineering** — Yang, Jimenez et al., NeurIPS 2024. Introduced the ACI concept: interfaces designed for LM agents outperform human UIs. ACI adds 10.7 percentage points over default Linux shell.

3. **CodeAct: Executable Code Actions Elicit Better LLM Agents** — Wang et al., ICML 2024. Using Python as a unified action space outperforms JSON/text actions by up to 20%.

4. **Building Effective AI Coding Agents for the Terminal** — Bui, arXiv 2603.05344. Presents OPENDEV. Distinguishes scaffolding (pre-first-prompt assembly) from harness (runtime orchestration). Adaptive context compaction, model routing, lazy tool discovery.

### Edit strategy research

5. **Prompting LLMs for Code Editing: Struggles and Remedies** — Analysis of developer logs at Google. arXiv 2504.20196.

6. **Diff-XYZ Benchmark** — arXiv 2510.12487. Academic benchmark for evaluating diff understanding. udiff best for apply/anti-apply; search/replace best for generation.

7. **Aider blog: "Unified diffs make GPT-4 Turbo 3X less lazy"** — Showed edit format choice dramatically affects model behavior. Not a paper but more empirically useful than most papers.

### Context management

8. **Cutting Through the Noise** — JetBrains Research, NeurIPS 2025 DL4C. Observation masking (+2.6% solve rate, 52% cost reduction) beats LLM summarization. Summarization paradoxically made agents run 13-15% longer by masking stopping signals.

9. **Context Rot: How Increasing Input Tokens Impacts LLM Performance** — Chroma Research. Agent-generated context quickly becomes noise.

### Multi-agent systems

10. **EvoMAC** — ICLR 2025. Adaptable multi-agent coding networks with textual back-propagation.

11. **MapCoder** — ACL 2024. Retrieval, planning, coding, and debugging agents for competitive problems.

### Planning

12. **Plan-and-Act** — ICML-level work. Without planning (ReAct baseline): 36.97%. With trained planner: 57.58%. Planning's contribution dominates over execution quality.

### Training for tool use

13. **SWE-Gym** — Pan, Wang, Neubig et al., ICML 2025. First environment for training SWE agents. Fine-tuning on 491 trajectories: +14% on SWE-bench Verified.

14. **OpenHands LM 32B** — Nov 2025. 37.2% on SWE-bench Verified from a 32B model, comparable to 671B DeepSeek V3. Runs on a single 3090 GPU.

### Meta-analysis

15. **Confucius Code Agent (CCA)** — Meta + Harvard, arXiv 2512.10398. Most rigorous ablation: context management alone adds +6.6 points. "Agent scaffolding, not just model capability, is a primary determinant of agent performance."

---

## Part 3: The influential blog posts

The field moves faster than academic publishing. These posts shaped practitioner thinking:

- **OpenAI — "Harness Engineering"** (Aug 2025): 1M+ LOC, zero human code. Established "harness engineering" as a discipline.
- **Anthropic — "Effective Harnesses for Long-Running Agents"**: Multi-context-window workflows, different prompts per context window.
- **LangChain — "Improving Deep Agents with Harness Engineering"**: Top 30 → Top 5 on Terminal-Bench by harness changes alone.
- **Lance Martin — "Context Engineering for Agents"** (Jun 2025): Filling context windows with the right information at each step.
- **Spotify Engineering — "Background Coding Agents: Context Engineering"** (Nov 2025): Production lessons.
- **swyx — "Agent Engineering"** (2025 AI Engineer Summit): "Model Labs" (lightweight harness, bet on next model) vs "Agent Labs" (speed, auditability, human-in-loop, rewrite every few months).
- **Aakash Gupta — "2025 Was Agents. 2026 Is Agent Harnesses"**: "The model is commodity, the harness is moat."
- **Addy Osmani — "The 80% Problem"**: Agents generate 80% correct but the remaining 20% contains critical logic errors.

---

## Part 4: Open questions where we can make progress

These are ordered by how tractable they are given tau's current foundation.

### 1. Edit reliability (HIGH tractability)

The single most impactful area for harness engineering. No universally reliable edit format exists.

**Current state:**
- Search/replace: 70-84% accuracy, fails on whitespace, ambiguous matches
- Unified diff: 80-85%, complex format errors
- Patch (Codex): 50%+ failure on non-OpenAI models
- Hashline: +8% avg across 16 models, 10x for weak models

**Where tau can contribute:** We already have hashline. The open questions are:
- Does hashline's re-read-after-edit overhead net out vs. its accuracy gains on multi-step tasks?
- Can hashline be combined with multi-edit (batch several edits before re-reading)?
- Head-to-head benchmark: hashline vs exact-match vs patch across models and task types on SWE-bench
- What's the right granularity for hash anchors — per-line, per-block, per-function?

### 2. Context management (HIGH tractability)

JetBrains showed observation masking beats summarization. But the design space is barely explored.

**Where tau can contribute:**
- Implement and benchmark observation masking vs. compaction vs. summarization on real coding tasks
- The OPENDEV paper describes adaptive compaction (progressively reduce older observations). tau could implement this and measure the effect.
- Sub-agent isolation: does spawning a search sub-agent (returns 1K token summary instead of 10K raw results) improve or hurt downstream task success?
- Cache reuse across similar tasks — do agents benefit from seeing how similar tasks were solved before?

### 3. Tool set optimization (MEDIUM tractability)

No one has done systematic research on optimal tool set size or composition.

**Where tau can contribute:**
- Ablation study: run SWE-bench with {bash-only} vs {bash+grep+glob} vs {bash+grep+glob+ls} and measure resolve rate AND token usage
- Tool description engineering: does system prompt wording for when to use grep vs bash affect tool selection accuracy?
- opencode's model-aware tool switching is the most sophisticated approach — tau could implement conditional toolsets and benchmark across models
- Measure the token cost of tool definitions: how many tokens do 6 vs 10 vs 20 tool schemas consume, and does this crowd out useful context?

### 4. Implicit LSP feedback (MEDIUM tractability)

opencode wires LSP diagnostics into edit/write tool results automatically. The model sees type errors without calling an LSP tool.

**Where tau can contribute:**
- Wire `cargo check` / `tsc` / `ruff check` output into file_edit results
- Measure: does immediate compiler feedback after edits reduce the number of edit-test-fix cycles?
- This is lower-hanging fruit than full LSP integration — just run a check command after each edit and append diagnostics

### 5. Verification and self-repair (MEDIUM-LOW tractability)

Agents can generate code but cannot reliably verify their own output. The "80% problem."

**Where tau can contribute:**
- SWE-Gym verifiers (reward models trained on agent trajectories) achieve 32% on SWE-bench Verified at K=16. Can simpler heuristic verifiers (runs tests? passes linter? compiles?) get close?
- "Fresh-context review" — after completing a task, re-read the changed files with clean context and critique. Does this catch errors?
- Multi-pass: generate, test, fix loop with explicit test output in context. Measure how many fix-cycles are needed and whether they converge.

### 6. Cost-effective model routing (MEDIUM-LOW tractability)

Premium models cost 60-300x more than lightweight ones. 80% of agent actions don't need frontier capability.

**Where tau can contribute:**
- Task classification: which tool calls need o3/Opus and which can use Haiku/Flash? Grep results processing is cheap; architectural planning is expensive.
- Agentic Plan Caching (APC): extract and reuse structured plan templates across similar tasks (46% cost reduction with 96.67% performance retention in the paper).
- Measure: does routing simple tool-result-processing to a cheap model degrade task completion?

### 7. Multi-agent coordination (LOW tractability, but relevant)

Every major tool now ships multi-agent. But coherence loss is real.

**Where tau can contribute (via hive):**
- tau already has hive (queen + workers). The open question: does hive-style coordination (human-like issue decomposition, sequential merging) produce more coherent output than equal-peer parallelism?
- Measure merge conflict rates and architectural consistency across different coordination strategies
- The "50 First Dates" problem (agents forget context between sessions) — does persistent queen-context help?

### 8. Training data for tool use (LOW tractability, high potential)

SWE-Gym showed fine-tuning on 491 trajectories gives +14%. But training is scaffold-dependent.

**Where tau can contribute long-term:**
- Generate agent trajectories using tau's scaffold on SWE-Gym tasks
- Fine-tune open models (Qwen 32B) on tau-specific trajectories — do they learn hashline editing?
- The scaffold generalization problem: models trained on OpenHands don't work well with other harnesses. Is this fundamental or just insufficient training data?

---

## Part 5: The meta-narrative

The field is converging on several truths:

1. **"The harness is the product."** The model is increasingly commodity. Same model, different scaffold, 22-point score swing. Harness engineering is what matters.

2. **Simpler often wins.** Observation masking > summarization. Hashline > complex diff formats. Six tools > thirty tools. The pattern holds across every dimension.

3. **Edit reliability is the bottleneck.** Models can reason about code. They struggle to express changes to files reliably. Every harness that invests in edit strategy sees outsized returns.

4. **Context is everything.** "Context engineering" has emerged as a discipline. What the model sees at each step determines success more than which model you use.

5. **Verification is the next frontier.** Generation is approaching "good enough." Knowing whether the generated code is correct remains unsolved. Whoever cracks self-verification unlocks fully autonomous coding.

6. **Multi-agent is early.** Everyone is shipping it. No one has solved coherence loss. The METR study showing developers are 19% slower (while thinking they're 20% faster) with AI tools should give pause.

7. **The academic-industry gap is closing.** Princeton NLP creates the benchmarks and frameworks. Industry builds the products. The feedback loop is tight — SWE-agent influences Claude Code influences SWE-bench Pro.

---

## Conferences and workshops

- **NeurIPS DL4C (Deep Learning for Code)** — 4th edition in 2025. The main venue for coding agent research. Topics: designing/stress-testing coding agents, benchmarks, human-agent collaboration.
- **ICML PRAL (Programmatic Representations for Agent Learning)** — 2025. Code and symbolic programs for agent learning.
- **ICLR** — SWE-bench (2024 Oral), tau-bench (2025), EvoMAC (2025), FeatureBench (2026).
- **ICML** — CodeAct (2024), SWE-Gym (2025).
- **ACL** — MapCoder (2024).
- **Latent Space podcast** — Key episodes on Cline, NeurIPS 2024 agents recap, Shunyu Yao on language agents.
