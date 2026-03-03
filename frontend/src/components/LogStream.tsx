import { useEffect, useRef, useState } from 'react'
import { useWebSocket } from '../hooks/useWebSocket'
import { useI18n } from '../hooks/useI18n'

interface LogStreamProps {
  agentId: string
  className?: string
}

export default function LogStream({ agentId, className = '' }: LogStreamProps) {
  const { t } = useI18n()
  const [logs, setLogs] = useState<string>('')
  const logRef = useRef<HTMLPreElement>(null)
  const [autoScroll, setAutoScroll] = useState(true)

  const { isConnected } = useWebSocket(`/ws/agents/${agentId}/logs`, {
    onMessage: (data) => setLogs(prev => prev + data),
    onOpen: () => setLogs(''),
  })

  useEffect(() => {
    if (autoScroll && logRef.current) {
      logRef.current.scrollTop = logRef.current.scrollHeight
    }
  }, [logs, autoScroll])

  return (
    <div className={`flex flex-col ${className}`}>
      <div className="flex items-center justify-between px-3 py-2 bg-gray-100 dark:bg-gray-800 rounded-t-lg border border-gray-200 dark:border-gray-700 border-b-0">
        <span className="text-xs text-gray-600 dark:text-gray-400">{t.agentDetail.liveLogs}</span>
        <div className="flex items-center gap-3">
          <label className="flex items-center gap-1.5 text-xs text-gray-600 dark:text-gray-400 cursor-pointer">
            <input type="checkbox" checked={autoScroll} onChange={e => setAutoScroll(e.target.checked)} className="w-3 h-3" />
            {t.agentDetail.autoScroll}
          </label>
          <div className={`w-2 h-2 rounded-full ${isConnected ? 'bg-green-400' : 'bg-red-400'}`} />
          <span className="text-xs text-gray-500">{isConnected ? t.agentDetail.connected : t.agentDetail.disconnected}</span>
        </div>
      </div>
      <pre
        ref={logRef}
        className="flex-1 overflow-auto bg-black text-green-300 text-xs p-4 rounded-b-lg border border-gray-200 dark:border-gray-700 font-mono leading-relaxed whitespace-pre-wrap"
        style={{ minHeight: '300px', maxHeight: '500px' }}
      >
        {logs || (isConnected ? t.agentDetail.waiting : t.agentDetail.connecting)}
      </pre>
    </div>
  )
}
