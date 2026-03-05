import { Outlet, NavLink, useNavigate } from 'react-router-dom'
import { LayoutDashboard, Server, Bot, Settings, Bell, LogOut, Zap, Languages, Sun, Moon, Users, ClipboardList } from 'lucide-react'
import { authApi } from '../lib/api'
import { clearAuth, getAuth } from '../lib/auth'
import { useI18n } from '../hooks/useI18n'
import { useTheme } from '../hooks/useTheme'

export default function Layout() {
  const navigate = useNavigate()
  const auth = getAuth()
  const user = auth?.user
  const isAdmin = user?.roles?.includes('admin') ?? false
  const { t, locale, setLocale } = useI18n()
  const { resolved, setTheme } = useTheme()

  const navItems: Array<{
    to: string
    label: string
    icon: typeof LayoutDashboard
    end?: boolean
  }> = [
    { to: '/', label: t.nav.dashboard, icon: LayoutDashboard, end: true },
    { to: '/requirements', label: t.nav.requirements, icon: ClipboardList },
    { to: '/agents', label: t.nav.agents, icon: Bot },
    { to: '/servers', label: t.nav.servers, icon: Server },
    { to: '/configs', label: t.nav.configs, icon: Settings, end: false },
    { to: '/notifications', label: t.nav.notifications, icon: Bell },
  ]
  if (isAdmin) {
    navItems.push({ to: '/admin/users', label: t.nav.users, icon: Users })
  }

  async function handleLogout() {
    try { await authApi.logout() } catch {}
    clearAuth()
    navigate('/login')
  }

  return (
    <div className="flex flex-col h-screen bg-gray-50 dark:bg-gray-950">
      {/* Top Nav */}
      <header className="bg-white border-b border-gray-100 dark:bg-gray-900 dark:border-gray-800 shrink-0">
        <div className="flex items-center gap-6 px-6 h-14">
          {/* Logo */}
          <div className="flex items-center gap-2 shrink-0">
            <Zap className="text-sky-400" size={20} />
            <span className="text-base font-bold text-gray-900 dark:text-white tracking-wide">Codex Fleet</span>
          </div>

          {/* Nav items */}
          <nav className="flex items-center gap-1 flex-1">
            {navItems.map(({ to, label, icon: Icon, end }) => (
              <NavLink
                key={to}
                to={to}
                end={end}
                className={({ isActive }) =>
                  `flex items-center gap-2 px-3 py-2 rounded-lg text-sm transition-colors ${
                    isActive
                      ? 'bg-sky-50 text-sky-600 font-medium dark:bg-sky-600/20 dark:text-sky-300'
                      : 'text-gray-500 hover:bg-gray-100 hover:text-gray-900 dark:text-gray-400 dark:hover:bg-gray-800 dark:hover:text-gray-200'
                  }`
                }
              >
                <Icon size={16} />
                {label}
              </NavLink>
            ))}
          </nav>

          {/* Right side: theme + language + user */}
          <div className="flex items-center gap-1 shrink-0">
            {/* Theme toggle */}
            <button
              onClick={() => setTheme(resolved === 'dark' ? 'light' : 'dark')}
              className="p-2 rounded-lg text-gray-500 hover:bg-gray-100 hover:text-gray-900 dark:text-gray-400 dark:hover:bg-gray-800 dark:hover:text-gray-200 transition-colors"
              title={resolved === 'dark' ? (locale === 'zh' ? '浅色模式' : 'Light mode') : (locale === 'zh' ? '深色模式' : 'Dark mode')}
            >
              {resolved === 'dark' ? <Sun size={16} /> : <Moon size={16} />}
            </button>

            {/* Language toggle */}
            <button
              onClick={() => setLocale(locale === 'en' ? 'zh' : 'en')}
              className="flex items-center gap-1.5 px-2.5 py-2 rounded-lg text-sm text-gray-500 hover:bg-gray-100 hover:text-gray-900 dark:text-gray-400 dark:hover:bg-gray-800 dark:hover:text-gray-200 transition-colors"
              title="Switch language / 切换语言"
            >
              <Languages size={16} />
              <span className="text-xs uppercase tracking-wider">{locale === 'en' ? 'zh' : 'en'}</span>
            </button>

            {/* Divider */}
            <div className="w-px h-5 bg-gray-200 dark:bg-gray-700 mx-1" />

            {/* User */}
            <div className="flex items-center gap-2 px-2 py-1.5 rounded-lg">
              <div className="w-7 h-7 rounded-full bg-sky-600 flex items-center justify-center text-xs font-bold text-white shrink-0">
                {user?.display_name?.[0]?.toUpperCase() ?? 'U'}
              </div>
              <span className="text-sm font-medium text-gray-800 dark:text-gray-200 max-w-[100px] truncate">{user?.display_name}</span>
            </div>

            {/* Logout */}
            <button
              onClick={handleLogout}
              className="p-2 rounded-lg text-gray-400 hover:text-red-500 dark:hover:text-red-400 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
              title={t.auth.signOut}
            >
              <LogOut size={16} />
            </button>
          </div>
        </div>
      </header>

      {/* Main */}
      <main className="flex-1 overflow-auto">
        <Outlet />
      </main>
    </div>
  )
}
