import { useEffect, useRef } from 'react'
import { Terminal as XTerm } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import '@xterm/xterm/css/xterm.css'
import { useWebSocket } from '../hooks/useWebSocket'
import { useI18n } from '../hooks/useI18n'

const darkTheme = { background: '#000000', foreground: '#d1d5db', cursor: '#60a5fa', selectionBackground: '#374151' }
const lightTheme = { background: '#ffffff', foreground: '#1f2937', cursor: '#2563eb', selectionBackground: '#bfdbfe' }

function isDarkMode() {
  return document.documentElement.classList.contains('dark')
}

interface TerminalProps {
  agentId: string
  className?: string
}

export default function Terminal({ agentId, className = '' }: TerminalProps) {
  const { t } = useI18n()
  const termRef = useRef<HTMLDivElement>(null)
  const xtermRef = useRef<XTerm | null>(null)
  const fitAddonRef = useRef<FitAddon | null>(null)
  const sendRef = useRef<(data: string) => void>(() => {})
  const sendBinaryRef = useRef<(data: Uint8Array) => void>(() => {})
  const wasConnectedRef = useRef(false)

  const wsUrl = `/ws/agents/${agentId}/terminal`

  const { isConnected, send, sendBinary, disconnect } = useWebSocket(wsUrl, {
    onBinaryMessage: (data) => xtermRef.current?.write(data),
    onMessage: (data) => {
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
      wasConnectedRef.current = true
      if (fitAddonRef.current && xtermRef.current) {
        fitAddonRef.current.fit()
        const term = xtermRef.current
        sendRef.current(JSON.stringify({ type: 'resize', cols: term.cols, rows: term.rows }))
      }
    },
    onClose: () => {
      // Only show "Disconnected" if we were actually connected before
      if (wasConnectedRef.current) {
        xtermRef.current?.write(`\r\n\x1b[31m${t.agentDetail.disconnected}\x1b[0m\r\n`)
        wasConnectedRef.current = false
      }
    },
  })

  // Keep refs in sync with latest send functions
  sendRef.current = send
  sendBinaryRef.current = sendBinary

  // Create xterm once on mount, dispose on unmount
  useEffect(() => {
    if (!termRef.current) return

    const term = new XTerm({
      theme: isDarkMode() ? darkTheme : lightTheme,
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

    // Terminal input -> WS binary (via ref to avoid dependency)
    term.onData((data) => {
      sendBinaryRef.current(new TextEncoder().encode(data))
    })

    // Resize events (via ref)
    term.onResize(({ cols, rows }) => {
      sendRef.current(JSON.stringify({ type: 'resize', cols, rows }))
    })

    const handleResize = () => {
      fitAddon.fit()
    }
    globalThis.addEventListener('resize', handleResize)

    // Watch for dark/light mode changes
    const mqDark = globalThis.matchMedia?.('(prefers-color-scheme: dark)')
    const handleThemeChange = () => {
      term.options.theme = isDarkMode() ? darkTheme : lightTheme
    }
    mqDark?.addEventListener('change', handleThemeChange)

    // Also observe the <html> class for manual dark mode toggle
    const observer = new MutationObserver(handleThemeChange)
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ['class'] })

    return () => {
      globalThis.removeEventListener('resize', handleResize)
      mqDark?.removeEventListener('change', handleThemeChange)
      observer.disconnect()
      term.dispose()
      xtermRef.current = null
    }
  }, []) // eslint-disable-line react-hooks/exhaustive-deps

  // Disconnect WS on unmount
  useEffect(() => {
    return () => disconnect()
  }, [disconnect])

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
