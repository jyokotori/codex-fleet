import { useEffect, useRef, useState } from 'react'
import { useWebSocket } from '../hooks/useWebSocket'

interface ProvisionLogProps {
  agentId: string
  onDone?: (status: string) => void
}

export default function ProvisionLog({ agentId, onDone }: ProvisionLogProps) {
  const [lines, setLines] = useState<string>('')
  const [done, setDone] = useState(false)
  const [doneStatus, setDoneStatus] = useState<string>('')
  const doneRef = useRef(false)
  const scrollRef = useRef<HTMLDivElement>(null)

  const { isConnected } = useWebSocket(`/ws/agents/${agentId}/provision`, {
    maxReconnects: 0,
    onMessage: (data) => {
      // Check if it's a JSON "done" frame
      try {
        const parsed = JSON.parse(data)
        if (parsed.done) {
          doneRef.current = true
          setDone(true)
          setDoneStatus(parsed.status ?? 'unknown')
          onDone?.(parsed.status ?? 'unknown')
          return
        }
      } catch {
        // not JSON, treat as log text
      }
      if (!doneRef.current) {
        setLines(prev => prev + data)
      }
    },
  })

  // Auto-scroll to bottom
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight
    }
  }, [lines])

  // Highlight [Step N] lines
  function renderLog(text: string) {
    return text.split('\n').map((line, i) => {
      const isStep = /^\[Step \d+\]/.test(line)
      const isDone = /^\[Done\]/.test(line)
      const isError = /^\[Error\]/.test(line)
      const isWarn = /\[warn\]/.test(line)

      let cls = 'text-green-400'
      if (isStep) cls = 'text-yellow-300 font-bold'
      if (isDone) cls = 'text-cyan-300 font-bold'
      if (isError) cls = 'text-red-400 font-bold'
      if (isWarn) cls = 'text-orange-400'

      return (
        <div key={i} className={cls}>
          {line || '\u00a0'}
        </div>
      )
    })
  }

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-2 bg-gray-900 border border-gray-700 rounded-t-lg border-b-0">
        <span className="text-xs text-gray-400 font-mono">Provisioning Log</span>
        <div className="flex items-center gap-2">
          <div className={`w-2 h-2 rounded-full ${done ? (doneStatus === 'stopped' ? 'bg-green-400' : 'bg-red-400') : (isConnected ? 'bg-yellow-400 animate-pulse' : 'bg-gray-500')}`} />
          <span className="text-xs text-gray-500 font-mono">
            {done ? (doneStatus === 'stopped' ? 'Complete' : `Error (${doneStatus})`) : (isConnected ? 'Running...' : 'Connecting...')}
          </span>
        </div>
      </div>

      {/* Log area */}
      <div
        ref={scrollRef}
        className="flex-1 overflow-y-auto bg-black border border-gray-700 rounded-b-lg p-4 font-mono text-sm leading-5"
        style={{ minHeight: '300px' }}
      >
        {renderLog(lines)}
        {!done && isConnected && (
          <div className="text-green-400 animate-pulse">_</div>
        )}
      </div>

      {/* Done banner */}
      {done && (
        <div className={`mt-3 px-4 py-2 rounded-lg text-sm font-medium text-center ${
          doneStatus === 'stopped'
            ? 'bg-green-900/40 text-green-300 border border-green-700'
            : 'bg-red-900/40 text-red-300 border border-red-700'
        }`}>
          {doneStatus === 'stopped'
            ? 'Provisioning completed successfully'
            : `Provisioning failed (status: ${doneStatus})`}
        </div>
      )}
    </div>
  )
}
