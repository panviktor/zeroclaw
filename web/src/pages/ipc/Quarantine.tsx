import { useState, useEffect, useCallback } from 'react';
import { t } from '@/lib/i18n';
import { fetchMessages, promoteMessage, dismissMessage } from '@/lib/ipc-api';
import type { IpcMessage } from '@/types/ipc';
import AgentLink from '@/components/ipc/AgentLink';
import KindBadge from '@/components/ipc/KindBadge';
import TimeAgo from '@/components/ipc/TimeAgo';
import ConfirmDialog from '@/components/ipc/ConfirmDialog';
import MessageDetail from '@/components/ipc/MessageDetail';

type PendingAction = { type: 'promote' | 'dismiss'; msg: IpcMessage };

export default function Quarantine() {
  const [messages, setMessages] = useState<IpcMessage[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [inspectMsg, setInspectMsg] = useState<IpcMessage | null>(null);
  const [pendingAction, setPendingAction] = useState<PendingAction | null>(null);
  const [toast, setToast] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const data = await fetchMessages({ quarantine: true, dismissed: false });
      setMessages(data);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to load quarantine');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
    const interval = setInterval(load, 15_000);
    return () => clearInterval(interval);
  }, [load]);

  const executeAction = async () => {
    if (!pendingAction) return;
    try {
      if (pendingAction.type === 'promote') {
        await promoteMessage(pendingAction.msg.id, pendingAction.msg.to_agent);
        setToast(`Message #${pendingAction.msg.id} promoted to ${pendingAction.msg.to_agent}`);
      } else {
        await dismissMessage(pendingAction.msg.id);
        setToast(`Message #${pendingAction.msg.id} dismissed`);
      }
      setPendingAction(null);
      await load();
      setTimeout(() => setToast(null), 3000);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Action failed');
      setPendingAction(null);
    }
  };

  const pendingCount = messages.filter((m) => !m.promoted && !m.blocked).length;

  if (loading) {
    return (
      <div className="flex items-center justify-center py-20 animate-fade-in">
        <div className="h-8 w-8 border-2 border-[#0080ff30] border-t-[#0080ff] rounded-full animate-spin" />
      </div>
    );
  }

  return (
    <div className="space-y-6 animate-fade-in">
      <div className="flex items-center gap-3">
        <h1 className="text-2xl font-bold text-gradient-blue">{t('ipc.quarantine_title')}</h1>
        {pendingCount > 0 && (
          <span className="px-2 py-0.5 rounded-full text-xs font-medium bg-orange-500/20 text-orange-400">
            {pendingCount} pending
          </span>
        )}
      </div>

      {error && (
        <div className="glass-card p-4 border-red-500/30 text-red-400 text-sm">{error}</div>
      )}

      {toast && (
        <div className="glass-card p-3 border-emerald-500/30 text-emerald-400 text-sm animate-fade-in">{toast}</div>
      )}

      {messages.length === 0 ? (
        <div className="glass-card p-12 text-center text-[#556080]">Quarantine queue is empty.</div>
      ) : (
        <div className="space-y-3">
          {messages.map((msg) => (
            <div key={msg.id} className="glass-card p-4 space-y-3">
              <div className="flex items-center justify-between flex-wrap gap-2">
                <div className="flex items-center gap-3">
                  <span className="text-xs text-[#556080]">#{msg.id}</span>
                  <AgentLink agentId={msg.from_agent} trustLevel={msg.from_trust_level} />
                  <span className="text-[#556080]">→</span>
                  <AgentLink agentId={msg.to_agent} showTrust={false} />
                  <KindBadge kind={msg.kind} />
                </div>
                <TimeAgo timestamp={msg.created_at} />
              </div>

              {/* Redacted payload preview */}
              <p className="text-sm text-[#8892a8] line-clamp-2">
                {msg.payload.slice(0, 200)}{msg.payload.length > 200 ? '...' : ''}
              </p>

              {/* Actions */}
              <div className="flex gap-2">
                <button
                  onClick={() => setInspectMsg(msg)}
                  className="px-3 py-1.5 text-xs font-medium text-[#0080ff] rounded-lg border border-[#1a1a3e]/50 hover:bg-[#0080ff10] transition-colors"
                >
                  Inspect
                </button>
                <button
                  onClick={() => setPendingAction({ type: 'promote', msg })}
                  className="px-3 py-1.5 text-xs font-medium text-emerald-400 rounded-lg border border-[#1a1a3e]/50 hover:bg-emerald-500/10 transition-colors"
                >
                  Promote
                </button>
                <button
                  onClick={() => setPendingAction({ type: 'dismiss', msg })}
                  className="px-3 py-1.5 text-xs font-medium text-[#556080] rounded-lg border border-[#1a1a3e]/50 hover:bg-[#1a1a3e]/30 transition-colors"
                >
                  Dismiss
                </button>
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Inspect modal */}
      {inspectMsg && (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
          <div className="absolute inset-0 bg-black/60 backdrop-blur-sm" onClick={() => setInspectMsg(null)} />
          <div className="relative w-full max-w-2xl max-h-[80vh] overflow-auto glass-card p-6 animate-fade-in-scale">
            <div className="flex justify-between items-center mb-4">
              <h3 className="text-lg font-semibold text-white">Message #{inspectMsg.id}</h3>
              <button onClick={() => setInspectMsg(null)} className="text-[#556080] hover:text-white">&times;</button>
            </div>
            <MessageDetail message={inspectMsg} />
          </div>
        </div>
      )}

      {/* Confirm action */}
      <ConfirmDialog
        open={pendingAction !== null}
        title={pendingAction?.type === 'promote' ? 'Promote message' : 'Dismiss message'}
        message={
          pendingAction?.type === 'promote'
            ? `Deliver message #${pendingAction.msg.id} to ${pendingAction.msg.to_agent}'s inbox?`
            : `Mark message #${pendingAction?.msg.id} as reviewed without delivering?`
        }
        confirmLabel={pendingAction?.type === 'promote' ? 'Promote' : 'Dismiss'}
        destructive={pendingAction?.type === 'dismiss'}
        onConfirm={executeAction}
        onCancel={() => setPendingAction(null)}
      />
    </div>
  );
}
