import { useEffect, useRef, useCallback } from 'react'
import { Terminal as XTerm } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import '@xterm/xterm/css/xterm.css'
import { useWebSocket } from '../hooks/useWebSocket'
import { useI18n } from '../hooks/useI18n'

interface TerminalProps {
  agentId: string
  className?: string
}

export default function Terminal({ agentId, className = '' }: TerminalProps) {
  const { t } = useI18n()
  const termRef = useRef<HTMLDivElement>(null)
  const xtermRef = useRef<XTerm | null>(null)
  const fitAddonRef = useRef<FitAddon | null>(null)

  const wsUrl = `/ws/agents/${agentId}/terminal`

  const sendResize = useCallback((cols: number, rows: number, sendFn: (data: string) => void) => {
    sendFn(JSON.stringify({ type: 'resize', cols, rows }))
  }, [])

  const { isConnected, send, sendBinary } = useWebSocket(wsUrl, {
    onBinaryMessage: (data) => xtermRef.current?.write(data),
    onMessage: (data) => {
      // Handle JSON error messages from server
      try {
        const msg = JSON.parse(data)
        if (msg.type === 'error') {
          xtermRef.current?.write(`\r\n\x1b[31m[Error] ${msg.message}\x1b[0m\r\n`)
          return
        }
      } catch {
        // Not JSON, treat as raw text
      }
      xtermRef.current?.write(data)
    },
    onOpen: () => {
      // Send initial resize after connect
      if (fitAddonRef.current) {
        fitAddonRef.current.fit()
        const term = xtermRef.current
        if (term) {
          sendResize(term.cols, term.rows, send)
        }
      }
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
    })

    const fitAddon = new FitAddon()
    term.loadAddon(fitAddon)
    term.open(termRef.current)
    fitAddon.fit()

    xtermRef.current = term
    fitAddonRef.current = fitAddon

    // Terminal input -> WS binary
    term.onData((data) => {
      sendBinary(new TextEncoder().encode(data))
    })

    // Resize events
    term.onResize(({ cols, rows }) => {
      send(JSON.stringify({ type: 'resize', cols, rows }))
    })

    const handleResize = () => {
      fitAddon.fit()
    }
    globalThis.addEventListener('resize', handleResize)

    return () => {
      globalThis.removeEventListener('resize', handleResize)
      term.dispose()
      xtermRef.current = null
    }
  }, [send, sendBinary])

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
