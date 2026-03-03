import { Construction } from 'lucide-react'
import { useLocation } from 'react-router-dom'
import { useI18n } from '../../hooks/useI18n'

export default function WIPSection() {
  const { t } = useI18n()
  const location = useLocation()
  const segment = location.pathname.split('/').filter(Boolean).pop() ?? ''

  const labelMap: Record<string, string> = {
    'claude-code': t.configs.claudeCode,
    'gemini-cli': t.configs.geminiCli,
    opencode: t.configs.opencode,
    skills: t.configs.skills,
    mcp: t.configs.mcp,
  }

  const sectionName = labelMap[segment] ?? segment

  return (
    <div className="flex flex-col items-center justify-center h-full min-h-64 p-12 text-center">
      <div className="w-14 h-14 rounded-2xl bg-yellow-50 dark:bg-yellow-900/20 flex items-center justify-center mb-4">
        <Construction size={28} className="text-yellow-500 dark:text-yellow-400" />
      </div>
      <h2 className="text-lg font-semibold text-gray-800 dark:text-gray-100 mb-2">{sectionName}</h2>
      <p className="text-gray-400 dark:text-gray-500 text-sm">{t.configs.underDevelopment}</p>
    </div>
  )
}
