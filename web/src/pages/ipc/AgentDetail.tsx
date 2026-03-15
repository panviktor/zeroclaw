import { useState, useEffect, useCallback } from 'react';
import { useParams, Link } from 'react-router-dom';
import { fetchAgentDetail, revokeAgent, quarantineAgent, disableAgent } from '@/lib/ipc-api';
import type { IpcAgentDetail } from '@/types/ipc';
import TrustBadge from '@/components/ipc/TrustBadge';
import StatusBadge from '@/components/ipc/StatusBadge';
import KeyStatusIcon from '@/components/ipc/KeyStatusIcon';
import KindBadge from '@/components/ipc/KindBadge';
import LaneDot from '@/components/ipc/LaneDot';
import AgentLink from '@/components/ipc/AgentLink';
import TimeAgo, { TimeAbsolute, TimeUntil } from '@/components/ipc/TimeAgo';
import ConfirmDialog from '@/components/ipc/ConfirmDialog';

export default function AgentDetail() {
  const { agentId } = useParams<{ agentId: string }>();
  const [detail, setDetail] = useState<IpcAgentDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [confirmAction, setConfirmAction] = useState<string | null>(null);

  const load = useCallback(async () => {
    if (!agentId) return;
    try {
      const data = await fetchAgentDetail(agentId);
      setDetail(data);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to load agent');
    } finally {
      setLoading(false);
    }
  }, [agentId]);

  useEffect(() => { load(); }, [load]);

  const executeAction = async () => {
    if (!agentId || !confirmAction) return;
    try {
      if (confirmAction === 'revoke') await revokeAgent(agentId);
      else if (confirmAction === 'quarantine') await quarantineAgent(agentId);
      else if (confirmAction === 'disable') await disableAgent(agentId);
      setConfirmAction(null);
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Action failed');
      setConfirmAction(null);
    }
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center py-20 animate-fade-in">
        <div className="h-8 w-8 border-2 border-[#0080ff30] border-t-[#0080ff] rounded-full animate-spin" />
      </div>
    );
  }

  if (error || !detail) {
    return (
      <div className="space-y-4 animate-fade-in">
        <Link to="/ipc/fleet" className="text-sm text-[#0080ff] hover:underline">&larr; Back to Fleet</Link>
        <div className="glass-card p-6 text-red-400">{error ?? 'Agent not found'}</div>
      </div>
    );
  }

  const { agent, recent_messages, active_spawns, quarantine_count } = detail;

  return (
    <div className="space-y-6 animate-fade-in">
      <Link to="/ipc/fleet" className="text-sm text-[#0080ff] hover:underline">&larr; Back to Fleet</Link>

      {/* Identity card */}
      <div className="glass-card p-6">
        <div className="flex items-start justify-between flex-wrap gap-4">
          <div className="space-y-2">
            <h1 className="text-2xl font-bold text-white font-mono">{agent.agent_id}</h1>
            <div className="flex items-center gap-3 flex-wrap">
              <StatusBadge status={agent.status} />
              <TrustBadge level={agent.trust_level} />
              <KeyStatusIcon publicKey={agent.public_key} />
              {agent.role && <span className="text-sm text-[#8892a8]">role: {agent.role}</span>}
            </div>
            {agent.public_key && (
              <p className="text-xs text-[#556080] font-mono">
                key: {agent.public_key.slice(0, 16)}...
              </p>
            )}
            {agent.last_seen && (
              <p className="text-xs text-[#556080]">
                last seen: <TimeAgo timestamp={agent.last_seen} />
              </p>
            )}
            {quarantine_count > 0 && (
              <p className="text-xs text-orange-400">
                {quarantine_count} pending quarantine message{quarantine_count > 1 ? 's' : ''}
              </p>
            )}
          </div>
          <div className="flex gap-2">
            <ActionBtn label="Disable" onClick={() => setConfirmAction('disable')} />
            <ActionBtn label="Quarantine" onClick={() => setConfirmAction('quarantine')} />
            <ActionBtn label="Revoke" className="text-red-400 hover:bg-red-500/10" onClick={() => setConfirmAction('revoke')} />
          </div>
        </div>
      </div>

      {/* Recent Messages */}
      <CollapsibleSection title="Recent Messages" count={recent_messages.length}>
        {recent_messages.length === 0 ? (
          <p className="text-[#556080] text-sm p-4">No messages.</p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-[#1a1a3e]/50 text-[#556080] text-xs uppercase tracking-wider">
                  <th className="text-left px-4 py-2">Time</th>
                  <th className="text-left px-4 py-2">Direction</th>
                  <th className="text-left px-4 py-2">Peer</th>
                  <th className="text-left px-4 py-2">Kind</th>
                  <th className="text-left px-4 py-2">Lane</th>
                  <th className="text-left px-4 py-2">Payload</th>
                </tr>
              </thead>
              <tbody>
                {recent_messages.map((msg) => {
                  const isSender = msg.from_agent === agentId;
                  return (
                    <tr key={msg.id} className="border-b border-[#1a1a3e]/30 hover:bg-[#0080ff05]">
                      <td className="px-4 py-2"><TimeAbsolute timestamp={msg.created_at} /></td>
                      <td className="px-4 py-2 text-[#556080]">{isSender ? '→' : '←'}</td>
                      <td className="px-4 py-2">
                        <AgentLink
                          agentId={isSender ? msg.to_agent : msg.from_agent}
                          trustLevel={isSender ? null : msg.from_trust_level}
                        />
                      </td>
                      <td className="px-4 py-2"><KindBadge kind={msg.kind} /></td>
                      <td className="px-4 py-2"><LaneDot lane={msg.lane} /></td>
                      <td className="px-4 py-2 text-[#8892a8] max-w-xs truncate">{msg.payload.slice(0, 100)}</td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </CollapsibleSection>

      {/* Active Spawn Runs */}
      <CollapsibleSection title="Active Spawn Runs" count={active_spawns.length}>
        {active_spawns.length === 0 ? (
          <p className="text-[#556080] text-sm p-4">No active spawns.</p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-[#1a1a3e]/50 text-[#556080] text-xs uppercase tracking-wider">
                  <th className="text-left px-4 py-2">Session</th>
                  <th className="text-left px-4 py-2">Role</th>
                  <th className="text-left px-4 py-2">Peer</th>
                  <th className="text-left px-4 py-2">Status</th>
                  <th className="text-left px-4 py-2">Created</th>
                  <th className="text-left px-4 py-2">Expires</th>
                </tr>
              </thead>
              <tbody>
                {active_spawns.map((spawn) => {
                  const isParent = spawn.parent_id === agentId;
                  return (
                    <tr key={spawn.id} className="border-b border-[#1a1a3e]/30">
                      <td className="px-4 py-2 font-mono text-xs">
                        <Link to={`/ipc/sessions?session_id=${spawn.id}`} className="text-[#0080ff] hover:underline">
                          {spawn.id.slice(0, 12)}...
                        </Link>
                      </td>
                      <td className="px-4 py-2 text-[#8892a8]">{isParent ? 'parent' : 'child'}</td>
                      <td className="px-4 py-2">
                        <AgentLink agentId={isParent ? spawn.child_id : spawn.parent_id} showTrust={false} />
                      </td>
                      <td className="px-4 py-2"><StatusBadge status={spawn.status} /></td>
                      <td className="px-4 py-2"><TimeAbsolute timestamp={spawn.created_at} /></td>
                      <td className="px-4 py-2"><TimeUntil timestamp={spawn.expires_at} /></td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </CollapsibleSection>

      <ConfirmDialog
        open={confirmAction !== null}
        title={confirmAction ?? ''}
        message={`${confirmAction} agent "${agentId}"?`}
        destructive
        onConfirm={executeAction}
        onCancel={() => setConfirmAction(null)}
      />
    </div>
  );
}

function ActionBtn({ label, onClick, className = '' }: { label: string; onClick: () => void; className?: string }) {
  return (
    <button
      onClick={onClick}
      className={`px-3 py-1.5 text-xs font-medium rounded-lg border border-[#1a1a3e]/50 hover:bg-[#1a1a3e]/30 transition-colors ${className || 'text-[#8892a8] hover:text-white'}`}
    >
      {label}
    </button>
  );
}

function CollapsibleSection({ title, count, children }: { title: string; count: number; children: React.ReactNode }) {
  const [open, setOpen] = useState(true);

  return (
    <div className="glass-card overflow-hidden">
      <button
        onClick={() => setOpen(!open)}
        className="w-full flex items-center justify-between px-6 py-3 text-sm font-medium text-white hover:bg-[#0080ff05] transition-colors"
      >
        <span>{title} ({count})</span>
        <span className="text-[#556080]">{open ? '▾' : '▸'}</span>
      </button>
      {open && children}
    </div>
  );
}
