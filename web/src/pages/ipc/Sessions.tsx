import { t } from '@/lib/i18n';

export default function Sessions() {
  return (
    <div className="space-y-6 animate-fade-in">
      <h1 className="text-2xl font-bold text-gradient-blue">{t('ipc.sessions_title')}</h1>
      <div className="glass-card p-6">
        <p className="text-[#556080]">{t('ipc.sessions_placeholder')}</p>
      </div>
    </div>
  );
}
