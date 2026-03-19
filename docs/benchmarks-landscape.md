# Coding Agent Benchmarks Landscape

A survey of benchmarks for evaluating coding agents and harnesses, organized by what they actually measure.

## The key distinction

Most benchmarks measure **model capability** — can the LLM generate correct code? A smaller but more important set measures **harness quality** — does the scaffolding (tools, system prompt, context management, edit strategy) make the agent better or worse?

The harness-sensitive benchmarks show alarming numbers: **same model, different scaffold, 17-22 point score swings.** That's larger than the gap between most frontier models.

---

## Tier 1: Harness-sensitive benchmarks

These are the benchmarks where harness engineering measurably moves the score.

### SWE-bench family

The dominant ecosystem. Real GitHub issues, real repos, real test suites.

| Variant | Scale | Created by | Key property |
|---------|-------|------------|-------------|
| [SWE-bench](https://www.swebench.com/) | 2,294 tasks, 12 Python repos | Princeton NLP (Jimenez, Yang, Yao, Narasimhan) | The original. ICLR 2024 Oral. |
| [SWE-bench Lite](https://www.swebench.com/) | 300 tasks | Princeton NLP | Filtered subset, faster eval cycle |
| [SWE-bench Verified](https://www.swebench.com/) | 500 tasks | Princeton NLP + OpenAI | Human-verified solvable. Now contaminated — OpenAI recommends SWE-bench Pro instead. |
| [SWE-bench Pro](https://arxiv.org/abs/2509.16941) | 1,865 tasks, 41 repos | Scale AI | Enterprise-difficulty. Hours-to-days of engineer time. Top scores ~23% vs 70%+ on Verified. |
| [SWE-bench Live](https://swe-bench-live.github.io/) | 50+ new/month | Microsoft Research | Continuously growing, temporally fresh, prevents contamination |
| [SWE-rebench](https://swe-rebench.com) | 21,000+ tasks | Community | Automated pipeline. 5 runs per model. Explicitly tracks contamination. |
| [Multi-SWE-bench](https://arxiv.org/abs/2504.02605) | 1,632 tasks, 7 languages | ByteDance Seed | Addresses Python-only limitation |
| [SWE-bench Multimodal](https://arxiv.org/abs/2410.03859) | 617 tasks, 17 JS libs | Princeton NLP | Bug reports include images |

**Why it measures harness quality:** Same model with SWE-agent vs OpenHands vs Agentless produces very different scores. On SWE-bench Pro, a basic scaffold scores 23% while an optimized 250-turn scaffold scores 45%+ — a **22-point swing from scaffolding alone.** SEAL leaderboard data shows Opus 4.5 at 45.9% with standardized scaffolding vs 49.8-51.8% with custom scaffolding.

**Top scores (March 2026):** Claude Opus 4.5: 80.9%, Claude Opus 4.6: 80.8%, Gemini 3.1 Pro: 80.6% on Verified. On Pro: ~23% for top models.

### Terminal-Bench

- **URL:** https://www.tbench.ai/ | [Paper](https://arxiv.org/abs/2601.11868)
- **Created by:** Mike Merrill, Alexander Shaw, Nicholas Carlini + 84 co-authors (Stanford / Laude Institute)
- **Scale:** 89 curated tasks (v2.0) in Docker environments
- **What it measures:** End-to-end terminal agent capability — compiling code, training ML models, configuring servers, reverse engineering, scientific workflows.
- **The harness engineering proof point:** LangChain improved from 52.8% to 66.5% (**+13.7 points**) by ONLY changing the harness (system prompt, tool choice, execution flow) while keeping the model fixed.
- **Infrastructure sensitivity:** Anthropic showed resource configuration alone swings scores by **+6 points** (p < 0.01). Differences below 3 points are noise.
- **Top scores:** GPT-5.3-Codex: 77.3%, Claude Code (Opus 4.6): 65.4%, Gemini 3 Pro: 54.2%.

### Aider Polyglot Benchmark

- **URL:** https://aider.chat/docs/leaderboards/
- **Created by:** Paul Gauthier (Aider)
- **Scale:** 225 Exercism exercises across 6 languages (C++, Go, Java, JS, Python, Rust)
- **What it measures:** Code editing skill with two-pass retry. Directly compares edit formats.
- **Why it's harness-relevant:** The only benchmark that isolates edit strategy impact. Unified diffs raised GPT-4 Turbo from 20% to 61%. Different edit formats produce wildly different scores with the same model. Also tracks malformed responses, syntax errors, indentation failures.
- **Top scores:** GPT-5: 88.0%, o3-pro: 84.9%, Gemini 2.5 Pro: 83.1%.

### FeatureBench

- **Paper:** [arxiv 2602.10975](https://arxiv.org/abs/2602.10975) (ICLR 2026)
- **Created by:** LiberCoders
- **Scale:** 200 tasks, 3,825 environments, 24 repos
- **What it measures:** Feature development (not just bug fixing). The massive difficulty gap reveals scaffolding limitations: Claude 4.5 Opus scores 74.4% on SWE-bench but only **11.0% on FeatureBench.**

---

## Tier 2: Model capability benchmarks (still useful context)

These measure raw code generation quality. They don't differentiate harnesses, but they establish the baseline the harness builds on.

### Function-level generation

| Benchmark | Scale | Created by | Status |
|-----------|-------|------------|--------|
| [HumanEval](https://github.com/openai/human-eval) | 164 Python tasks | OpenAI (2021) | Saturated. Frontier models >90%. |
| [MBPP](https://arxiv.org/abs/2108.07732) | 974 tasks | Google Research (2021) | Larger but similar difficulty |
| [MultiPL-E](https://github.com/nuprl/MultiPL-E) | HumanEval+MBPP in 18+ languages | Northeastern (NUPRL) | Multilingual extension |
| [BigCodeBench](https://bigcode-bench.github.io/) | 1,140 tasks, 139 libraries | BigCode / HuggingFace, ICLR 2025 | "Next gen HumanEval." LLMs ~60% vs 97% human. |
| [LiveCodeBench](https://livecodebench.github.io/) | 1,055 problems (growing) | UC Berkeley, MIT, Cornell | Contamination-free by design (timestamped contest problems) |

### Repository-level understanding

| Benchmark | Scale | Created by | What it tests |
|-----------|-------|------------|---------------|
| [CrossCodeEval](https://crosscodeeval.github.io/) | 10K examples, 4 languages | Amazon Science, NeurIPS 2023 | Cross-file completion. Static analysis ensures single-file context is insufficient. |
| [RepoBench](https://github.com/Leolty/repobench) | Python + Java repos | ICLR 2024 | Three sub-tasks: retrieval, completion, pipeline. Tests context retrieval strategy. |

---

## Tier 3: Agent task benchmarks (adjacent)

Not coding-specific but relevant to agent architecture decisions.

| Benchmark | What it measures | Top scores |
|-----------|------------------|------------|
| [WebArena](https://webarena.dev/) (CMU) | Web agent task completion across 4 domains | 14% → ~60% in 2 years |
| [AgentBench](https://arxiv.org/abs/2308.03688) (Tsinghua, ICLR 2024) | Multi-dimensional agent assessment: OS, web, DB, etc. | — |
| [tau-bench](https://github.com/sierra-research/tau-bench) (Sierra AI) | Agent reliability in multi-turn tool+user interaction. pass^k metric. | GPT-4 <50%, ~25% over 8 repeats |

---

## Tier 4: Training environments (not benchmarks, but adjacent)

| Environment | Scale | Key result |
|-------------|-------|------------|
| [SWE-Gym](https://github.com/SWE-Gym/SWE-Gym) (UC Berkeley, ICML 2025) | 2,438 tasks (64,689 raw) | Fine-tuning on <500 trajectories: +14% on SWE-bench Verified |
| [R2E-Gym](https://r2e-gym.github.io/) (COLM 2025) | 8,100+ tasks | R2E-Gym-32B achieves 51% on Verified (SOTA open-weight) |

---

## Tier 5: Meta-benchmarks and evaluation infrastructure

### HAL (Holistic Agent Leaderboard)

- **URL:** https://hal.cs.princeton.edu/ | [Paper](https://arxiv.org/abs/2510.11977)
- **Created by:** Princeton PLI (SAgE Team)
- **What it is:** Standardized, cost-aware, third-party leaderboard + unified harness. Supports SWE-bench Verified, USACO, AppWorld, CORE-bench, tau-bench.
- **Scale:** Validated with 21,730 rollouts, ~$40K total cost.
- **Key innovation:** Decouples scaffold from benchmark. Cost-controlled by default.

### SEAL Leaderboard (Scale AI)

- **URL:** https://labs.scale.com/leaderboard
- **What it does:** Standardized scaffolding with 250-turn limit to isolate raw model capability. Agent systems consistently score 5-15 points higher than same base model on SEAL.

### VeRO

- **Paper:** [arxiv 2602.22480](https://arxiv.org/abs/2602.22480)
- **What it is:** Evaluates "agents that optimize agents." Budget-controlled, versioned agent snapshots.

---

## Other notable benchmarks

| Benchmark | Focus | Note |
|-----------|-------|------|
| [SWT-Bench](https://swtbench.com/) (ETH Zurich, NeurIPS 2024) | Test generation — can agents write failing-then-passing tests? | 276 samples |
| [DPAI Arena](https://dpaia.dev/) (JetBrains, Linux Foundation) | Java/Spring enterprise tasks. Framework-agnostic. | 140+ tasks, 15 Spring projects |
| [OpenCode Bench](https://opencode.ai/zen) | Multi-judge scoring across 5 dimensions. 3 episodes per task. | Harness-published eval |

---

## What harness-specific benchmarks exist?

Surprisingly few harnesses publish their own evals:

| Harness | Published evals? |
|---------|-----------------|
| Aider | Yes — polyglot benchmark + leaderboard. The gold standard. |
| OpenCode | Yes — OpenCode Bench with multi-LLM judging |
| Anthropic/Claude Code | Partial — infrastructure noise study, code review benchmark (52% issue detection) |
| Cursor | No |
| Codex CLI | No |
| Windsurf | No |
| Cline | No |

Most harnesses rely on third-party benchmarks (SWE-bench, Terminal-Bench). This means **harness-specific optimizations (edit format, context management, tool selection) are under-measured.**

---

## Key numbers for harness engineers

These are the results that should inform harness design decisions:

- **22-point swing** from scaffolding alone on SWE-bench Pro (same model, different scaffold)
- **13.7-point gain** on Terminal-Bench from harness-only changes (LangChain)
- **6-point swing** from infrastructure configuration on Terminal-Bench (Anthropic)
- **10x improvement** in edit accuracy from hashline format (Grok: 6.7% → 68.3%)
- **3x reduction** in lazy coding from unified diff format (Aider)
- **6.6-point boost** from context management alone (CCA ablation)
- **+14% absolute** from fine-tuning on 491 agent trajectories (SWE-Gym)
- **74.4% → 11.0%** drop from bug-fixing to feature development (FeatureBench)
- **98% first-pass accuracy** claimed by Morph's semantic edit approach
