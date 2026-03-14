---
name: ipc-context
description: "Load full IPC project context for a new session. Reads plans, progress, architectural decisions, and current state so Claude understands the multi-agent IPC system. Use at the start of any session that will touch IPC code, plans, or reviews. Trigger on: 'загрузи контекст', 'что мы делаем', 'контекст IPC', 'catch me up', 'where are we', 'new session'."
user-invocable: true
---

# IPC Project Context Loader

Load the full project context so this session understands the IPC system, its history, current state, and what's next.

## Step 1: Read core documents

Read these files in parallel:

- `docs/fork/README.md` — doc index and branch model
- `docs/fork/ipc-phase3-plan.md` — current phase plan (Architectural Decisions AD-1 through AD-6, implementation steps)
- `docs/fork/ipc-phase2-progress.md` — what Phase 2 delivered (8 steps, PRs #26-#34)
- `docs/fork/ipc-progress.md` — what Phase 1 delivered (11 steps, PRs #5-#21)
- `docs/fork/delta-registry.md` — fork-owned vs upstream files, shared hotspots

## Step 2: Read current code state

Read these key files (first 50 lines each is enough for orientation):

- `src/gateway/ipc.rs` — IPC broker (handlers, IpcDb, ACL validation, audit events)
- `src/tools/agents_ipc.rs` — IPC tools (agents_spawn, send, inbox, reply, state)
- `src/security/pairing.rs` — token auth, TokenMetadata
- `src/cron/scheduler.rs` — scheduler (currently in-process, Phase 3 adds subprocess)
- `src/config/schema.rs` — AgentsIpcConfig, IpcPromptGuardConfig, SandboxConfig

## Step 3: Check git state

Run:
```bash
git log --oneline -10
git status
git branch --show-current
```

## Step 4: Present summary

Output a concise summary in this format:

```
## IPC Project Context

### Architecture
- Broker-mediated HTTP IPC between agents with trust levels L0-L4
- 5 ACL rules, quarantine lane for L4, promote-to-task workflow
- PromptGuard + LeakDetector + sequence integrity + session limits

### Phase Status
- Phase 1 (brokered coordination): DONE — PRs #5-#21
- Phase 2 (broker-side safety): DONE — PRs #26-#34
- Phase 3 (trusted execution): PLANNED — plan finalized with 6 ADs

### Phase 3A: Usable Isolation (current target)
Steps 0-6:
0. Subprocess execution path in scheduler (prerequisite)
1. Ephemeral identity provisioning
2. agents_spawn upgrade (wait=true, workload profiles)
3. Child process IPC bootstrap (env vars)
4. Result delivery via spawn_runs table
5. Fail-closed execution profiles
6. Integration tests + docs

Key architectural decisions:
- AD-1: Agent-local runtime + broker-issued identity (no broker-side compute)
- AD-2: Fail-closed sandbox (L2-L4 refuse to start without sandbox)
- AD-3: Execution boundary (trust-derived) vs workload profile (can only narrow)
- AD-4: spawn_runs table for wait=true (not inbox polling)
- AD-5: Ephemeral identities runtime-only (not in persistent config)
- AD-6: Agents own signing keys, broker issues delegation certificates

### Current branch: {branch}
### Recent commits: {last 3}
### Uncommitted changes: {yes/no}
```

## Step 5: Ask what to do

After presenting context, ask:

> Контекст загружен. Что делаем?

## Arguments

- No args: full context load
- `brief`: skip code reading, just docs + git state
- `code`: skip docs, focus on current code state + git
