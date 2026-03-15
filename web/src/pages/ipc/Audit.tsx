import { useState, useCallback } from 'react';
import { t } from '@/lib/i18n';
import { fetchAudit, verifyAuditChain } from '@/lib/ipc-api';
import type { IpcAuditEvent, AuditFilter } from '@/types/ipc';
import { TimeAbsolute } from '@/components/ipc/TimeAgo';

const EVENT_TYPES = ['', 'ipc_send', 'ipc_received', 'ipc_blocked', 'ipc_admin_action', 'ipc_rate_limited', 'ipc_state_change'];
const PAGE_SIZE = 50;

export default function Audit() {
  const [events, setEvents] = useState<IpcAuditEvent[]>([]);
  const [loading, setLoading] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [hasMore, setHasMore] = useState(false);
  const [expandedIdx, setExpandedIdx] = useState<number | null>(null);
  const [agentId, setAgentId] = useState('');
  const [eventType, setEventType] = useState('');
  const [search, setSearch] = useState('');
  const [verifyResult, setVerifyResult] = useState<{ ok: boolean; verified?: number; error?: string } | null>(null);
  const [verifying, setVerifying] = useState(false);

  const doSearch = useCallback(async (offset = 0) => {
    setLoading(true);
    try {
      const filters: AuditFilter = { limit: PAGE_SIZE, offset };
      if (agentId) filters.agent_id = agentId;
      if (eventType) filters.event_type = eventType;
      if (search) filters.search = search;
      const data = await fetchAudit(filters);
      if (offset === 0) setEvents(data);
      else setEvents((prev) => [...prev, ...data]);
      setHasMore(data.length === PAGE_SIZE);
      setLoaded(true);
    } catch {
      // handled by empty state
    } finally {
      setLoading(false);
    }
  }, [agentId, eventType, search]);

  const handleVerify = async () => {
    setVerifying(true);
    try {
      const result = await verifyAuditChain();
      setVerifyResult(result);
    } catch (e) {
      setVerifyResult({ ok: false, error: e instanceof Error ? e.message : 'Verification failed' });
    } finally {
      setVerifying(false);
    }
  };

  const handleExport = () => {
    const blob = new Blob([JSON.stringify(events, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `audit-export-${Date.now()}.json`;
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <div className="space-y-6 animate-fade-in">
      <h1 className="text-2xl font-bold text-gradient-blue">{t('ipc.audit_title')}</h1>

      {/* Filters */}
      <div className="glass-card p-4 flex flex-wrap gap-3 items-end">
        <div className="space-y-1">
          <label className="text-xs text-[#556080] uppercase tracking-wider">Agent</label>
          <input type="text" value={agentId} onChange={(e) => setAgentId(e.target.value)} placeholder="agent_id" className="input-electric px-3 py-2 text-sm w-36" />
        </div>
        <div className="space-y-1">
          <label className="text-xs text-[#556080] uppercase tracking-wider">Type</label>
          <select value={eventType} onChange={(e) => setEventType(e.target.value)} className="input-electric px-3 py-2 text-sm">
            {EVENT_TYPES.map((et) => <option key={et} value={et}>{et || 'all'}</option>)}
          </select>
        </div>
        <div className="space-y-1">
          <label className="text-xs text-[#556080] uppercase tracking-wider">Search</label>
          <input type="text" value={search} onChange={(e) => setSearch(e.target.value)} placeholder="keyword" className="input-electric px-3 py-2 text-sm w-40" />
        </div>
        <button onClick={() => doSearch(0)} disabled={loading} className="btn-electric px-4 py-2 text-sm font-medium">
          {loading ? 'Loading...' : 'Search'}
        </button>
        <button onClick={handleVerify} disabled={verifying} className="px-4 py-2 text-sm font-medium text-emerald-400 rounded-lg border border-[#1a1a3e]/50 hover:bg-emerald-500/10 transition-colors">
          {verifying ? 'Verifying...' : 'Verify Chain'}
        </button>
        {loaded && events.length > 0 && (
          <button onClick={handleExport} className="px-4 py-2 text-sm font-medium text-[#8892a8] rounded-lg border border-[#1a1a3e]/50 hover:bg-[#1a1a3e]/30 transition-colors">
            Export JSON
          </button>
        )}
      </div>

      {/* Verify result */}
      {verifyResult && (
        <div className={`glass-card p-3 text-sm animate-fade-in ${verifyResult.ok ? 'text-emerald-400 border-emerald-500/30' : 'text-red-400 border-red-500/30'}`}>
          {verifyResult.ok
            ? `Chain verified: ${verifyResult.verified} events OK`
            : `Chain broken: ${verifyResult.error}`}
        </div>
      )}

      {/* Results */}
      {!loaded ? (
        <div className="glass-card p-12 text-center text-[#556080]">Apply filters and click Search.</div>
      ) : events.length === 0 ? (
        <div className="glass-card p-12 text-center text-[#556080]">No audit events found.</div>
      ) : (
        <div className="glass-card overflow-hidden">
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-[#1a1a3e]/50 text-[#556080] text-xs uppercase tracking-wider">
                  <th className="text-left px-4 py-3">Time</th>
                  <th className="text-left px-4 py-3">Type</th>
                  <th className="text-left px-4 py-3">Actor</th>
                  <th className="text-left px-4 py-3">Detail</th>
                  <th className="text-center px-4 py-3">HMAC</th>
                </tr>
              </thead>
              <tbody>
                {events.map((evt, idx) => (
                  <AuditRow
                    key={evt.event_id ?? idx}
                    event={evt}
                    expanded={expandedIdx === idx}
                    onToggle={() => setExpandedIdx(expandedIdx === idx ? null : idx)}
                  />
                ))}
              </tbody>
            </table>
          </div>
          {hasMore && (
            <div className="px-4 py-3 border-t border-[#1a1a3e]/30 text-center">
              <button onClick={() => doSearch(events.length)} disabled={loading} className="text-sm text-[#0080ff] hover:underline">
                {loading ? 'Loading...' : 'Load more'}
              </button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function EventTypeBadge({ type }: { type: string }) {
  const colors: Record<string, string> = {
    ipc_send: 'bg-blue-500/20 text-blue-400',
    ipc_received: 'bg-emerald-500/20 text-emerald-400',
    ipc_blocked: 'bg-red-500/20 text-red-400',
    ipc_admin_action: 'bg-purple-500/20 text-purple-400',
    ipc_rate_limited: 'bg-yellow-500/20 text-yellow-400',
    ipc_state_change: 'bg-gray-500/20 text-gray-400',
  };
  const cls = colors[type] ?? 'bg-gray-500/20 text-gray-400';
  return <span className={`inline-flex px-1.5 py-0.5 rounded text-xs font-medium ${cls}`}>{type}</span>;
}

function AuditRow({ event, expanded, onToggle }: { event: IpcAuditEvent; expanded: boolean; onToggle: () => void }) {
  const ts = event.timestamp ? Math.floor(new Date(event.timestamp).getTime() / 1000) : 0;
  const actor = event.actor?.user_id ?? event.actor?.channel ?? '-';
  const detail = event.action?.command ?? '-';

  return (
    <>
      <tr onClick={onToggle} className="border-b border-[#1a1a3e]/30 hover:bg-[#0080ff05] cursor-pointer transition-colors">
        <td className="px-4 py-2">{ts > 0 ? <TimeAbsolute timestamp={ts} /> : <span className="text-[#556080]">-</span>}</td>
        <td className="px-4 py-2"><EventTypeBadge type={event.event_type} /></td>
        <td className="px-4 py-2 font-mono text-xs text-[#8892a8]">{actor}</td>
        <td className="px-4 py-2 text-[#8892a8] max-w-md truncate">{detail}</td>
        <td className="px-4 py-2 text-center">
          {event.hmac ? (
            <span className="text-emerald-400 text-xs" title={event.hmac}>&#x1f517;</span>
          ) : (
            <span className="text-[#334060] text-xs">-</span>
          )}
        </td>
      </tr>
      {expanded && (
        <tr>
          <td colSpan={5} className="p-4 bg-[#050510]">
            <pre className="text-xs text-[#8892a8] whitespace-pre-wrap break-all max-h-64 overflow-auto">
              {JSON.stringify(event, null, 2)}
            </pre>
          </td>
        </tr>
      )}
    </>
  );
}
