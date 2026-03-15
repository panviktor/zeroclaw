import { t } from '@/lib/i18n';

export default function Audit() {
  return (
    <div className="space-y-6 animate-fade-in">
      <h1 className="text-2xl font-bold text-gradient-blue">{t('ipc.audit_title')}</h1>
      <div className="glass-card p-6">
        <p className="text-[#556080]">{t('ipc.audit_placeholder')}</p>
      </div>
    </div>
  );
}
