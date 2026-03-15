# IPC Phase 3.6: Agent Provisioning UI

Phase 3.5: human control plane | **Phase 3.6: agent provisioning** | Phase 4: federated execution

---

## What Phase 3.6 gives

Four promises to the operator:

1. **One-click agent provisioning** — create a new agent identity, pick a preset, get a ready-to-use config.toml and pairing code — all from the web UI.
2. **Fleet blueprints** — pre-built multi-agent configurations for real-world scenarios (marketing pipeline, office assistant, dev team, family, research bureau). Deploy an entire coordinated fleet, not just individual agents.
3. **Full provider & channel catalog** — every provider and channel ZeroClaw supports is available in a dropdown. Users enter only their API keys and bot tokens.
4. **Full lifecycle from UI** — add, configure, pair, revoke, delete — without ever touching curl or TOML by hand.

---

## Why Phase 3.6 exists

Phase 3.5 gave the operator **visibility and intervention** — they can see agents, inspect messages, quarantine, revoke. But creating a new agent still requires:

1. `curl POST /admin/paircode/new` with JSON body
2. Manually writing a `config.toml` with provider keys, channel tokens, IPC settings
3. Copying the config to the target machine
4. Running `zeroclaw pair` with the code
5. Starting the daemon

Deploying a coordinated multi-agent fleet means repeating this 4-8 times, plus manually configuring lateral_text_pairs, l4_destinations, and trust relationships.

This is fine for the developer who built the system. It is not fine for a family member, a collaborator, or even the same developer six months later.

With Phase 3.6:
- Fleet Overview has "Add Agent" and "Deploy Blueprint" buttons
- A guided form collects: preset/blueprint, names, provider keys, channel tokens
- The system generates complete config.toml files ready to download (one per agent, or a zip for blueprints)
- Pairing codes are shown with copy-to-clipboard
- Agents appear in Fleet immediately after pairing

---

## Non-goals

- Not a remote deployment system (config must still be placed on target machines manually or via SSH)
- Not a visual TOML editor for all 200+ config fields (the existing `/config` page handles that)
- Not a multi-host orchestration layer (Phase 4)
- Not auto-discovery of agents on the network
- Not a custom blueprint builder (v1 ships static blueprints; user-defined blueprints are Phase 4)

---

## Relationship to existing UI

```
/agent          → Talk to THIS agent (chat interface)
/config         → Edit THIS agent's config (TOML editor)
/ipc/fleet      → See ALL agents + provision new ones (Phase 3.6)
  ├─ Add Agent  → Single agent: preset → credentials → download config + pairing code
  └─ Blueprint  → Multi-agent fleet: scenario → credentials → download all configs + codes
/ipc/fleet/:id  → Inspect ONE agent (detail, messages, spawns)
```

---

## Architectural Decisions

### AD-1: Config generation is client-side, not broker-side

The browser assembles TOML configs from templates + user inputs and offers them as downloads. The broker **never sees or stores** the new agent's provider API keys or channel tokens.

**Why**: the broker stores only identity metadata (agent_id, trust_level, role, public_key). Sending provider credentials to the broker would create a credential aggregation risk — one compromised broker leaks all agents' API keys.

**Consequence**: the "Download config" button generates the file in-browser via Blob URL. No `POST /admin/generate-config` endpoint. For blueprints, a zip is generated client-side.

### AD-2: Two levels of presets — agents and blueprints

- **Agent presets** (5): define a single agent's trust, role, model, tools, prompt. Used in "Add Agent" flow.
- **Fleet blueprints** (5+): define a coordinated multi-agent topology — which agent presets to use, how they connect (lateral pairs, l4 destinations), what the coordinator's prompt says about delegation. Used in "Deploy Blueprint" flow.

Both are static TypeScript data, version-controlled with the UI code.

### AD-3: Provider catalog mirrors the onboarding wizard

The provider selection in the UI uses the same tiers and provider IDs as the CLI wizard (`src/onboard/wizard.rs`). This ensures config compatibility — a config generated from the UI will work identically to one created by the wizard.

### AD-4: Channel catalog covers all supported channels

Every channel with a config struct in `schema.rs` is available in the dropdown, with per-channel credential fields. Channels with compile-time feature gates (Matrix, Nostr) show a note about required build flags.

### AD-5: Pairing is still two-step (create code → exchange code)

The UI generates a pairing code via `POST /admin/paircode/new` with agent metadata. The operator copies the code to the new agent instance. No change from the existing flow.

### AD-6: Delete = revoke + cleanup, not a new endpoint

"Delete agent" means: revoke token → set status=revoked → hide from default view. Audit trail preserved. Reuses existing `/admin/ipc/revoke`.

---

## Provider Catalog

Organized by tier, matching the CLI wizard exactly.

### Tier 1 — Recommended

| ID | Name | Credential | Notes |
|----|------|-----------|-------|
| `openrouter` | OpenRouter | API key | 200+ models, single key |
| `venice` | Venice AI | API key | Privacy-first |
| `anthropic` | Anthropic | API key | Claude Sonnet, Opus |
| `openai` | OpenAI | API key | GPT-4o, o1, GPT-5 |
| `openai-codex` | OpenAI Codex | OAuth (no key) | ChatGPT subscription |
| `deepseek` | DeepSeek | API key | V3 & R1 |
| `mistral` | Mistral | API key | Large, Codestral |
| `xai` | xAI | API key | Grok 3 & 4 |
| `perplexity` | Perplexity | API key | Search-augmented |
| `gemini` | Google Gemini | API key or CLI auth | Flash & Pro |

### Tier 2 — Fast Inference

| ID | Name | Credential |
|----|------|-----------|
| `groq` | Groq | API key |
| `fireworks` | Fireworks AI | API key |
| `novita` | Novita AI | API key |
| `together-ai` | Together AI | API key |
| `nvidia` | NVIDIA NIM | API key |

### Tier 3 — Gateway / Proxy

| ID | Name | Credential |
|----|------|-----------|
| `vercel` | Vercel AI Gateway | API key |
| `cloudflare` | Cloudflare AI Gateway | API key |
| `astrai` | Astrai | API key |
| `bedrock` | Amazon Bedrock | AWS credentials |

### Tier 4 — Specialized (China & niche)

| ID | Name | Credential | Notes |
|----|------|-----------|-------|
| `kimi-code` | Kimi Code | API key | Coding-optimized |
| `qwen-code` | Qwen Code | OAuth | ~/.qwen/oauth_creds.json |
| `moonshot` | Moonshot/Kimi (CN) | API key | China endpoint |
| `moonshot-intl` | Moonshot/Kimi (intl) | API key | International |
| `glm` | GLM/Zhipu (intl) | API key | ChatGLM |
| `glm-cn` | GLM/Zhipu (CN) | API key | China endpoint |
| `minimax` | MiniMax (intl) | API key | |
| `minimax-cn` | MiniMax (CN) | API key | |
| `qwen` | Qwen/DashScope (CN) | API key | |
| `qwen-intl` | Qwen/DashScope (intl) | API key | |
| `qwen-us` | Qwen/DashScope (US) | API key | |
| `qianfan` | Qianfan/Baidu (CN) | API key | |
| `zai` | Z.AI (global) | API key | Coding |
| `zai-cn` | Z.AI (CN) | API key | |
| `synthetic` | Synthetic AI | API key | |
| `opencode` | OpenCode Zen | API key | |
| `opencode-go` | OpenCode Go | API key | Subsidized |
| `cohere` | Cohere | API key | Command R+ |

### Tier 5 — Local / Private

| ID | Name | Credential | Notes |
|----|------|-----------|-------|
| `ollama` | Ollama | None | Llama, Mistral, Phi |
| `llamacpp` | llama.cpp server | None | OpenAI-compatible |
| `sglang` | SGLang | None | High-performance |
| `vllm` | vLLM | None | High-performance |
| `osaurus` | Osaurus | None | Edge runtime |

### Tier 6 — Custom (BYOP)

Any OpenAI-compatible API: base_url + optional API key + model name.

---

## Channel Catalog

### Messaging

| ID | Name | Credentials needed |
|----|------|-------------------|
| `telegram` | Telegram | bot_token, allowed_users |
| `discord` | Discord | bot_token, guild_id, allowed_users |
| `slack` | Slack | bot_token, app_token, channel_id, allowed_users |
| `matrix` | Matrix | homeserver, access_token or password, room_id, allowed_users |
| `mattermost` | Mattermost | url, bot_token, channel_id, allowed_users |
| `signal` | Signal | phone_number, signal_cli_path |
| `whatsapp` | WhatsApp (Cloud) | access_token, phone_number_id, verify_token |
| `whatsapp_web` | WhatsApp Web | session data (feature: whatsapp-web) |
| `imessage` | iMessage | applescript_path (macOS only) |
| `irc` | IRC | server, port, nick, channel, password |

### Work / China

| ID | Name | Credentials needed |
|----|------|-------------------|
| `lark` | Lark/Feishu | app_id, app_secret |
| `dingtalk` | DingTalk | app_key, app_secret, robot_code |
| `wecom` | WeCom (企业微信) | corp_id, agent_id, secret |
| `qq` | QQ | app_id, token |
| `clawdtalk` | ClawdTalk | built-in web chat |

### Infrastructure

| ID | Name | Credentials needed |
|----|------|-------------------|
| `webhook` | Webhook | url, secret |
| `mqtt` | MQTT | broker_url, topic, username, password |
| `nostr` | Nostr | private_key, relays (feature: channel-nostr) |
| `nextcloud_talk` | Nextcloud Talk | url, token, room_token |
| `linq` | LinQ | api_key, workspace_id |

### Notes
- `email` — incoming: IMAP polling, outgoing: SMTP. Credentials: imap_host, smtp_host, username, password
- Channels with feature gates (`matrix`, `nostr`, `whatsapp_web`) show a build flag reminder in the UI

---

## Agent Presets (single agent)

### 1. Coordinator (L1)

**For**: the main orchestrator.

| Field | Default |
|-------|---------|
| trust_level | 1 |
| role | coordinator |
| suggested_model | `claude-opus-4-6` via anthropic, or best available |
| tools | all (no restrictions) |
| system_prompt | "You are the primary coordinator. Delegate tasks to specialists, synthesize results, make decisions." |

### 2. Ops Monitor (L2)

**For**: infrastructure monitoring, incident response.

| Field | Default |
|-------|---------|
| trust_level | 2 |
| role | monitor |
| suggested_model | `claude-sonnet-4-6` or `deepseek-chat` |
| tools | shell, http_request, memory_read, memory_write, IPC tools |
| system_prompt | "You monitor infrastructure health. Run diagnostics, report incidents upstream, escalate destructive actions." |

### 3. Research Worker (L3)

**For**: information gathering, browsing, analysis.

| Field | Default |
|-------|---------|
| trust_level | 3 |
| role | researcher |
| suggested_model | `claude-sonnet-4-6` or `gpt-4o` |
| tools | web_search, web_fetch, memory_read, memory_write, IPC tools |
| system_prompt | "You research topics using web search and browsing. Return structured findings to the coordinator." |

### 4. Code Worker (L3)

**For**: development, code review, testing.

| Field | Default |
|-------|---------|
| trust_level | 3 |
| role | developer |
| suggested_model | `claude-sonnet-4-6` or `deepseek-coder` |
| tools | shell, file_read, file_write, memory_read, memory_write, IPC tools |
| system_prompt | "You write, review, and test code. Work within the project workspace. Report results upstream." |

### 5. Restricted Assistant (L4)

**For**: children, guests, low-trust environments.

| Field | Default |
|-------|---------|
| trust_level | 4 |
| role | restricted |
| suggested_model | `claude-haiku-4-5` or cheapest available |
| tools | memory_read, web_search (filtered) |
| system_prompt | "You are a friendly assistant. Answer questions, help with homework, tell stories. You cannot run commands or access files." |

---

## Fleet Blueprints (multi-agent)

Each blueprint defines: which agent presets to deploy, their names, IPC wiring (lateral pairs, l4 destinations), and the coordinator's delegation prompt.

### Blueprint 1: Marketing Pipeline

**Scenario**: automated content marketing — monitor news, aggregate trends, write copy, post to social.

| Agent | Preset | Name | Channel | Notes |
|-------|--------|------|---------|-------|
| Coordinator | L1 Coordinator | `marketing-lead` | Telegram (operator) | Delegates, reviews drafts |
| News Reader | L3 Research | `news-reader` | — | Reads RSS/web, sends findings |
| Aggregator | L3 Research | `trend-aggregator` | — | Synthesizes from news-reader |
| Copywriter | L3 Code Worker | `copywriter` | — | Writes threads/posts from aggregated data |
| Publisher | L2 Ops Monitor | `publisher` | Webhook (social API) | Posts approved content |

**IPC wiring**:
- `lateral_text_pairs`: `[["news-reader", "trend-aggregator"], ["copywriter", "publisher"]]`
- Flow: coordinator → task to news-reader → result → coordinator → task to aggregator → result → coordinator → task to copywriter → result → coordinator reviews → task to publisher

**Coordinator prompt addition**: "You manage a content pipeline. Delegate news gathering to news-reader, trend analysis to trend-aggregator, copywriting to copywriter, and publishing to publisher. Review all content before publishing."

### Blueprint 2: Office Assistant

**Scenario**: email management, calendar, meeting prep, document drafting.

| Agent | Preset | Name | Channel | Notes |
|-------|--------|------|---------|-------|
| Coordinator | L1 Coordinator | `office-lead` | Telegram/Slack (operator) | Central inbox, decisions |
| Email Watcher | L3 Research | `email-watcher` | Email (IMAP) | Reads inbox, classifies, escalates |
| Calendar Bot | L3 Research | `calendar-bot` | Webhook (calendar API) | Monitors events, sends reminders |
| Doc Writer | L3 Code Worker | `doc-writer` | — | Drafts documents, presentations |
| Responder | L2 Ops Monitor | `auto-responder` | Email (SMTP) | Sends approved replies |

**IPC wiring**:
- `lateral_text_pairs`: `[["email-watcher", "auto-responder"]]`
- Flow: email-watcher detects important mail → text to coordinator → coordinator decides → task to doc-writer or auto-responder

**Coordinator prompt addition**: "You manage an office assistant team. email-watcher reads incoming mail and escalates important items to you. You decide what needs a response, a document, or a meeting reminder. Delegate accordingly."

### Blueprint 3: Dev Team

**Scenario**: code review, testing, deployment monitoring.

| Agent | Preset | Name | Channel | Notes |
|-------|--------|------|---------|-------|
| Coordinator | L1 Coordinator | `dev-lead` | Slack/Discord (team) | Code decisions, PR reviews |
| Code Reviewer | L3 Code Worker | `reviewer` | — | Reviews PRs, suggests changes |
| Test Runner | L3 Code Worker | `test-runner` | — | Runs test suites, reports results |
| Ops Monitor | L2 Ops Monitor | `ops` | Webhook (monitoring) | Watches deployments, alerts |

**IPC wiring**:
- `lateral_text_pairs`: `[["reviewer", "test-runner"], ["ops", "reviewer"]]`
- Flow: webhook triggers ops → text to coordinator → coordinator → task to reviewer → result → task to test-runner → result → coordinator synthesizes

### Blueprint 4: Family

**Scenario**: home multi-agent system with children's restricted access.

| Agent | Preset | Name | Channel | Notes |
|-------|--------|------|---------|-------|
| Coordinator | L1 Coordinator | `opus` | Telegram (parent) | Family brain |
| Daily Digest | L3 Research | `daily` | Telegram (family group) | News, weather, reminders |
| Kids Assistant | L4 Restricted | `kids` | Telegram (kids bot) | Homework help, stories |
| Tutor | L4 Restricted | `tutor` | Telegram (tutor bot) | Educational Q&A |

**IPC wiring**:
- `l4_destinations`: `{"supervisor": "opus", "escalation": "opus"}`
- `lateral_text_pairs`: `[]` (L4↔L4 forbidden by ACL)
- Kids/tutor messages go to quarantine lane, reviewed by coordinator

**Coordinator prompt addition**: "You manage a family assistant network. daily sends morning digests. kids and tutor serve the children — their messages arrive in the quarantine lane. Review quarantine content before acting on it."

### Blueprint 5: Research Bureau

**Scenario**: deep research with multiple specialized investigators.

| Agent | Preset | Name | Channel | Notes |
|-------|--------|------|---------|-------|
| Coordinator | L1 Coordinator | `research-lead` | Telegram (operator) | Assigns topics, synthesizes |
| Web Researcher | L3 Research | `web-researcher` | — | Broad web search |
| Analyst | L3 Research | `analyst` | — | Deep analysis, fact-checking |
| Report Writer | L3 Code Worker | `report-writer` | — | Structures findings into reports |

**IPC wiring**:
- `lateral_text_pairs`: `[["web-researcher", "analyst"]]`
- Flow: coordinator → task to web-researcher → result → coordinator → task to analyst → result → coordinator → task to report-writer → final report

---

## User Stories

1. **"I want to add a single research agent."**
   - Fleet → "Add Agent" → preset "Research Worker" → name "research" → pick provider + enter API key → optionally pick channel → Create → get pairing code + download config

2. **"I want to deploy a marketing pipeline."**
   - Fleet → "Deploy Blueprint" → "Marketing Pipeline" → enter provider API key (shared or per-agent) → enter channel tokens → Create All → download zip with 5 configs + see 5 pairing codes

3. **"My kid needs a chat assistant."**
   - Fleet → "Add Agent" → preset "Restricted Assistant" → name "kids" → pick cheap model (Haiku/DeepSeek) → enter Telegram bot token → Create → L4 agent, quarantine lane active

4. **"I want to set up an office email assistant with Qwen on DashScope."**
   - Fleet → "Deploy Blueprint" → "Office Assistant" → provider: `qwen-intl` → enter DashScope API key → email channel: enter IMAP/SMTP credentials → Create All

5. **"I want to delete an agent."**
   - Fleet → agent row → Actions → "Delete" → confirmation → revoked + hidden

6. **"I want to use a local Ollama model, no API key."**
   - Add Agent → provider: `ollama` → no API key field shown → enter Ollama base URL (default localhost:11434) → pick model name

---

## Screens

### 1. Add Agent Dialog (modal on Fleet page)

**Trigger**: "Add Agent" button in Fleet header.

**Step 1 — Pick Preset**:
- 5 agent preset cards in a grid: icon, name, trust badge, one-line description
- Click to select → highlight
- "Custom" option for manual config

**Step 2 — Identity**:
- Agent ID (text input, lowercase, no spaces)
- Role (pre-filled from preset, editable)
- Trust level (pre-filled, dropdown L0-L4, warning if changed from preset)

**Step 3 — Provider**:
- Tier selector (6 tiers as in Provider Catalog)
- Provider dropdown (filtered by tier)
- Credential fields (dynamic per provider — API key, or OAuth note, or "no key needed" for local)
- Model name (pre-filled from preset, editable)
- Base URL override (optional, shown for custom/local)

**Step 4 — Channel (optional)**:
- Channel dropdown (all from Channel Catalog, grouped by category)
- Per-channel credential fields (dynamic)
- Feature gate warning for Matrix/Nostr/WhatsApp-web
- Gateway port (default auto-assigned)

**Step 5 — Result**:
- Pairing code (large, copyable)
- "Download config.toml" button
- Setup instructions (3 steps: place config → pair → start daemon)
- "Done" → refresh Fleet

### 2. Deploy Blueprint Dialog (modal on Fleet page)

**Trigger**: "Deploy Blueprint" button in Fleet header.

**Step 1 — Pick Blueprint**:
- 5 blueprint cards: icon, name, agent count, one-line description
- Shows agent topology diagram (text-based: A → B → C)

**Step 2 — Provider**:
- "Same provider for all agents" toggle (default on)
- If on: single provider + API key selection
- If off: per-agent provider selection (tab per agent)
- Model overrides per agent (pre-filled from preset)

**Step 3 — Channels**:
- Per-agent channel assignment
- Pre-filled from blueprint (e.g., coordinator = Telegram, email-watcher = Email)
- Credential fields per channel

**Step 4 — Review**:
- Summary table: agent_id, role, trust, provider, model, channel
- Edit button per row (jumps back to relevant step)
- Broker config additions: lateral_text_pairs, l4_destinations (shown read-only)

**Step 5 — Result**:
- All pairing codes in a table (copyable)
- "Download All Configs (zip)" button
- Per-agent "Download config.toml" buttons
- Setup instructions per agent
- "Done" → refresh Fleet

### 3. Fleet page updates

- "Add Agent" button in header
- "Deploy Blueprint" button in header
- Filter toggle: "Show revoked" (default off)
- "Delete" action in agent row menu

---

## Implementation Steps

### Step 0: Provider & channel catalogs

**Files**: `web/src/lib/ipc-providers.ts` (new), `web/src/lib/ipc-channels.ts` (new)

**What**:
- `ProviderTier` type, `PROVIDER_TIERS` array with all 6 tiers
- `ProviderDef`: id, name, tier, credential_type (api_key | oauth | none), env_var, default_model, base_url
- `PROVIDERS: ProviderDef[]` — full catalog (40+ providers)
- `ChannelCategory` type, `CHANNEL_CATEGORIES` array
- `ChannelDef`: id, name, category, fields (dynamic credential fields), feature_gate
- `CHANNELS: ChannelDef[]` — full catalog (20+ channels)

**Verify**: TypeScript compiles, catalog matches wizard.rs

---

### Step 1: Agent presets & fleet blueprints

**Files**: `web/src/lib/ipc-presets.ts` (new)

**What**:
- `AgentPreset`: id, name, description, icon, trust_level, role, suggested_provider_tier, suggested_model, tools, system_prompt
- `AGENT_PRESETS: AgentPreset[]` — 5 presets
- `FleetBlueprint`: id, name, description, agents (array of { preset_id, default_name, default_channel }), lateral_text_pairs, l4_destinations, coordinator_prompt_addition
- `FLEET_BLUEPRINTS: FleetBlueprint[]` — 5 blueprints

**Verify**: TypeScript compiles

---

### Step 2: Config generator

**Files**: `web/src/lib/ipc-config-gen.ts` (new)

**What**:
- `AgentConfigInputs`: agentId, role, trustLevel, provider (id + apiKey + model + baseUrl), channel (id + credentials), brokerUrl, gatewayPort, systemPrompt
- `generateAgentConfig(inputs: AgentConfigInputs): string` — produces valid TOML
- `generateBlueprintConfigs(blueprint, sharedInputs, perAgentInputs): { name: string, config: string }[]` — produces configs for all agents in blueprint
- `downloadAsFile(filename, content)` — Blob download helper
- `downloadAsZip(files: {name, content}[])` — client-side zip (use JSZip or inline deflate)
- Broker config additions (lateral pairs, l4 destinations) generated as a separate snippet

**Verify**: `cd web && npm run build`

---

### Step 3: Add Agent dialog

**Files**: `web/src/components/ipc/AddAgentDialog.tsx` (new)

**What**:
- 5-step modal as described in Screens section
- Dynamic provider fields based on tier/provider selection
- Dynamic channel fields based on channel selection
- Form validation per step
- On final step: `createPaircode()` → show code + download config
- Step indicator, back/next navigation, escape to close

**Verify**: `cd web && npm run build`

---

### Step 4: Deploy Blueprint dialog

**Files**: `web/src/components/ipc/DeployBlueprintDialog.tsx` (new)

**What**:
- 5-step modal as described in Screens section
- Topology visualization (text diagram)
- Shared vs per-agent provider toggle
- Per-agent channel assignment
- Review table with inline edit
- On final step: batch `createPaircode()` for each agent → show all codes + download zip

**Verify**: `cd web && npm run build`

---

### Step 5: Wire into Fleet page + API

**Files**: `web/src/pages/ipc/Fleet.tsx`, `web/src/lib/ipc-api.ts`

**What**:
- `createPaircode()` in API client
- "Add Agent" and "Deploy Blueprint" buttons in Fleet header
- Dialog open/close state for both
- "Delete" action in agent row (= revokeAgent + hide)
- "Show revoked" toggle filter
- Refresh Fleet after provisioning

**Verify**: `cd web && npm run build`, manual browser test

---

### Step 6: Polish + docs

**What**:
- Copy-to-clipboard for all pairing codes
- Trust level tooltips (L0-L4 explanation)
- Provider help links (where to get API key)
- Channel setup hints (how to create a Telegram bot, etc.)
- Feature gate warnings (Matrix requires `--features channel-matrix`)
- Update `ipc-quickstart.md` with UI provisioning flow
- Update `delta-registry.md`

**Verify**: full walkthrough: single agent add + blueprint deploy + delete

---

## File Structure

```
web/src/
├── lib/
│   ├── ipc-providers.ts         # NEW: provider catalog (40+ providers)
│   ├── ipc-channels.ts          # NEW: channel catalog (20+ channels)
│   ├── ipc-presets.ts           # NEW: 5 agent presets + 5 fleet blueprints
│   ├── ipc-config-gen.ts        # NEW: TOML generator + zip download
│   └── ipc-api.ts               # EDIT: add createPaircode()
├── components/
│   └── ipc/
│       ├── AddAgentDialog.tsx    # NEW: single agent provisioning (5 steps)
│       └── DeployBlueprintDialog.tsx # NEW: fleet provisioning (5 steps)
└── pages/
    └── ipc/
        └── Fleet.tsx             # EDIT: add buttons + dialogs + delete + filter
```

---

## Verification

### Final checklist
1. "Add Agent" button visible on Fleet page
2. All 5 agent presets render with correct defaults
3. All 40+ providers available in tier-grouped dropdown
4. Provider credential fields change dynamically (API key / OAuth / none)
5. All 20+ channels available with correct credential fields
6. Feature gate warnings shown for Matrix, Nostr, WhatsApp-web
7. Config.toml downloads with valid TOML structure
8. "Deploy Blueprint" shows 5 blueprints with topology diagrams
9. Blueprint creates N pairing codes and downloads N configs as zip
10. Pairing code copyable with one click
11. Downloaded config works: agent starts, pairs, appears in Fleet
12. "Delete" action revokes and hides agent
13. "Show revoked" toggle works
14. API keys never sent to broker (client-side generation only)

---

## Risk

| Risk | Impact | Mitigation |
|------|--------|------------|
| API key leaked to broker | Credential compromise | AD-1: config generated client-side only |
| Invalid TOML generated | Agent won't start | Template-based generation, test each preset |
| Blueprint topology doesn't match real use case | Misleading defaults | Clear description, editable fields, "Custom" option |
| Provider catalog drifts from wizard.rs | Config incompatibility | AD-3: mirror wizard exactly, review on upstream sync |
| Zip generation fails in browser | No bundle download | Fallback: individual file downloads |
| Scope creep into remote deployment | Delays delivery | Non-goal, documented |

---

## Dependencies

**Required (done)**:
- Phase 3.5: Fleet page, admin endpoints, sidebar
- `POST /admin/paircode/new` with optional body (Phase 1)

**Not required**:
- New Rust backend code (all frontend-only)
- Phase 4 (federated execution)

**Optional enhancement**:
- JSZip or similar for client-side zip generation (blueprint download)

---

## What's NOT in Phase 3.6

- Remote deployment / SSH provisioning
- Config sync between broker and agents
- Custom blueprint builder UI (ship static blueprints first)
- Visual TOML editor for all fields
- Agent auto-discovery on LAN
- Provider credential management on broker side
- Real-time blueprint topology visualization (graph/canvas)
