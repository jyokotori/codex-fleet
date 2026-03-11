import { useEffect, useRef, useState } from 'react'
import { Copy, CheckCircle, XCircle, Loader } from 'lucide-react'
import { useWebSocket } from '../hooks/useWebSocket'
import { copyToClipboard } from '../lib/clipboard'

interface TaskLogViewerProps {
  taskId: string
  className?: string
}

export default function TaskLogViewer({ taskId, className = '' }: TaskLogViewerProps) {
  const [logs, setLogs] = useState('')
  const [threadId, setThreadId] = useState<string | null>(null)
  const [taskStatus, setTaskStatus] = useState<string>('agent_in_progress')
  const [copied, setCopied] = useState(false)
  const logRef = useRef<HTMLPreElement>(null)
  const [autoScroll, setAutoScroll] = useState(true)
  const firstLineParsed = useRef(false)

  const { isConnected } = useWebSocket(`/ws/tasks/${taskId}/logs`, {
    onMessage: (data) => {
      // Check for task_done message
      try {
        const msg = JSON.parse(data)
        if (msg.type === 'task_done') {
          setTaskStatus(msg.status)
          return
        }
      } catch {
        // Not a JSON control message
      }

      setLogs(prev => prev + data)

      // Try to parse thread_id from first JSONL line
      if (!firstLineParsed.current) {
        const lines = data.split('\n')
        for (const line of lines) {
          if (!line.trim()) continue
          try {
            const json = JSON.parse(line)
            if (json.type === 'thread.started' && json.thread_id) {
              setThreadId(json.thread_id)
            }
          } catch {
            // Not JSON
          }
          firstLineParsed.current = true
          break
        }
      }
    },
    onOpen: () => {
      setLogs('')
      firstLineParsed.current = false
    },
  })

  useEffect(() => {
    if (autoScroll && logRef.current) {
      logRef.current.scrollTop = logRef.current.scrollHeight
    }
  }, [logs, autoScroll])

  async function handleCopyThreadId() {
    if (!threadId) return
    await copyToClipboard(threadId)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  const statusIcon = taskStatus === 'agent_completed' || taskStatus === 'human_approved'
    ? <CheckCircle size={14} className="text-green-400" />
    : taskStatus === 'agent_failed' || taskStatus === 'human_rejected'
    ? <XCircle size={14} className="text-red-400" />
    : <Loader size={14} className="text-yellow-400 animate-spin" />

  const statusColor = (taskStatus === 'agent_completed' || taskStatus === 'human_approved') ? 'badge-green'
    : (taskStatus === 'agent_failed' || taskStatus === 'human_rejected') ? 'badge-red'
    : 'badge-yellow'

  return (
    <div className={`flex flex-col gap-3 ${className}`}>
      {/* Header with thread_id and status */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          {statusIcon}
          <span className={statusColor}>{taskStatus}</span>
        </div>
        {threadId && (
          <button
            onClick={handleCopyThreadId}
            className="flex items-center gap-1.5 text-xs font-mono text-gray-400 hover:text-gray-200 bg-gray-800 px-2 py-1 rounded border border-gray-700"
            title="Copy thread ID"
          >
            <Copy size={11} />
            <span className="truncate max-w-[200px]">{threadId}</span>
            {copied && <span className="text-green-400 text-xs ml-1">Copied!</span>}
          </button>
        )}
      </div>

      {/* Log output */}
      <div className="flex flex-col">
        <div className="flex items-center justify-between px-3 py-2 bg-gray-100 dark:bg-gray-800 rounded-t-lg border border-gray-200 dark:border-gray-700 border-b-0">
          <span className="text-xs text-gray-600 dark:text-gray-400">Task Output</span>
          <div className="flex items-center gap-3">
            <label className="flex items-center gap-1.5 text-xs text-gray-600 dark:text-gray-400 cursor-pointer">
              <input type="checkbox" checked={autoScroll} onChange={e => setAutoScroll(e.target.checked)} className="w-3 h-3" />
              Auto-scroll
            </label>
            <div className={`w-2 h-2 rounded-full ${isConnected ? 'bg-green-400' : 'bg-gray-400'}`} />
          </div>
        </div>
        <pre
          ref={logRef}
          className="flex-1 overflow-auto bg-black text-gray-300 text-xs p-4 rounded-b-lg border border-gray-200 dark:border-gray-700 font-mono leading-relaxed whitespace-pre-wrap"
          style={{ minHeight: '300px', maxHeight: '500px' }}
        >
          {logs || (isConnected ? 'Waiting for output...' : 'Connecting...')}
        </pre>
      </div>
    </div>
  )
}
