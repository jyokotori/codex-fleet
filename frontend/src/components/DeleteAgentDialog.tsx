import { useEffect, useState } from 'react'
import { AlertTriangle, Trash2 } from 'lucide-react'
import type { Agent } from '../lib/api'
import { useI18n } from '../hooks/useI18n'

interface DeleteAgentDialogProps {
  agent: Agent | null
  open: boolean
  pending?: boolean
  onClose: () => void
  onConfirm: (agent: Agent) => void
}

export default function DeleteAgentDialog({
  agent,
  open,
  pending = false,
  onClose,
  onConfirm,
}: DeleteAgentDialogProps) {
  const { t } = useI18n()
  const [confirmed, setConfirmed] = useState(false)

  useEffect(() => {
    if (open) {
      setConfirmed(false)
    }
  }, [open, agent?.id])

  if (!open || !agent) return null

  const acknowledgeLabel = agent.use_docker
    ? t.agents.deleteDialogAcknowledgeDocker
    : t.agents.deleteDialogAcknowledgeHost
  const description = agent.use_docker
    ? t.agents.deleteDialogDescriptionDocker
    : t.agents.deleteDialogDescriptionHost
  const confirmLabel = agent.use_docker
    ? t.agents.deleteDialogConfirmDocker
    : t.agents.deleteDialogConfirmHost

  return (
    <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50 p-4">
      <div className="bg-white border border-gray-200 rounded-xl w-full max-w-lg dark:bg-gray-900 dark:border-gray-700">
        <div className="flex items-center justify-between px-6 py-4 border-b border-gray-200 dark:border-gray-700">
          <div className="flex items-center gap-3 min-w-0">
            <div className="w-9 h-9 rounded-full bg-red-100 text-red-600 dark:bg-red-900/30 dark:text-red-300 flex items-center justify-center shrink-0">
              <AlertTriangle size={16} />
            </div>
            <div className="min-w-0">
              <h3 className="font-semibold text-gray-900 dark:text-gray-100 truncate">
                {t.agents.deleteDialogTitle(agent.name)}
              </h3>
              <p className="text-xs text-gray-500 dark:text-gray-400 mt-1">
                {t.agents.deleteDialogPath(agent.id)}
              </p>
            </div>
          </div>
          <button
            onClick={onClose}
            disabled={pending}
            className="text-gray-400 hover:text-gray-700 dark:hover:text-gray-300"
          >
            ✕
          </button>
        </div>

        <div className="p-6 space-y-4">
          <div className="rounded-lg border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700 dark:border-red-900/60 dark:bg-red-950/40 dark:text-red-200">
            {description}
          </div>

          <label className="flex items-start gap-3 rounded-lg border border-gray-200 dark:border-gray-700 px-4 py-3 cursor-pointer">
            <input
              type="checkbox"
              checked={confirmed}
              onChange={(e) => setConfirmed(e.target.checked)}
              className="mt-0.5 rounded border-gray-300 dark:border-gray-600"
            />
            <span className="text-sm text-gray-700 dark:text-gray-300">
              {acknowledgeLabel}
            </span>
          </label>

          <div className="flex justify-end gap-3 pt-2">
            <button onClick={onClose} disabled={pending} className="btn-secondary">
              {t.common.cancel}
            </button>
            <button
              onClick={() => onConfirm(agent)}
              disabled={pending || !confirmed}
              className="btn-danger flex items-center gap-2 disabled:opacity-60"
            >
              <Trash2 size={14} />
              {confirmLabel}
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}
