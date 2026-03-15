import { useState, useCallback, useEffect, useRef } from 'react';
import { useSearchParams } from 'react-router-dom';
import { t } from '@/lib/i18n';
import { fetchMessages } from '@/lib/ipc-api';
import type { IpcMessage, MessagesFilter } from '@/types/ipc';
import KindBadge from '@/components/ipc/KindBadge';
import LaneDot from '@/components/ipc/LaneDot';
import AgentLink from '@/components/ipc/AgentLink';
import { TimeAbsolute } from '@/components/ipc/TimeAgo';
import MessageDetail from '@/components/ipc/MessageDetail';
import { redactPayload } from '@/components/ipc/redact';
import { TIME_RANGES, timeRangeToTs } from '@/components/ipc/TimeRangeFilter';

const KINDS = ['', 'text', 'task', 'query', 'result'];
const LANES = ['', 'normal', 'quarantine', 'blocked'];
const PAGE_SIZE = 50;

export default function Sessions() {
  const [searchParams, setSearchParams] = useSearchParams();
  const [messages, setMessages] = useState<IpcMessage[]>([]);
  const [loading, setLoading] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [expandedId, setExpandedId] = useState<number | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const [timeRange, setTimeRange] = useState('');
  const userInputRef = useRef(false);

  const agentId = searchParams.get('agent_id') ?? '';
  const sessionId = searchParams.get('session_id') ?? '';
  const kind = searchParams.get('kind') ?? '';
  const lane = searchParams.get('lane') ?? '';

  const doSearch = useCallback(async (offset = 0) => {
    setLoading(true);
    try {
      const filters: MessagesFilter = { limit: PAGE_SIZE, offset };
      if (agentId) filters.agent_id = agentId;
      if (sessionId) filters.session_id = sessionId;
      if (kind) filters.kind = kind;
      if (lane) filters.lane = lane;
      const fromTs = timeRangeToTs(timeRange);
      if (fromTs) filters.from_ts = fromTs;
      const data = await fetchMessages(filters);
      if (offset === 0) {
        setMessages(data);
      } else {
        setMessages((prev) => [...prev, ...data]);
      }
      setHasMore(data.length === PAGE_SIZE);
      setLoaded(true);
    } catch {
      // error handled by empty state
    } finally {
      setLoading(false);
    }
  }, [agentId, sessionId, kind, lane, timeRange]);

  // Auto-load on navigation-driven param changes (cross-links, back/forward)
  useEffect(() => {
    if (userInputRef.current) {
      userInputRef.current = false;
      return;
    }
    if (agentId || sessionId || kind || lane) {
      doSearch(0);
    }
  }, [agentId, sessionId, kind, lane, doSearch]);

  const updateParam = (key: string, value: string) => {
    userInputRef.current = true;
    const params = new URLSearchParams(searchParams);
    if (value) params.set(key, value);
    else params.delete(key);
    setSearchParams(params);
  };

  return (
    <div className="space-y-6 animate-fade-in">
      <h1 className="text-2xl font-bold text-gradient-blue">{t('ipc.sessions_title')}</h1>

      {/* Filters */}
      <div className="glass-card p-4 flex flex-wrap gap-3 items-end">
        <FilterInput label="Agent" value={agentId} onChange={(v) => updateParam('agent_id', v)} placeholder="agent_id" />
        <FilterInput label="Session" value={sessionId} onChange={(v) => updateParam('session_id', v)} placeholder="session_id" />
        <FilterSelect label="Kind" value={kind} options={KINDS} onChange={(v) => updateParam('kind', v)} />
        <FilterSelect label="Lane" value={lane} options={LANES} onChange={(v) => updateParam('lane', v)} />
        <FilterSelect label="Time" value={timeRange} options={TIME_RANGES.map((r) => r.value)} onChange={setTimeRange} labels={TIME_RANGES.map((r) => r.label)} />
        <button
          onClick={() => doSearch(0)}
          disabled={loading}
          className="btn-electric px-4 py-2 text-sm font-medium"
        >
          {loading ? 'Loading...' : 'Search'}
        </button>
      </div>

      {/* Results */}
      {!loaded ? (
        <div className="glass-card p-12 text-center text-[#556080]">Apply filters and click Search.</div>
      ) : messages.length === 0 ? (
        <div className="glass-card p-12 text-center text-[#556080]">No messages found.</div>
      ) : (
        <div className="glass-card overflow-hidden">
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-[#1a1a3e]/50 text-[#556080] text-xs uppercase tracking-wider">
                  <th className="text-left px-4 py-3">Time</th>
                  <th className="text-left px-4 py-3">From → To</th>
                  <th className="text-left px-4 py-3">Kind</th>
                  <th className="text-center px-4 py-3">Lane</th>
                  <th className="text-left px-4 py-3">Seq</th>
                  <th className="text-left px-4 py-3">Payload</th>
                </tr>
              </thead>
              <tbody>
                {messages.map((msg) => (
                  <MsgRow
                    key={msg.id}
                    msg={msg}
                    expanded={expandedId === msg.id}
                    onToggle={() => setExpandedId(expandedId === msg.id ? null : msg.id)}
                  />
                ))}
              </tbody>
            </table>
          </div>
          {hasMore && (
            <div className="px-4 py-3 border-t border-[#1a1a3e]/30 text-center">
              <button
                onClick={() => doSearch(messages.length)}
                disabled={loading}
                className="text-sm text-[#0080ff] hover:underline"
              >
                {loading ? 'Loading...' : 'Load more'}
              </button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function MsgRow({ msg, expanded, onToggle }: { msg: IpcMessage; expanded: boolean; onToggle: () => void }) {
  return (
    <>
      <tr
        onClick={onToggle}
        className="border-b border-[#1a1a3e]/30 hover:bg-[#0080ff05] cursor-pointer transition-colors"
      >
        <td className="px-4 py-2"><TimeAbsolute timestamp={msg.created_at} /></td>
        <td className="px-4 py-2">
          <span className="inline-flex items-center gap-1.5">
            <AgentLink agentId={msg.from_agent} trustLevel={msg.from_trust_level} />
            <span className="text-[#556080]">→</span>
            <AgentLink agentId={msg.to_agent} showTrust={false} />
          </span>
        </td>
        <td className="px-4 py-2"><KindBadge kind={msg.kind} /></td>
        <td className="px-4 py-2 text-center"><LaneDot lane={msg.lane} /></td>
        <td className="px-4 py-2 font-mono text-xs text-[#556080]">{msg.seq}</td>
        <td className="px-4 py-2 text-[#8892a8] max-w-xs truncate">{redactPayload(msg.payload, msg.kind)}</td>
      </tr>
      {expanded && (
        <tr>
          <td colSpan={6} className="p-4 bg-[#050510]">
            <MessageDetail message={msg} />
          </td>
        </tr>
      )}
    </>
  );
}

function FilterInput({ label, value, onChange, placeholder }: {
  label: string; value: string; onChange: (v: string) => void; placeholder?: string;
}) {
  return (
    <div className="space-y-1">
      <label className="text-xs text-[#556080] uppercase tracking-wider">{label}</label>
      <input
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className="input-electric px-3 py-2 text-sm w-40"
      />
    </div>
  );
}

function FilterSelect({ label, value, options, onChange, labels }: {
  label: string; value: string; options: string[]; onChange: (v: string) => void; labels?: string[];
}) {
  return (
    <div className="space-y-1">
      <label className="text-xs text-[#556080] uppercase tracking-wider">{label}</label>
      <select
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="input-electric px-3 py-2 text-sm"
      >
        {options.map((o, i) => (
          <option key={o} value={o}>{labels ? labels[i] : (o || 'all')}</option>
        ))}
      </select>
    </div>
  );
}
