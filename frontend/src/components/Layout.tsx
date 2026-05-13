import { useState, useRef, useEffect } from 'react'
import { Outlet, NavLink, useNavigate } from 'react-router-dom'
import { LayoutDashboard, Bot, Settings, Bell, LogOut, Zap, Languages, Sun, Moon, Users, ChevronDown, Plane } from 'lucide-react'
import { authApi } from '../lib/api'
import { clearAuth, getAuth } from '../lib/auth'
import { useI18n } from '../hooks/useI18n'
import { useTheme } from '../hooks/useTheme'
import type { Locale } from '../lib/i18n'

const localeLabels: Record<Locale, string> = {
  en: 'English',
  zh: '简体中文',
}

export default function Layout() {
  const navigate = useNavigate()
  const auth = getAuth()
  const user = auth?.user
  const isAdmin = user?.roles?.includes('admin') ?? false
  const { t, locale, setLocale } = useI18n()
  const { resolved, setTheme } = useTheme()
  const [langOpen, setLangOpen] = useState(false)
  const langRef = useRef<HTMLDivElement>(null)
  const [userOpen, setUserOpen] = useState(false)
  const userRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    function handleClick(e: MouseEvent) {
      if (langRef.current && !langRef.current.contains(e.target as Node)) setLangOpen(false)
      if (userRef.current && !userRef.current.contains(e.target as Node)) setUserOpen(false)
    }
    document.addEventListener('mousedown', handleClick)
    return () => document.removeEventListener('mousedown', handleClick)
  }, [])

  const navItems: Array<{
    to: string
    label: string
    icon: typeof LayoutDashboard
    end?: boolean
  }> = [
    { to: '/', label: t.nav.dashboard, icon: LayoutDashboard, end: true },
    { to: '/agents', label: t.nav.agents, icon: Bot },
    { to: '/plane', label: t.nav.plane, icon: Plane },
    { to: '/notifications', label: t.nav.notifications, icon: Bell },
    { to: '/configs', label: t.nav.configs, icon: Settings, end: false },
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

            {/* Language selector */}
            <div className="relative" ref={langRef}>
              <button
                onClick={() => setLangOpen(!langOpen)}
                className="flex items-center gap-1.5 px-2.5 py-2 rounded-lg text-sm text-gray-500 hover:bg-gray-100 hover:text-gray-900 dark:text-gray-400 dark:hover:bg-gray-800 dark:hover:text-gray-200 transition-colors"
                title="Switch language / 切换语言"
              >
                <Languages size={16} />
                <span className="text-xs">{localeLabels[locale]}</span>
                <ChevronDown size={12} className={`transition-transform ${langOpen ? 'rotate-180' : ''}`} />
              </button>
              {langOpen && (
                <div className="absolute right-0 top-full mt-1 min-w-[120px] bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg shadow-lg py-1 z-50">
                  {(Object.keys(localeLabels) as Locale[]).map((loc) => (
                    <button
                      key={loc}
                      onClick={() => { setLocale(loc); setLangOpen(false) }}
                      className={`w-full text-left px-3 py-1.5 text-sm transition-colors ${
                        loc === locale
                          ? 'bg-sky-50 text-sky-600 dark:bg-sky-600/20 dark:text-sky-300'
                          : 'text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700'
                      }`}
                    >
                      {localeLabels[loc]}
                    </button>
                  ))}
                </div>
              )}
            </div>

            {/* Divider */}
            <div className="w-px h-5 bg-gray-200 dark:bg-gray-700 mx-1" />

            {/* User dropdown */}
            <div className="relative" ref={userRef}>
              <button
                onClick={() => setUserOpen(!userOpen)}
                className="flex items-center gap-2 px-2 py-1 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
              >
                <div className="w-7 h-7 rounded-full bg-sky-600 flex items-center justify-center text-xs font-bold text-white shrink-0">
                  {user?.display_name?.[0]?.toUpperCase() ?? 'U'}
                </div>
                <span className="text-sm font-medium text-gray-800 dark:text-gray-200 max-w-[100px] truncate">{user?.display_name}</span>
                <ChevronDown size={12} className={`text-gray-400 transition-transform ${userOpen ? 'rotate-180' : ''}`} />
              </button>
              {userOpen && (
                <div className="absolute right-0 top-full mt-1 min-w-[200px] bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg shadow-lg py-1 z-50">
                  <div className="px-3 py-2">
                    <div className="text-sm font-medium text-gray-900 dark:text-gray-100 truncate">{user?.display_name}</div>
                    <div className="text-xs text-gray-500 dark:text-gray-400 truncate">{user?.username}</div>
                  </div>
                  <div className="border-t border-gray-100 dark:border-gray-700 my-1" />
                  <div className="px-3 py-1.5 text-xs text-gray-500 dark:text-gray-400">
                    {t.layout.version} v{__APP_VERSION__}
                  </div>
                  <div className="border-t border-gray-100 dark:border-gray-700 my-1" />
                  <button
                    onClick={() => { setUserOpen(false); handleLogout() }}
                    className="w-full flex items-center gap-2 px-3 py-1.5 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 hover:text-red-600 dark:hover:text-red-400 transition-colors"
                  >
                    <LogOut size={14} />
                    {t.auth.signOut}
                  </button>
                </div>
              )}
            </div>
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
