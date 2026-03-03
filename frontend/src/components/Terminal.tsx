import { useEffect, useRef } from 'react'
import { Terminal as XTerm } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import '@xterm/xterm/css/xterm.css'
import { useWebSocket } from '../hooks/useWebSocket'
import { useI18n } from '../hooks/useI18n'

interface TerminalProps {
  agentId: string
  tmuxWindow?: string
  className?: string
}

export default function Terminal({ agentId, tmuxWindow, className = '' }: TerminalProps) {
  const { t } = useI18n()
  const termRef = useRef<HTMLDivElement>(null)
  const xtermRef = useRef<XTerm | null>(null)
  const fitAddonRef = useRef<FitAddon | null>(null)

  const wsUrl = tmuxWindow
    ? `/ws/agents/${agentId}/terminal?window=${encodeURIComponent(tmuxWindow)}`
    : `/ws/agents/${agentId}/terminal`

  const { isConnected, send } = useWebSocket(wsUrl, {
    onMessage: (data) => xtermRef.current?.write(data),
    onOpen: () => {
      xtermRef.current?.clear()
      xtermRef.current?.write(`\x1b[32m${t.agentDetail.terminalConnected}\x1b[0m\r\n`)
    },
    onClose: () => xtermRef.current?.write(`\r\n\x1b[31m${t.agentDetail.disconnected}\x1b[0m\r\n`),
  })

  useEffect(() => {
    if (!termRef.current) return

    const term = new XTerm({
      theme: { background: '#000000', foreground: '#d1d5db', cursor: '#60a5fa', selectionBackground: '#374151' },
      fontFamily: 'ui-monospace, monospace',
      fontSize: 13,
      cursorBlink: true,
      convertEol: true,
    })

    const fitAddon = new FitAddon()
    term.loadAddon(fitAddon)
    term.open(termRef.current)
    fitAddon.fit()

    xtermRef.current = term
    fitAddonRef.current = fitAddon

    term.onData((data) => send(data))

    const handleResize = () => fitAddon.fit()
    globalThis.addEventListener('resize', handleResize)

    return () => {
      globalThis.removeEventListener('resize', handleResize)
      term.dispose()
      xtermRef.current = null
    }
  }, [send])

  return (
    <div className={`flex flex-col ${className}`}>
      <div className="flex items-center justify-between px-3 py-2 bg-gray-100 dark:bg-gray-800 rounded-t-lg border border-gray-200 dark:border-gray-700 border-b-0">
        <span className="text-xs text-gray-600 dark:text-gray-400">{t.agentDetail.interactiveTerminal}</span>
        <div className="flex items-center gap-2">
          <div className={`w-2 h-2 rounded-full ${isConnected ? 'bg-green-400' : 'bg-red-400'}`} />
          <span className="text-xs text-gray-500">{isConnected ? t.agentDetail.connected : t.agentDetail.disconnected}</span>
        </div>
      </div>
      <div ref={termRef} className="flex-1 rounded-b-lg border border-gray-200 dark:border-gray-700 overflow-hidden" style={{ minHeight: '300px' }} />
    </div>
  )
}
