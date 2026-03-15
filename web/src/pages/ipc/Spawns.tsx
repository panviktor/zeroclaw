import { useState, useCallback } from 'react';
import { Link } from 'react-router-dom';
import { t } from '@/lib/i18n';
import { fetchSpawnRuns, revokeAgent } from '@/lib/ipc-api';
import type { IpcSpawnRun, SpawnRunsFilter } from '@/types/ipc';
import StatusBadge from '@/components/ipc/StatusBadge';
import AgentLink from '@/components/ipc/AgentLink';
import { TimeAbsolute, TimeUntil } from '@/components/ipc/TimeAgo';
import ConfirmDialog from '@/components/ipc/ConfirmDialog';

const STATUSES = ['', 'running', 'completed', 'timeout', 'revoked', 'interrupted'];
const PAGE_SIZE = 50;

export default function Spawns() {
  const [runs, setRuns] = useState<IpcSpawnRun[]>([]);
  const [loading, setLoading] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [hasMore, setHasMore] = useState(false);
  const [status, setStatus] = useState('');
  const [parentId, setParentId] = useState('');
  const [revokeTarget, setRevokeTarget] = useState<string | null>(null);
  const [expandedId, setExpandedId] = useState<string | null>(null);

  const doSearch = useCallback(async (offset = 0) => {
    setLoading(true);
    try {
      const filters: SpawnRunsFilter = { limit: PAGE_SIZE, offset };
      if (status) filters.status = status;
      if (parentId) filters.parent_id = parentId;
      const data = await fetchSpawnRuns(filters);
      if (offset === 0) setRuns(data);
      else setRuns((prev) => [...prev, ...data]);
      setHasMore(data.length === PAGE_SIZE);
      setLoaded(true);
    } catch {
      // handled by empty state
    } finally {
      setLoading(false);
    }
  }, [status, parentId]);

  const handleRevoke = async () => {
    if (!revokeTarget) return;
    try {
      await revokeAgent(revokeTarget);
      setRevokeTarget(null);
      await doSearch(0);
    } catch {
      setRevokeTarget(null);
    }
  };

  return (
    <div className="space-y-6 animate-fade-in">
      <h1 className="text-2xl font-bold text-gradient-blue">{t('ipc.spawns_title')}</h1>

      {/* Filters */}
      <div className="glass-card p-4 flex flex-wrap gap-3 items-end">
        <div className="space-y-1">
          <label className="text-xs text-[#556080] uppercase tracking-wider">Status</label>
          <select value={status} onChange={(e) => setStatus(e.target.value)} className="input-electric px-3 py-2 text-sm">
            {STATUSES.map((s) => <option key={s} value={s}>{s || 'all'}</option>)}
          </select>
        </div>
        <div className="space-y-1">
          <label className="text-xs text-[#556080] uppercase tracking-wider">Parent</label>
          <input type="text" value={parentId} onChange={(e) => setParentId(e.target.value)} placeholder="parent_id" className="input-electric px-3 py-2 text-sm w-40" />
        </div>
        <button onClick={() => doSearch(0)} disabled={loading} className="btn-electric px-4 py-2 text-sm font-medium">
          {loading ? 'Loading...' : 'Search'}
        </button>
      </div>

      {/* Results */}
      {!loaded ? (
        <div className="glass-card p-12 text-center text-[#556080]">Apply filters and click Search.</div>
      ) : runs.length === 0 ? (
        <div className="glass-card p-12 text-center text-[#556080]">No spawn runs found.</div>
      ) : (
        <div className="glass-card overflow-hidden">
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-[#1a1a3e]/50 text-[#556080] text-xs uppercase tracking-wider">
                  <th className="text-left px-4 py-3">Session</th>
                  <th className="text-left px-4 py-3">Parent</th>
                  <th className="text-left px-4 py-3">Child</th>
                  <th className="text-left px-4 py-3">Status</th>
                  <th className="text-left px-4 py-3">Created</th>
                  <th className="text-left px-4 py-3">Expires</th>
                  <th className="text-left px-4 py-3">Completed</th>
                  <th className="text-right px-4 py-3">Actions</th>
                </tr>
              </thead>
              <tbody>
                {runs.map((run) => (
                  <SpawnRow
                    key={run.id}
                    run={run}
                    expanded={expandedId === run.id}
                    onToggle={() => setExpandedId(expandedId === run.id ? null : run.id)}
                    onRevoke={() => setRevokeTarget(run.child_id)}
                  />
                ))}
              </tbody>
            </table>
          </div>
          {hasMore && (
            <div className="px-4 py-3 border-t border-[#1a1a3e]/30 text-center">
              <button onClick={() => doSearch(runs.length)} disabled={loading} className="text-sm text-[#0080ff] hover:underline">
                {loading ? 'Loading...' : 'Load more'}
              </button>
            </div>
          )}
        </div>
      )}

      <ConfirmDialog
        open={revokeTarget !== null}
        title="Revoke child agent"
        message={`Revoke ephemeral agent "${revokeTarget}"? This will terminate the spawn run.`}
        destructive
        onConfirm={handleRevoke}
        onCancel={() => setRevokeTarget(null)}
      />
    </div>
  );
}

function SpawnRow({ run, expanded, onToggle, onRevoke }: {
  run: IpcSpawnRun; expanded: boolean; onToggle: () => void; onRevoke: () => void;
}) {
  return (
    <>
      <tr className="border-b border-[#1a1a3e]/30 hover:bg-[#0080ff05] transition-colors">
        <td className="px-4 py-2 font-mono text-xs">
          <Link to={`/ipc/sessions?session_id=${run.id}`} className="text-[#0080ff] hover:underline">
            {run.id.slice(0, 16)}...
          </Link>
        </td>
        <td className="px-4 py-2"><AgentLink agentId={run.parent_id} showTrust={false} /></td>
        <td className="px-4 py-2"><AgentLink agentId={run.child_id} showTrust={false} /></td>
        <td className="px-4 py-2"><StatusBadge status={run.status} /></td>
        <td className="px-4 py-2"><TimeAbsolute timestamp={run.created_at} /></td>
        <td className="px-4 py-2"><TimeUntil timestamp={run.expires_at} /></td>
        <td className="px-4 py-2">{run.completed_at ? <TimeAbsolute timestamp={run.completed_at} /> : <span className="text-[#556080]">-</span>}</td>
        <td className="px-4 py-2 text-right space-x-2">
          {run.status === 'running' && (
            <button onClick={onRevoke} className="text-xs text-red-400 hover:text-red-300">Revoke</button>
          )}
          {run.result && (
            <button onClick={onToggle} className="text-xs text-[#0080ff] hover:underline">
              {expanded ? 'Hide' : 'Result'}
            </button>
          )}
        </td>
      </tr>
      {expanded && run.result && (
        <tr>
          <td colSpan={8} className="p-4 bg-[#050510]">
            <pre className="text-sm text-[#8892a8] whitespace-pre-wrap break-all max-h-48 overflow-auto">{run.result}</pre>
          </td>
        </tr>
      )}
    </>
  );
}
