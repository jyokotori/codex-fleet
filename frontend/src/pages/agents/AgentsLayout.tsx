import { Outlet, NavLink } from 'react-router-dom'
import { Bot, Group, Server } from 'lucide-react'
import { useI18n } from '../../hooks/useI18n'
import { getAuth } from '../../lib/auth'

export default function AgentsLayout() {
  const { t } = useI18n()
  const isAdmin = getAuth()?.user?.roles?.includes('admin') ?? false

  const items: Array<{ label: string; to: string; icon: typeof Bot; end?: boolean }> = [
    { label: t.nav.agents, to: '/agents', icon: Bot, end: true },
    { label: t.nav.agentGroups, to: '/agents/groups', icon: Group },
  ]
  if (isAdmin) {
    items.push({ label: t.nav.servers, to: '/agents/servers', icon: Server })
  }

  return (
    <div className="flex h-full">
      <aside className="w-52 shrink-0 border-r border-gray-200 dark:border-gray-800 bg-white dark:bg-gray-900 flex flex-col py-3 gap-1 overflow-y-auto">
        {items.map(item => (
          <NavLink
            key={item.to}
            to={item.to}
            end={item.end}
            className={({ isActive }) =>
              `flex items-center gap-2 px-3 py-2 text-sm rounded-lg mx-1 transition-colors ${
                isActive
                  ? 'bg-sky-50 text-sky-600 font-medium dark:bg-sky-600/20 dark:text-sky-300'
                  : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-800 hover:text-gray-900 dark:hover:text-gray-200'
              }`
            }
          >
            <item.icon size={15} />
            <span className="flex-1">{item.label}</span>
          </NavLink>
        ))}
      </aside>

      <div className="flex-1 overflow-auto">
        <Outlet />
      </div>
    </div>
  )
}
