# Perfhawk Ask — Natural-Language Profiling Insights

**Status:** ⏸ **Deferred** (decided 2026-11-13) — parked until the basic app (PLAN.md Phases 0–3) is ready. Kept for reference; not in current scope.
Draft v0.1 · extends [PLAN.md](PLAN.md) §6 item 6

**Goal:** Users ask questions in plain language ("why is p99 slow on /checkout?", "what changed between these two runs?", "is this a memory leak?") and get **grounded, verifiable answers** whose every claim links to a frame, timeline range or object in the UI — consistent across all languages because the AI only ever sees the unified model.

---

## 1. Design principles

1. **The model never reads raw profiles.** A 200 MB pprof or heap snapshot doesn't fit and shouldn't. The model calls **tools over the Rust analysis engine** (top-N, call paths, diffs, timeline SQL, dominators) and reasons over their compact results. The engine is the source of truth; the model is the interpreter.
2. **Grounded or silent.** Every finding must cite evidence IDs (`node:…`, `range:…`, `obj:…`) returned by tools. The UI renders citations as clickable chips that jump to the flame graph / timeline. Claims with no evidence are dropped by a validator before display.
3. **Language-agnostic by construction.** Tools speak the unified model (§4.2 of PLAN.md) plus the capability registry and fidelity badges, so the same question works for Go, Java, Python, Node… and the model is told when data is `approximate`.
4. **Read-only by default.** Analysis tools are free to call. Anything with side effects — *recording* a new profile, attaching to a process, enabling mutex profiling, uploading/sharing — requires explicit user approval in the UI.
5. **Privacy first.** Users see and control what leaves the machine (symbols, file paths, source snippets). Redaction runs before any tool result is sent. A local-model / offline mode is supported (see §7).

---

## 2. User-facing experiences

| Surface | What it does | Example |
|---|---|---|
| **Ask panel** (chat beside any view) | Multi-turn Q&A over the open session(s) | "Where does the wall-clock time go that isn't CPU?" |
| **Auto-insights** (after every capture) | 3–5 ranked findings cards generated automatically | "61% of wall time is blocked in `os.read` on `/data/idx` → Block I/O view" |
| **Explain this** (right-click a frame / range / object) | Focused explanation with source context and likely fix | "Why is `json.Marshal` hot here?" |
| **Compare narrative** (on `perfhawk diff`) | Plain-language regression/improvement summary | "p99 regression comes from new lock in `cache.Get` (+38 ms/req)" |
| **Command bar → filter** | NL → deterministic query DSL / UI transform, shown before applying | "only worker threads, hide stdlib" → `thread~"worker-*" !lib:stdlib` |
| **Capture copilot** | Turns a goal into a capture plan, waits for approval | "profile why login is slow in pod api-7f9" → plan: cpu+wall+lock 30s, trigger on `/login` |
| **Report writer** | Drafts a shareable Markdown/HTML report from findings | `perfhawk report --ai last` |
| **CLI / agents** | `perfhawk ask "…"` and an MCP server so coding agents use the same tools | `perfhawk mcp` |

**Example exchange**

> **User:** Why did checkout get slower after yesterday's deploy?
> **Perfhawk:** Comparing `run-1142` (before) and `run-1187` (after), wall-clock per request rose 41 ms. Most of it (≈34 ms) is new **lock contention** in `inventory.(*Cache).Get` [node:7f2a] waiting on `sync.RWMutex.Lock` held by `refreshLoop` [node:9c11]. CPU time is essentially unchanged [diff:cpu]. The refresh holds the write lock while calling `db.Query` [src:cache.go:88]. Suggested fix: build the new map outside the lock and swap it atomically. *Confidence: high (native mutex profile, 2 runs).*

---

## 3. Architecture

```
 UI (Ask panel, cards, citations)        CLI `perfhawk ask`        External agents
            │  streaming events                 │                    │ MCP
            ▼                                   ▼                    ▼
 ┌──────────────────────────── perfhawk-ai (orchestrator) ─────────────────────────┐
 │ Router → Agent loop (tool use) → Evidence validator → Insight renderer       │
 │ Context builder · Redactor · Approval gate · Cost/latency budget · Cache     │
 │ Model provider interface:  Claude (default) │ local/OpenAI-compatible (opt.) │
 └───────────────┬──────────────────────────────────────────────────────────────┘
                 │ JSON-RPC (local)
 ┌───────────────▼──────────────── perfhawk core (Rust) ───────────────────────────┐
 │ Analysis tools · Query DSL · Timeline SQL (DuckDB) · Symbolizer · Store      │
 │ Knowledge base: per-runtime rules & idioms (Go/JVM/Python/Node/.NET/…)        │
 └──────────────────────────────────────────────────────────────────────────────┘
```

**Where the orchestrator lives.** Rust has no official Anthropic SDK, so the recommended split is:
- **Rust core** exposes all analysis as tools (JSON-RPC + MCP).
- **`perfhawk-ai`** is a small TypeScript service (bundled with Node/Bun as a sidecar in the desktop app, started by `perfhawk ui`/`perfhawk ask`) using the official `@anthropic-ai/sdk` and its tool runner. The UI already is TypeScript, so types and schemas are shared.
- API keys never live in the webview; the sidecar holds them (OS keychain).

*(Alternative: implement the loop in Rust over raw HTTP. Fewer moving parts, but you hand-maintain request types, streaming and tool-loop logic. Recommend the TS sidecar.)*

---

## 4. Tool surface (what the model can call)

All tools are read-only unless marked ⚠ (requires user approval). Results are compact JSON with **evidence IDs**, capped in size, and redacted.

| Tool | Purpose | Returns (abridged) |
|---|---|---|
| `list_sessions(filter)` | Find captures by target, tag, git SHA, time | ids, kinds, durations, runtime |
| `session_overview(session)` | Capture metadata + kinds + fidelity + runtime counters | totals per kind, GC/threads/event-loop stats |
| `top_functions(profile, metric, n, filters)` | Hot functions by self/total | rows with `node:` ids, % and absolute units |
| `call_paths(profile, node, direction, depth)` | Callers/callees (sandwich) | pruned tree with ids |
| `hot_paths(profile, n)` | Heaviest root→leaf paths | paths with ids |
| `diff_profiles(a, b, metric, normalize)` | A/B regressions & improvements | ranked deltas with ids, significance |
| `timeline_query(session, sql)` | SQL over events (GC, locks, I/O, scheduler) | ≤200 rows + `range:` ids |
| `thread_groups(snapshot)` | Goroutine/thread dedup by stack + state | groups, counts, states |
| `heap_dominators(snapshot, n)` / `heap_growth(s1,s2,s3)` | Retained size, leak suspects | objects/types with `obj:` ids |
| `io_breakdown(session)` | Block I/O by stack, file, device, latency histogram | rows with ids |
| `source_snippet(node, context_lines)` | Source around a frame (if available & allowed) | code + `src:` id |
| `runtime_knowledge(runtime, topic)` | Curated notes: e.g. Go mutex semantics, GIL, JVM safepoints | short doc |
| `capabilities(target)` | What can be captured, at what fidelity | matrix |
| `apply_view(view_spec)` | Drive the UI: open flame graph focused on node X | ack |
| ⚠ `plan_capture(target, goal)` → `start_capture(plan)` | Record a new session | plan / new session id |
| ⚠ `share_session(session)` | Upload to share server | link |

Tool design notes:
- **Strict schemas** (`strict: true`, `additionalProperties: false`) so inputs always validate.
- **Size discipline:** every tool takes `n`/`limit` and the engine truncates with an explicit `truncated: true, total: …` so the model knows it saw a slice.
- **Units always explicit** (`ns`, `bytes`, `count`) — avoids the model confusing alloc bytes with live bytes.
- **Fidelity travels with data:** every result includes `fidelity` so answers can say "approximate (py-spy GIL sampling)".

---

## 5. Prompting, grounding & output

- **System prompt (stable, cached):** role, profiling methodology (CPU vs wall vs off-CPU; alloc vs in-use; contention vs hold time), the "cite or omit" rule, fidelity disclosure rule, and the unified-model glossary. Kept byte-stable so prompt caching hits on every turn.
- **Per-session context (cached after first turn):** `session_overview` output + capability matrix + auto-insight results.
- **Structured findings:** auto-insights and compare narratives use structured outputs:

```json
{
  "findings": [{
    "title": "Lock contention in inventory.Cache.Get",
    "kind": "lock",
    "impact": {"metric": "wall_ns_per_req", "value": 34000000, "share": 0.83},
    "evidence": ["node:7f2a", "node:9c11", "src:cache.go:88"],
    "explanation": "...",
    "suggestion": "...",
    "confidence": "high",
    "fidelity": "native"
  }]
}
```

- **Evidence validator** (deterministic, in `perfhawk-ai`): every `evidence` id must exist in tool results from this conversation, and every number quoted must match a tool value within rounding. Failing findings are dropped or flagged "unverified".
- **Runtime knowledge base:** short curated docs per runtime/kind (e.g. "Go `block` ≠ disk I/O", "JVM safepoint bias", "Node event-loop lag"), retrieved via `runtime_knowledge` rather than stuffed into every prompt.

---

## 6. Model & API choices (Claude)

| Use | Model | Settings |
|---|---|---|
| Ask panel, Explain, Compare, Capture copilot | `claude-opus-5-5` | adaptive thinking, `effort: "medium"` (raise to `high` for multi-session investigations), streaming, tool runner |
| Auto-insights (runs after every capture) | `claude-opus-5-5` | `effort: "low"`, structured output; optional cheaper model (`claude-haiku-5-5`) if cost testing shows quality holds — your call |
| NL → filter DSL | same model, `effort: "low"` | structured output with the DSL grammar; result shown before applying |

API features to use:
- **Streaming** for all interactive answers (progress + tool steps visible in the panel).
- **Prompt caching** on system prompt + tool definitions + session context — large cost/latency win for multi-turn chats.
- **Server-side refusal fallbacks** (`fallbacks: "default"`) enabled by default so a rare classifier refusal doesn't break the panel.
- **Task budget** on long "investigate this incident" runs so the agent paces itself.
- **MCP:** `perfhawk mcp` exposes the same tool set to Claude Code / other agents, so "profile and fix this" can happen inside a coding agent.

---

## 7. Privacy, security & cost controls

- **Data sent preview:** "What will be sent" drawer listing function names, paths, snippets for each turn.
- **Redaction profiles:** strip paths/hostnames/env/argv, hash function names (with local reverse map), never send heap string contents by default.
- **Source access opt-in** per repo; off by default for shared sessions.
- **Provider modes:** `cloud` (Claude), `local` (OpenAI-compatible local server such as Ollama/llama.cpp — reduced features, no guarantees on quality), `off`. Org policy file can force a mode.
- **Prompt-injection hygiene:** symbol names, log strings and source can contain text — tool results are passed as data, side-effect tools are approval-gated, and the model can't share/upload without a UI click.
- **Budgets:** per-question token/cost cap, monthly cap, cost shown per answer.

---

## 8. Quality: evals

Build an eval suite before shipping:
- **Golden profiles** with planted, known bottlenecks per language × kind (hot loop, lock convoy, N+1 I/O, goroutine leak, retained cache, GC thrash, event-loop block, GIL contention).
- **Question set** (~200): diagnostic ("why slow"), lookup ("top allocators"), comparative, and trick questions (unsupported kind, approximate data).
- **Graders:** deterministic (correct `node:` ids cited, numbers match) + rubric-based LLM judge for explanation quality and correct fidelity disclosure.
- Run in CI on prompt/tool changes; track accuracy, citation validity, cost and latency per answer.

---

## 9. Delivery plan (fits PLAN.md roadmap)

| Phase | AI scope |
|---|---|
| **Phase 1** (with MVP capture) | Engine tools + JSON-RPC; `perfhawk ask` CLI; Ask panel with streaming; citations + validator; Claude provider; redaction v1 |
| **Phase 2** | Auto-insights cards, Explain-this, NL → filter, Compare narrative, eval suite in CI |
| **Phase 3** | Capture copilot with approvals, timeline SQL tool, heap tools, runtime knowledge base for all languages, MCP server |
| **Phase 4** | Report writer, local-model mode, org policies, IDE integration (ask from editor gutter) |

## 10. Open decisions

1. Is sending function names / file paths to a cloud model acceptable for your target users, or must local-only be first-class from day one?
2. TS sidecar (recommended) vs pure-Rust orchestrator?
3. Should auto-insights run automatically on every capture (cost) or on demand?
4. Bring-your-own API key vs a hosted proxy you operate (billing, abuse controls)?
