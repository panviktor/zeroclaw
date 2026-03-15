import { useState, useEffect, useCallback } from 'react';
import { t } from '@/lib/i18n';
import { fetchFleet, revokeAgent, quarantineAgent, disableAgent, downgradeAgent } from '@/lib/ipc-api';
import type { IpcAgent } from '@/types/ipc';
import TrustBadge from '@/components/ipc/TrustBadge';
import StatusBadge from '@/components/ipc/StatusBadge';
import KeyStatusIcon from '@/components/ipc/KeyStatusIcon';
import TimeAgo from '@/components/ipc/TimeAgo';
import AgentLink from '@/components/ipc/AgentLink';
import ConfirmDialog from '@/components/ipc/ConfirmDialog';

type ActionType = 'revoke' | 'quarantine' | 'disable' | 'downgrade';

interface PendingAction {
  type: ActionType;
  agent: IpcAgent;
  level?: number;
}

export default function Fleet() {
  const [agents, setAgents] = useState<IpcAgent[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [pendingAction, setPendingAction] = useState<PendingAction | null>(null);
  const [actionLoading, setActionLoading] = useState(false);

  const load = useCallback(async () => {
    try {
      const data = await fetchFleet();
      setAgents(data);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to load agents');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
    const interval = setInterval(load, 10_000);
    return () => clearInterval(interval);
  }, [load]);

  const executeAction = async () => {
    if (!pendingAction) return;
    setActionLoading(true);
    try {
      const { type, agent, level } = pendingAction;
      switch (type) {
        case 'revoke':
          await revokeAgent(agent.agent_id);
          break;
        case 'quarantine':
          await quarantineAgent(agent.agent_id);
          break;
        case 'disable':
          await disableAgent(agent.agent_id);
          break;
        case 'downgrade':
          if (level !== undefined) await downgradeAgent(agent.agent_id, level);
          break;
      }
      setPendingAction(null);
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Action failed');
    } finally {
      setActionLoading(false);
    }
  };

  const confirmMessage = pendingAction
    ? `${pendingAction.type} agent "${pendingAction.agent.agent_id}"${
        pendingAction.type === 'downgrade' ? ` to L${pendingAction.level}` : ''
      }?`
    : '';

  if (loading) {
    return (
      <div className="flex items-center justify-center py-20 animate-fade-in">
        <div className="h-8 w-8 border-2 border-[#0080ff30] border-t-[#0080ff] rounded-full animate-spin" />
      </div>
    );
  }

  return (
    <div className="space-y-6 animate-fade-in">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold text-gradient-blue">{t('ipc.fleet_title')}</h1>
        <span className="text-sm text-[#556080]">{agents.length} agents</span>
      </div>

      {error && (
        <div className="glass-card p-4 border-red-500/30 text-red-400 text-sm">{error}</div>
      )}

      {agents.length === 0 ? (
        <div className="glass-card p-12 text-center">
          <p className="text-[#556080]">No agents registered. Pair an agent to get started.</p>
        </div>
      ) : (
        <div className="glass-card overflow-hidden">
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-[#1a1a3e]/50 text-[#556080] text-xs uppercase tracking-wider">
                  <th className="text-left px-4 py-3">Agent</th>
                  <th className="text-left px-4 py-3">Role</th>
                  <th className="text-left px-4 py-3">Trust</th>
                  <th className="text-left px-4 py-3">Status</th>
                  <th className="text-left px-4 py-3">Last Seen</th>
                  <th className="text-center px-4 py-3">Key</th>
                  <th className="text-right px-4 py-3">Actions</th>
                </tr>
              </thead>
              <tbody>
                {agents.map((agent) => (
                  <AgentRow
                    key={agent.agent_id}
                    agent={agent}
                    onAction={setPendingAction}
                  />
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      <ConfirmDialog
        open={pendingAction !== null}
        title={`${pendingAction?.type ?? ''}`}
        message={confirmMessage}
        confirmLabel={actionLoading ? 'Processing...' : 'Confirm'}
        destructive
        onConfirm={executeAction}
        onCancel={() => setPendingAction(null)}
      />
    </div>
  );
}

function AgentRow({
  agent,
  onAction,
}: {
  agent: IpcAgent;
  onAction: (action: PendingAction) => void;
}) {
  const [showMenu, setShowMenu] = useState(false);
  const isActive = agent.status === 'online' || agent.status === 'stale';

  return (
    <tr className="border-b border-[#1a1a3e]/30 hover:bg-[#0080ff05] transition-colors">
      <td className="px-4 py-3">
        <AgentLink agentId={agent.agent_id} trustLevel={null} showTrust={false} />
      </td>
      <td className="px-4 py-3 text-[#8892a8]">{agent.role ?? '-'}</td>
      <td className="px-4 py-3">
        <TrustBadge level={agent.trust_level} />
      </td>
      <td className="px-4 py-3">
        <StatusBadge status={agent.status} />
      </td>
      <td className="px-4 py-3">
        {agent.last_seen ? <TimeAgo timestamp={agent.last_seen} staleThreshold={300} /> : '-'}
      </td>
      <td className="px-4 py-3 text-center">
        <KeyStatusIcon publicKey={agent.public_key} />
      </td>
      <td className="px-4 py-3 text-right relative">
        <button
          onClick={() => setShowMenu(!showMenu)}
          className="text-xs text-[#556080] hover:text-white px-2 py-1 rounded hover:bg-[#1a1a3e]/50 transition-colors"
        >
          Actions
        </button>
        {showMenu && (
          <>
            <div className="fixed inset-0 z-10" onClick={() => setShowMenu(false)} />
            <div className="absolute right-4 top-full mt-1 z-20 glass-card py-1 min-w-[140px] shadow-lg">
              {isActive && (
                <>
                  <MenuButton
                    label="Disable"
                    onClick={() => {
                      setShowMenu(false);
                      onAction({ type: 'disable', agent });
                    }}
                  />
                  <MenuButton
                    label="Quarantine"
                    onClick={() => {
                      setShowMenu(false);
                      onAction({ type: 'quarantine', agent });
                    }}
                  />
                  {(agent.trust_level ?? 0) < 4 && (
                    <MenuButton
                      label="Downgrade to L4"
                      onClick={() => {
                        setShowMenu(false);
                        onAction({ type: 'downgrade', agent, level: 4 });
                      }}
                    />
                  )}
                </>
              )}
              <MenuButton
                label="Revoke"
                className="text-red-400 hover:text-red-300"
                onClick={() => {
                  setShowMenu(false);
                  onAction({ type: 'revoke', agent });
                }}
              />
            </div>
          </>
        )}
      </td>
    </tr>
  );
}

function MenuButton({
  label,
  onClick,
  className = '',
}: {
  label: string;
  onClick: () => void;
  className?: string;
}) {
  return (
    <button
      onClick={onClick}
      className={`w-full text-left px-3 py-1.5 text-xs hover:bg-[#1a1a3e]/50 transition-colors ${className || 'text-[#8892a8] hover:text-white'}`}
    >
      {label}
    </button>
  );
}
