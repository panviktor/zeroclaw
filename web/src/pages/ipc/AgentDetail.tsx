import { useParams } from 'react-router-dom';
import { t } from '@/lib/i18n';

export default function AgentDetail() {
  const { agentId } = useParams<{ agentId: string }>();

  return (
    <div className="space-y-6 animate-fade-in">
      <h1 className="text-2xl font-bold text-gradient-blue">
        {t('ipc.agent_detail_title')}: {agentId}
      </h1>
      <div className="glass-card p-6">
        <p className="text-[#556080]">{t('ipc.agent_detail_placeholder')}</p>
      </div>
    </div>
  );
}
