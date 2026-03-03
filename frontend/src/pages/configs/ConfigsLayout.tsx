import { useState } from 'react'
import { Outlet, NavLink, useLocation } from 'react-router-dom'
import { ChevronDown, ChevronRight, FileText, Bot, Wrench, Plug, Container } from 'lucide-react'
import { useI18n } from '../../hooks/useI18n'

const WIP_BADGE = (
  <span className="ml-auto text-[10px] px-1.5 py-0.5 rounded bg-yellow-100 text-yellow-700 dark:bg-yellow-900/30 dark:text-yellow-400 font-medium">
    WIP
  </span>
)

export default function ConfigsLayout() {
  const { t } = useI18n()
  const location = useLocation()
  const [configFilesOpen, setConfigFilesOpen] = useState(true)

  const isConfigFiles = location.pathname.includes('/configs/config-files')

  const configFileItems = [
    { key: 'codex', label: t.configs.codex, to: '/configs/config-files/codex', wip: false },
    { key: 'claude-code', label: t.configs.claudeCode, to: '/configs/config-files/claude-code', wip: true },
    { key: 'gemini-cli', label: t.configs.geminiCli, to: '/configs/config-files/gemini-cli', wip: true },
    { key: 'opencode', label: t.configs.opencode, to: '/configs/config-files/opencode', wip: true },
  ]

  const topItems = [
    { label: t.configs.agentsMd, to: '/configs/agents-md', icon: Bot, wip: false },
    { label: t.configs.docker, to: '/configs/docker', icon: Container, wip: false },
    { label: t.configs.skills, to: '/configs/skills', icon: Wrench, wip: true },
    { label: t.configs.mcp, to: '/configs/mcp', icon: Plug, wip: true },
  ]

  return (
    <div className="flex h-full">
      {/* Left Sidebar */}
      <aside className="w-52 shrink-0 border-r border-gray-200 dark:border-gray-800 bg-white dark:bg-gray-900 flex flex-col py-3 gap-1 overflow-y-auto">
        {/* Config Files section (collapsible) */}
        <div>
          <button
            onClick={() => setConfigFilesOpen(v => !v)}
            className={`w-full flex items-center gap-2 px-3 py-2 text-sm font-medium rounded-lg mx-1 transition-colors ${
              isConfigFiles
                ? 'text-sky-600 dark:text-sky-400'
                : 'text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800'
            }`}
            style={{ width: 'calc(100% - 8px)' }}
          >
            <FileText size={15} />
            <span className="flex-1 text-left">{t.configs.configFiles}</span>
            {configFilesOpen ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
          </button>
          {configFilesOpen && (
            <div className="ml-2 mt-0.5 flex flex-col gap-0.5">
              {configFileItems.map(item => (
                <NavLink
                  key={item.key}
                  to={item.to}
                  className={({ isActive }) =>
                    `flex items-center gap-2 px-3 py-1.5 text-sm rounded-lg mx-1 transition-colors ${
                      item.wip
                        ? 'text-gray-400 dark:text-gray-600 cursor-default'
                        : isActive
                        ? 'bg-sky-50 text-sky-600 font-medium dark:bg-sky-600/20 dark:text-sky-300'
                        : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-800 hover:text-gray-900 dark:hover:text-gray-200'
                    }`
                  }
                  onClick={item.wip ? e => e.preventDefault() : undefined}
                >
                  <span className="flex-1">{item.label}</span>
                  {item.wip && WIP_BADGE}
                </NavLink>
              ))}
            </div>
          )}
        </div>

        <div className="h-px bg-gray-100 dark:bg-gray-800 mx-3 my-1" />

        {/* Top-level items */}
        {topItems.map(item => (
          <NavLink
            key={item.to}
            to={item.to}
            className={({ isActive }) =>
              `flex items-center gap-2 px-3 py-2 text-sm rounded-lg mx-1 transition-colors ${
                item.wip
                  ? 'text-gray-400 dark:text-gray-600 cursor-default'
                  : isActive
                  ? 'bg-sky-50 text-sky-600 font-medium dark:bg-sky-600/20 dark:text-sky-300'
                  : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-800 hover:text-gray-900 dark:hover:text-gray-200'
              }`
            }
            onClick={item.wip ? e => e.preventDefault() : undefined}
          >
            <item.icon size={15} />
            <span className="flex-1">{item.label}</span>
            {item.wip && WIP_BADGE}
          </NavLink>
        ))}
      </aside>

      {/* Content */}
      <div className="flex-1 overflow-auto">
        <Outlet />
      </div>
    </div>
  )
}
