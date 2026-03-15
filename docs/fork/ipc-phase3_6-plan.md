# IPC Phase 3.6: Agent Provisioning UI

Phase 3.5: human control plane | **Phase 3.6: agent provisioning** | Phase 4: federated execution

---

## What Phase 3.6 gives

Three promises to the operator:

1. **Single-agent provisioning from UI** — create identity, pick a preset, enter credentials, get a ready-to-use config.toml and pairing code. Full flow without curl or TOML editing.
2. **Fleet blueprints** — pre-built multi-agent topologies for real-world scenarios. Generate all agent configs + pairing codes in one flow. **Honest scope**: blueprint generates configs and codes, but broker-side IPC wiring (lateral pairs, l4 destinations) requires a manual config patch on the broker — this is documented as a post-deploy step, not hidden.
3. **Full lifecycle from UI** — add, pair, revoke, delete single agents entirely from the web UI.

---

## Why Phase 3.6 exists

Phase 3.5 gave the operator **visibility and intervention**. But creating a new agent still requires:

1. `curl POST /admin/paircode/new` with JSON body
2. Manually writing a `config.toml` with provider keys, channel tokens, IPC settings
3. Copying the config to the target machine
4. Running `zeroclaw pair` with the code
5. Starting the daemon

With Phase 3.6:
- Fleet Overview has an "Add Agent" button → guided form → download config + pairing code
- "Deploy Blueprint" generates N configs + codes for a coordinated fleet
- "Delete" revokes and hides agents from Fleet

---

## Non-goals

- Not a remote deployment system (configs placed manually or via SSH)
- Not a visual TOML editor for all 200+ fields
- Not auto-discovery of agents on the network
- Not a custom blueprint builder (static blueprints in v1; user-defined = Phase 4)
- Not broker-side IPC wiring from UI (lateral_text_pairs, l4_destinations remain in broker config.toml)

---

## Architectural Decisions

### AD-1: Config generation is client-side, not broker-side

The browser assembles TOML configs from templates + user inputs and offers them as downloads. The broker **never sees or stores** agent provider API keys or channel tokens.

**Why**: credential aggregation risk — one compromised broker would leak all agents' API keys.

### AD-2: Two levels of presets — agents and blueprints

- **Agent presets** (5): single agent's trust, role, model, tools, prompt.
- **Fleet blueprints** (5): coordinated multi-agent topology with named agents, preset assignments, and IPC wiring specification.

Both are static TypeScript data, version-controlled with the UI.

### AD-3: Blueprint generates configs + codes, not broker wiring

A blueprint produces: N agent config.toml files + N pairing codes + a **broker config patch snippet** (lateral_text_pairs, l4_destinations) that the operator pastes into the broker's config.toml.

**Why**: the broker's `agents_ipc` section is not exposed via an API endpoint for live patching. Adding a config-mutation API is scope for Phase 4. For v1, the snippet + manual paste is honest and safe.

### AD-4: Curated provider subset for v1; full catalog for v2

v1 ships with **Tier 1 (Recommended) + Tier 5 (Local) + Custom** — about 15 providers. The remaining 25+ specialized/China/gateway providers are deferred to v2 to avoid a static catalog that drifts from `wizard.rs` on every upstream sync.

**Why**: 40+ providers in a static TS module is a maintenance burden. Tier 1 + Local + Custom covers 90%+ of real use cases. Specialized providers can be reached via Custom (BYOP) with a base URL.

### AD-5: Channel catalog = agent-facing transports only

The channel picker in the UI includes only real bidirectional agent channels that an operator would assign to an agent instance. Excluded from v1:

- **webhook** — an inbound listener endpoint, not a user-facing chat transport. Excluded from channel picker.
- **mqtt** — a SOP listener path, not a ChannelsConfig transport. Excluded.
- **clawdtalk** — the built-in web chat, auto-configured. No credentials needed.
- **whatsapp_web** — not a separate channel; it's a mode inside `WhatsAppConfig` (set `session_path` instead of `access_token`). Shown as a toggle within the WhatsApp channel form, not a separate entry.

### AD-6: Delete = revoke + hide

"Delete agent" = revoke token → set status=revoked → hide from default Fleet view. Audit trail preserved. Reuses existing `/admin/ipc/revoke`.

---

## Provider Catalog (v1 scope)

### Tier 1 — Recommended

| ID | Name | Credential | Default model |
|----|------|-----------|--------------|
| `anthropic` | Anthropic | API key | claude-sonnet-4-6 |
| `openai` | OpenAI | API key | gpt-4o |
| `openrouter` | OpenRouter | API key | (user picks) |
| `deepseek` | DeepSeek | API key | deepseek-chat |
| `mistral` | Mistral | API key | mistral-large-latest |
| `xai` | xAI | API key | grok-3 |
| `gemini` | Google Gemini | API key | gemini-2.0-flash |
| `groq` | Groq | API key | llama-3.3-70b |
| `perplexity` | Perplexity | API key | sonar |
| `venice` | Venice AI | API key | (user picks) |

### Tier 5 — Local / Private

| ID | Name | Credential | Notes |
|----|------|-----------|-------|
| `ollama` | Ollama | None | base_url default: http://localhost:11434 |
| `llamacpp` | llama.cpp server | None | base_url required |
| `vllm` | vLLM | None | base_url required |

### Custom (BYOP)

Any OpenAI-compatible API: base_url + optional API key + model name.

### Deferred to v2

Tiers 2 (Fast inference), 3 (Gateway/proxy), 4 (Specialized/China) — fireworks, together-ai, nvidia, vercel, cloudflare, bedrock, kimi, qwen, glm, minimax, qianfan, zai, cohere, etc. Reachable via Custom in v1.

---

## Channel Catalog (v1 scope)

Agent-facing bidirectional transports only. Fields verified against `schema.rs`.

### Messaging

| ID | Name | Required fields | Optional fields |
|----|------|----------------|-----------------|
| `telegram` | Telegram | `bot_token`, `allowed_users` | `stream_mode`, `mention_only`, `interrupt_on_new_message` |
| `discord` | Discord | `bot_token`, `allowed_users` | `guild_id`, `listen_to_bots`, `mention_only` |
| `slack` | Slack | `bot_token`, `allowed_users` | `app_token`, `channel_id`, `interrupt_on_new_message` |
| `mattermost` | Mattermost | `url`, `bot_token`, `allowed_users` | `channel_id`, `thread_replies`, `mention_only` |
| `matrix` | Matrix | `homeserver`, `room_id`, `allowed_users` | `access_token` or `password`+`user_id`, `device_id` |
| `signal` | Signal | `http_url`, `account` | `group_id`, `allowed_from`, `ignore_attachments` |
| `whatsapp` | WhatsApp (Cloud) | `access_token`, `phone_number_id`, `allowed_numbers` | `verify_token`, `app_secret` |
| `whatsapp` (web mode) | WhatsApp (Web) | `session_path`, `allowed_numbers` | `pair_phone`, `pair_code` |
| `imessage` | iMessage | `allowed_contacts` | — (macOS only) |
| `irc` | IRC | `server`, `nick`, `channel` | `port`, `password`, `use_tls` |

### Work / Enterprise

| ID | Name | Required fields | Optional fields |
|----|------|----------------|-----------------|
| `lark` | Lark / Feishu | `app_id`, `app_secret` | `verification_token`, `encrypt_key` |
| `dingtalk` | DingTalk | `app_key`, `app_secret`, `robot_code` | `allowed_users` |
| `wecom` | WeCom | `webhook_key` | `allowed_users` |
| `qq` | QQ Official | `app_id`, `app_secret` | `allowed_users` |
| `nextcloud_talk` | Nextcloud Talk | `url`, `token`, `room_token` | `allowed_users` |

### Feature-gated

| ID | Build flag | Notes |
|----|-----------|-------|
| `matrix` | `--features channel-matrix` | UI shows warning if not built with flag |
| `nostr` | `--features channel-nostr` | Deferred to v2 (niche) |
| `whatsapp` (web) | `--features whatsapp-web` | Toggle within WhatsApp form |

### Excluded from v1 channel picker

| What | Why |
|------|-----|
| `webhook` | Inbound listener, not a user-facing chat transport |
| `mqtt` | SOP listener, not a ChannelsConfig transport |
| `clawdtalk` | Built-in web chat, auto-configured, no credentials |
| `linq` | Niche SMS API |
| `nostr` | Niche, feature-gated, complex key management |

---

## Agent Presets (single agent)

### 1. Coordinator (L1)

| Field | Default |
|-------|---------|
| trust_level | 1 |
| role | coordinator |
| suggested_provider | anthropic |
| suggested_model | claude-opus-4-6 |
| tools | all |
| system_prompt | "You are the primary coordinator. Delegate tasks, synthesize results, make decisions." |

### 2. Ops Monitor (L2)

| Field | Default |
|-------|---------|
| trust_level | 2 |
| role | monitor |
| suggested_provider | anthropic or deepseek |
| suggested_model | claude-sonnet-4-6 |
| tools | shell, http_request, memory_*, IPC tools |
| system_prompt | "You monitor infrastructure. Run diagnostics, report incidents upstream, escalate destructive actions." |

### 3. Research Worker (L3)

| Field | Default |
|-------|---------|
| trust_level | 3 |
| role | researcher |
| suggested_provider | anthropic or openai |
| suggested_model | claude-sonnet-4-6 |
| tools | web_search, web_fetch, memory_*, IPC tools |
| system_prompt | "You research topics using web search. Return structured findings to the coordinator." |

### 4. Code Worker (L3)

| Field | Default |
|-------|---------|
| trust_level | 3 |
| role | developer |
| suggested_provider | anthropic or deepseek |
| suggested_model | claude-sonnet-4-6 |
| tools | shell, file_read, file_write, memory_*, IPC tools |
| system_prompt | "You write, review, and test code. Work within the workspace. Report results upstream." |

### 5. Restricted Assistant (L4)

| Field | Default |
|-------|---------|
| trust_level | 4 |
| role | restricted |
| suggested_provider | anthropic (haiku) or ollama |
| suggested_model | claude-haiku-4-5 |
| tools | memory_read, web_search |
| system_prompt | "You are a friendly assistant. Answer questions, help with homework, tell stories. No commands, no files." |

---

## Fleet Blueprints (multi-agent)

Each blueprint defines: agent presets, names, suggested channels, IPC wiring.

**Important**: the IPC wiring (lateral_text_pairs, l4_destinations) is generated as a **broker config patch snippet** that the operator manually adds to the broker's config.toml. This is a documented post-deploy step, not automated.

### Blueprint 1: Marketing Pipeline

| Agent | Preset | Default name | Suggested channel |
|-------|--------|-------------|-------------------|
| Coordinator | L1 | `marketing-lead` | Telegram |
| News Reader | L3 Research | `news-reader` | — |
| Trend Aggregator | L3 Research | `trend-aggregator` | — |
| Copywriter | L3 Code Worker | `copywriter` | — |
| Publisher | L2 Ops Monitor | `publisher` | — |

**Broker patch**:
```toml
[agents_ipc]
lateral_text_pairs = [["news-reader", "trend-aggregator"]]
```

**Flow**: coordinator delegates: news → aggregation → copywriting → review → publish.

### Blueprint 2: Office Assistant

| Agent | Preset | Default name | Suggested channel |
|-------|--------|-------------|-------------------|
| Coordinator | L1 | `office-lead` | Telegram or Slack |
| Email Watcher | L3 Research | `email-watcher` | — |
| Calendar Bot | L3 Research | `calendar-bot` | — |
| Doc Writer | L3 Code Worker | `doc-writer` | — |

**Broker patch**:
```toml
[agents_ipc]
lateral_text_pairs = []
```

**Flow**: email-watcher/calendar-bot escalate to coordinator → coordinator delegates to doc-writer.

### Blueprint 3: Dev Team

| Agent | Preset | Default name | Suggested channel |
|-------|--------|-------------|-------------------|
| Coordinator | L1 | `dev-lead` | Slack or Discord |
| Code Reviewer | L3 Code Worker | `reviewer` | — |
| Test Runner | L3 Code Worker | `test-runner` | — |
| Ops Monitor | L2 Ops Monitor | `ops` | — |

**Broker patch**:
```toml
[agents_ipc]
lateral_text_pairs = [["reviewer", "test-runner"], ["ops", "reviewer"]]
```

### Blueprint 4: Family

| Agent | Preset | Default name | Suggested channel |
|-------|--------|-------------|-------------------|
| Coordinator | L1 | `opus` | Telegram (parent) |
| Daily Digest | L3 Research | `daily` | Telegram (family group) |
| Kids Assistant | L4 Restricted | `kids` | Telegram (kids bot) |
| Tutor | L4 Restricted | `tutor` | Telegram (tutor bot) |

**Broker patch**:
```toml
[agents_ipc]
lateral_text_pairs = []

[agents_ipc.l4_destinations]
supervisor = "opus"
escalation = "opus"
```

### Blueprint 5: Research Bureau

| Agent | Preset | Default name | Suggested channel |
|-------|--------|-------------|-------------------|
| Coordinator | L1 | `research-lead` | Telegram |
| Web Researcher | L3 Research | `web-researcher` | — |
| Analyst | L3 Research | `analyst` | — |
| Report Writer | L3 Code Worker | `report-writer` | — |

**Broker patch**:
```toml
[agents_ipc]
lateral_text_pairs = [["web-researcher", "analyst"]]
```

---

## Screens

### 1. Add Agent Dialog (modal on Fleet page)

**Step 1 — Preset**: 5 cards + "Custom" option.

**Step 2 — Identity**: agent_id, role, trust level (pre-filled, editable with warning).

**Step 3 — Provider**: tier selector (Recommended / Local / Custom) → provider dropdown → credential fields (API key or "no key" for local) → model name → base URL override.

**Step 4 — Channel** (optional): channel dropdown (messaging / work / none) → per-channel credential fields (verified against schema.rs) → WhatsApp shows Cloud/Web mode toggle → feature gate warnings for Matrix.

**Step 5 — Result**: pairing code (large, copyable) + "Download config.toml" + setup instructions.

### 2. Deploy Blueprint Dialog (modal on Fleet page)

**Step 1 — Blueprint**: 5 cards with topology description.

**Step 2 — Provider**: "Same for all" toggle → provider + key → per-agent model override.

**Step 3 — Channels**: per-agent channel assignment from the same channel catalog.

**Step 4 — Review**: summary table + broker config patch snippet (copyable).

**Step 5 — Result**: N pairing codes + "Download All (zip)" + broker patch snippet + per-agent setup instructions.

### 3. Fleet page updates

- "Add Agent" and "Deploy Blueprint" buttons in header
- "Delete" action in agent row menu
- "Show revoked" toggle (default off)

---

## Implementation Steps

### Step 0: Provider & channel catalogs

**Files**: `web/src/lib/ipc-providers.ts`, `web/src/lib/ipc-channels.ts`

- Curated provider list (v1 scope: ~15 providers)
- Channel definitions with correct field schemas from `schema.rs`
- WhatsApp: single entry with cloud/web mode toggle
- Feature gate metadata for Matrix

### Step 1: Agent presets & fleet blueprints

**Files**: `web/src/lib/ipc-presets.ts`

- 5 agent presets + 5 fleet blueprints
- Blueprint includes broker_patch_toml string

### Step 2: Config generator

**Files**: `web/src/lib/ipc-config-gen.ts`

- TOML generation from preset + user inputs
- Blueprint: generates N configs + broker patch snippet
- Blob download + zip (via JSZip or inline)

### Step 3: Add Agent dialog

**Files**: `web/src/components/ipc/AddAgentDialog.tsx`

- 5-step modal, validated form, pairing code generation

### Step 4: Deploy Blueprint dialog

**Files**: `web/src/components/ipc/DeployBlueprintDialog.tsx`

- 5-step modal, batch pairing, zip download, broker patch display

### Step 5: Wire into Fleet page

**Files**: `web/src/pages/ipc/Fleet.tsx`, `web/src/lib/ipc-api.ts`

- Buttons, dialogs, delete action, revoked filter

### Step 6: Polish + docs

- Copy-to-clipboard, tooltips, help links, quickstart update

---

## Risk

| Risk | Impact | Mitigation |
|------|--------|------------|
| API key leaked to broker | Credential compromise | AD-1: client-side generation only |
| Invalid TOML generated | Agent won't start | Fields verified against schema.rs |
| Channel fields drift from schema | Broken configs | AD-5: curated v1 subset, review on sync |
| Provider catalog drifts from wizard | Incompatible configs | AD-4: curated subset, Custom fallback |
| Blueprint broker patch forgotten | IPC wiring broken | Explicit "copy this to broker config" step with warning |
| Scope creep | Delays delivery | Non-goals documented, v2 boundary clear |

---

## v1 vs v2 boundary

| Feature | v1 (this phase) | v2 (future) |
|---------|----------------|-------------|
| Providers | Tier 1 + Local + Custom (~15) | All 40+ tiers |
| Channels | 15 agent-facing transports | + webhook, mqtt, nostr, linq |
| Blueprint wiring | Broker patch snippet (manual) | Live broker config API |
| Custom blueprints | — | User-defined blueprint builder |
| Provider sync | Manual review on upstream sync | Auto-generation from wizard.rs |

---

## Dependencies

**Required (done)**:
- Phase 3.5: Fleet page, admin endpoints, sidebar
- `POST /admin/paircode/new` with optional body (Phase 1)

**Not required**:
- New Rust backend code (all frontend-only)
- Phase 4 (federated execution)

**Optional**:
- JSZip for client-side zip (blueprint download)
