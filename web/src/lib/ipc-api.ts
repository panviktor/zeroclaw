// IPC admin API client for Phase 3.5 operator UI

import { apiFetch } from './api';
import type {
  IpcAgent,
  IpcAgentDetail,
  IpcMessage,
  IpcSpawnRun,
  IpcAuditEvent,
  MessagesFilter,
  SpawnRunsFilter,
  AuditFilter,
} from '../types/ipc';

// ---------------------------------------------------------------------------
// Read endpoints (admin, localhost-only)
// ---------------------------------------------------------------------------

export async function fetchFleet(): Promise<IpcAgent[]> {
  const data = await apiFetch<{ agents: IpcAgent[] }>('/admin/ipc/agents');
  return data.agents;
}

export async function fetchAgentDetail(agentId: string): Promise<IpcAgentDetail> {
  return apiFetch<IpcAgentDetail>(
    `/admin/ipc/agents/${encodeURIComponent(agentId)}/detail`,
  );
}

export async function fetchMessages(filters: MessagesFilter = {}): Promise<IpcMessage[]> {
  const params = new URLSearchParams();
  if (filters.agent_id) params.set('agent_id', filters.agent_id);
  if (filters.session_id) params.set('session_id', filters.session_id);
  if (filters.kind) params.set('kind', filters.kind);
  if (filters.quarantine !== undefined) params.set('quarantine', String(filters.quarantine));
  if (filters.dismissed !== undefined) params.set('dismissed', String(filters.dismissed));
  if (filters.lane) params.set('lane', filters.lane);
  if (filters.from_ts !== undefined) params.set('from_ts', String(filters.from_ts));
  if (filters.to_ts !== undefined) params.set('to_ts', String(filters.to_ts));
  if (filters.limit !== undefined) params.set('limit', String(filters.limit));
  if (filters.offset !== undefined) params.set('offset', String(filters.offset));
  const qs = params.toString();
  const data = await apiFetch<{ messages: IpcMessage[] }>(
    `/admin/ipc/messages${qs ? `?${qs}` : ''}`,
  );
  return data.messages;
}

export async function fetchSpawnRuns(filters: SpawnRunsFilter = {}): Promise<IpcSpawnRun[]> {
  const params = new URLSearchParams();
  if (filters.status) params.set('status', filters.status);
  if (filters.parent_id) params.set('parent_id', filters.parent_id);
  if (filters.from_ts !== undefined) params.set('from_ts', String(filters.from_ts));
  if (filters.to_ts !== undefined) params.set('to_ts', String(filters.to_ts));
  if (filters.limit !== undefined) params.set('limit', String(filters.limit));
  if (filters.offset !== undefined) params.set('offset', String(filters.offset));
  const qs = params.toString();
  const data = await apiFetch<{ spawn_runs: IpcSpawnRun[] }>(
    `/admin/ipc/spawn-runs${qs ? `?${qs}` : ''}`,
  );
  return data.spawn_runs;
}

export async function fetchAudit(filters: AuditFilter = {}): Promise<IpcAuditEvent[]> {
  const params = new URLSearchParams();
  if (filters.agent_id) params.set('agent_id', filters.agent_id);
  if (filters.event_type) params.set('event_type', filters.event_type);
  if (filters.from_ts !== undefined) params.set('from_ts', String(filters.from_ts));
  if (filters.to_ts !== undefined) params.set('to_ts', String(filters.to_ts));
  if (filters.search) params.set('search', filters.search);
  if (filters.limit !== undefined) params.set('limit', String(filters.limit));
  if (filters.offset !== undefined) params.set('offset', String(filters.offset));
  const qs = params.toString();
  const data = await apiFetch<{ events: IpcAuditEvent[] }>(
    `/admin/ipc/audit${qs ? `?${qs}` : ''}`,
  );
  return data.events;
}

// ---------------------------------------------------------------------------
// Write endpoints (admin actions)
// ---------------------------------------------------------------------------

export function revokeAgent(agentId: string): Promise<{ ok: boolean; found: boolean; tokens_revoked: number }> {
  return apiFetch('/admin/ipc/revoke', {
    method: 'POST',
    body: JSON.stringify({ agent_id: agentId }),
  });
}

export function quarantineAgent(agentId: string): Promise<{ ok: boolean; found: boolean; messages_quarantined: number }> {
  return apiFetch('/admin/ipc/quarantine', {
    method: 'POST',
    body: JSON.stringify({ agent_id: agentId }),
  });
}

export function disableAgent(agentId: string): Promise<{ ok: boolean; found: boolean }> {
  return apiFetch('/admin/ipc/disable', {
    method: 'POST',
    body: JSON.stringify({ agent_id: agentId }),
  });
}

export function downgradeAgent(agentId: string, newLevel: number): Promise<{ ok: boolean; old_level: number; new_level: number }> {
  return apiFetch('/admin/ipc/downgrade', {
    method: 'POST',
    body: JSON.stringify({ agent_id: agentId, new_level: newLevel }),
  });
}

export function promoteMessage(messageId: number, toAgent: string): Promise<{ promoted: boolean; new_message_id: number }> {
  return apiFetch('/admin/ipc/promote', {
    method: 'POST',
    body: JSON.stringify({ message_id: messageId, to_agent: toAgent }),
  });
}

export function dismissMessage(messageId: number): Promise<{ ok: boolean; dismissed: boolean }> {
  return apiFetch('/admin/ipc/dismiss-message', {
    method: 'POST',
    body: JSON.stringify({ message_id: messageId }),
  });
}

export function verifyAuditChain(): Promise<{ ok: boolean; verified?: number; error?: string }> {
  return apiFetch('/admin/ipc/audit/verify', { method: 'POST' });
}

// ---------------------------------------------------------------------------
// Availability check
// ---------------------------------------------------------------------------

/** Check if IPC admin endpoints are accessible (localhost-only). */
export async function checkIpcAccess(): Promise<boolean> {
  try {
    await apiFetch('/admin/ipc/agents');
    return true;
  } catch {
    return false;
  }
}
