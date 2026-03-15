import { NavLink } from 'react-router-dom';
import { useState, useEffect } from 'react';
import {
  LayoutDashboard,
  MessageSquare,
  Wrench,
  Clock,
  Puzzle,
  Brain,
  Settings,
  DollarSign,
  Activity,
  Stethoscope,
  Network,
  Users,
  ScrollText,
  Rocket,
  ShieldAlert,
  FileSearch,
} from 'lucide-react';
import { t } from '@/lib/i18n';
import { checkIpcAccess } from '@/lib/ipc-api';

interface NavItem {
  to: string;
  icon: React.ComponentType<{ className?: string }>;
  labelKey: string;
  end?: boolean;
}

const navItems: NavItem[] = [
  { to: '/', icon: LayoutDashboard, labelKey: 'nav.dashboard', end: true },
  { to: '/agent', icon: MessageSquare, labelKey: 'nav.agent' },
  { to: '/tools', icon: Wrench, labelKey: 'nav.tools' },
  { to: '/cron', icon: Clock, labelKey: 'nav.cron' },
  { to: '/integrations', icon: Puzzle, labelKey: 'nav.integrations' },
  { to: '/memory', icon: Brain, labelKey: 'nav.memory' },
  { to: '/config', icon: Settings, labelKey: 'nav.config' },
  { to: '/cost', icon: DollarSign, labelKey: 'nav.cost' },
  { to: '/logs', icon: Activity, labelKey: 'nav.logs' },
  { to: '/doctor', icon: Stethoscope, labelKey: 'nav.doctor' },
];

const ipcNavItems: NavItem[] = [
  { to: '/ipc/fleet', icon: Users, labelKey: 'nav.ipc_fleet' },
  { to: '/ipc/sessions', icon: ScrollText, labelKey: 'nav.ipc_sessions' },
  { to: '/ipc/spawns', icon: Rocket, labelKey: 'nav.ipc_spawns' },
  { to: '/ipc/quarantine', icon: ShieldAlert, labelKey: 'nav.ipc_quarantine' },
  { to: '/ipc/audit', icon: FileSearch, labelKey: 'nav.ipc_audit' },
];

function NavLinkItem({ to, icon: Icon, labelKey, end, idx }: NavItem & { idx: number }) {
  return (
    <NavLink
      key={to}
      to={to}
      end={end}
      className={({ isActive }) =>
        [
          'flex items-center gap-3 px-3 py-2.5 rounded-xl text-sm font-medium transition-all duration-300 animate-slide-in-left group',
          isActive
            ? 'text-white shadow-[0_0_15px_rgba(0,128,255,0.2)]'
            : 'text-[#556080] hover:text-white hover:bg-[#0080ff08]',
        ].join(' ')
      }
      style={({ isActive }) => ({
        animationDelay: `${idx * 40}ms`,
        ...(isActive ? { background: 'linear-gradient(135deg, rgba(0,128,255,0.15), rgba(0,128,255,0.05))' } : {}),
      })}
    >
      {({ isActive }) => (
        <>
          <Icon className={`h-5 w-5 flex-shrink-0 transition-colors duration-300 ${isActive ? 'text-[#0080ff]' : 'group-hover:text-[#0080ff80]'}`} />
          <span>{t(labelKey)}</span>
          {isActive && (
            <div className="ml-auto h-1.5 w-1.5 rounded-full bg-[#0080ff] glow-dot" />
          )}
        </>
      )}
    </NavLink>
  );
}

export default function Sidebar() {
  const [ipcAvailable, setIpcAvailable] = useState(false);

  useEffect(() => {
    checkIpcAccess().then(setIpcAvailable);
  }, []);

  return (
    <aside className="fixed top-0 left-0 h-screen w-60 flex flex-col" style={{ background: 'linear-gradient(180deg, #080818 0%, #050510 100%)' }}>
      {/* Glow line on right edge */}
      <div className="sidebar-glow-line" />

      {/* Logo / Title */}
      <div className="flex items-center gap-3 px-4 py-4 border-b border-[#1a1a3e]/50">
        <img
          src="/_app/logo.png"
          alt="ZeroClaw"
          className="h-10 w-10 rounded-xl object-cover animate-pulse-glow"
        />
        <span className="text-lg font-bold text-gradient-blue tracking-wide">
          ZeroClaw
        </span>
      </div>

      {/* Navigation */}
      <nav className="flex-1 overflow-y-auto py-4 px-3 space-y-1">
        {navItems.map((item, idx) => (
          <NavLinkItem key={item.to} {...item} idx={idx} />
        ))}

        {/* IPC Section */}
        {ipcAvailable && (
          <>
            <div className="pt-4 pb-1 px-3">
              <div className="flex items-center gap-2 text-[10px] text-[#334060] tracking-wider uppercase font-semibold">
                <Network className="h-3 w-3" />
                <span>{t('nav.ipc_section')}</span>
              </div>
            </div>
            {ipcNavItems.map((item, idx) => (
              <NavLinkItem key={item.to} {...item} idx={navItems.length + idx} />
            ))}
          </>
        )}
      </nav>

      {/* Footer */}
      <div className="px-5 py-4 border-t border-[#1a1a3e]/50">
        <p className="text-[10px] text-[#334060] tracking-wider uppercase">ZeroClaw Runtime</p>
      </div>
    </aside>
  );
}
