import { useEffect, useRef, useState } from 'react'
import { Terminal as XTerm } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import '@xterm/xterm/css/xterm.css'
import { useWebSocket } from '../hooks/useWebSocket'
import { useI18n } from '../hooks/useI18n'

interface ProvisionLogProps {
  agentId: string
  onDone?: (status: string) => void
}

interface StepState {
  stepNum: number
  status: 'pending' | 'running' | 'ok' | 'skipped' | 'failed'
  error?: string
}

const TOTAL_STEPS = 4

function makeSteps(): StepState[] {
  return Array.from({ length: TOTAL_STEPS }, (_, i) => ({
    stepNum: i + 1,
    status: 'pending' as const,
  }))
}

export default function ProvisionLog({ agentId, onDone }: ProvisionLogProps) {
  const { t } = useI18n()
  const [steps, setSteps] = useState<StepState[]>(makeSteps())
  const [done, setDone] = useState(false)
  const [doneStatus, setDoneStatus] = useState('')
  const [runningStep, setRunningStep] = useState<number | null>(null)

  const termRef = useRef<HTMLDivElement>(null)
  const xtermRef = useRef<XTerm | null>(null)
  const fitAddonRef = useRef<FitAddon | null>(null)
  const doneRef = useRef(false)

  // xterm setup
  useEffect(() => {
    if (!termRef.current) return

    const term = new XTerm({
      theme: {
        background: '#000000',
        foreground: '#d1d5db',
        cursor: '#60a5fa',
        selectionBackground: '#374151',
      },
      fontFamily: 'ui-monospace, monospace',
      fontSize: 13,
      disableStdin: true,
      convertEol: true,
    })

    const fitAddon = new FitAddon()
    term.loadAddon(fitAddon)
    term.open(termRef.current)
    fitAddon.fit()

    xtermRef.current = term
    fitAddonRef.current = fitAddon

    const handleResize = () => fitAddon.fit()
    globalThis.addEventListener('resize', handleResize)

    return () => {
      globalThis.removeEventListener('resize', handleResize)
      term.dispose()
      xtermRef.current = null
    }
  }, [])

  const writeToTerm = (text: string, color?: string) => {
    if (!xtermRef.current) return
    const colored = color ? `${color}${text}\x1b[0m\r\n` : `${text}\r\n`
    xtermRef.current.write(colored)
  }

  const updateStep = (stepNum: number, patch: Partial<StepState>) => {
    setSteps(prev =>
      prev.map(s => {
        if (s.stepNum !== stepNum) return s
        // Don't regress from a terminal state back to running (anti-regression for reconnects)
        const terminalStates: StepState['status'][] = ['ok', 'failed', 'skipped']
        if (terminalStates.includes(s.status) && patch.status === 'running') return s
        return { ...s, ...patch }
      })
    )
  }

  const { isConnected } = useWebSocket(`/ws/agents/${agentId}/provision`, {
    maxReconnects: 5,
    onMessage: (data) => {
      if (doneRef.current) return
      let ev: Record<string, unknown>
      try {
        ev = JSON.parse(data)
      } catch {
        // Fallback: show raw text in terminal
        writeToTerm(data)
        return
      }

      const type = ev.t as string
      const step = ev.step as number | undefined
      const text = (ev.text ?? ev.error ?? ev.reason ?? '') as string

      switch (type) {
        case 'provision_init': {
          // Restore step states from DB on connect/reconnect — prevents stale UI after page refresh
          const stepsMap = (ev.steps ?? {}) as Record<string, string>
          setSteps(prev => prev.map(s => ({
            ...s,
            status: (stepsMap[String(s.stepNum)] ?? 'pending') as StepState['status'],
          })))
          const runningEntry = Object.entries(stepsMap).find(([, v]) => v === 'running')
          setRunningStep(runningEntry ? parseInt(runningEntry[0]) : null)
          break
        }

        case 'step_start':
          if (step != null) {
            updateStep(step, { status: 'running' })
            setRunningStep(step)
          }
          break

        case 'step_output':
          writeToTerm(text)
          break

        case 'warn':
          writeToTerm(text, '\x1b[33m')
          break

        case 'step_done':
          if (step != null) {
            updateStep(step, { status: 'ok' })
            if (runningStep === step) setRunningStep(null)
          }
          break

        case 'step_skipped':
          if (step != null) {
            updateStep(step, { status: 'skipped' })
            if (runningStep === step) setRunningStep(null)
          }
          break

        case 'step_failed':
          if (step != null) {
            updateStep(step, { status: 'failed', error: text })
            if (runningStep === step) setRunningStep(null)
            writeToTerm(text, '\x1b[31m')
          }
          break

        case 'provision_done': {
          const finalStatus = (ev.status as string) ?? 'unknown'
          doneRef.current = true
          setDone(true)
          setDoneStatus(finalStatus)
          onDone?.(finalStatus)
          break
        }

        default:
          if (text) writeToTerm(text)
      }
    },
  })

  const stepName = (n: number) => t.provision.steps[n] ?? `Step ${n}`

  const StepIcon = ({ status }: { status: StepState['status'] }) => {
    if (status === 'ok') return <span className="text-green-400 text-base leading-none">✓</span>
    if (status === 'failed') return <span className="text-red-400 text-base leading-none">✗</span>
    if (status === 'skipped') return <span className="text-gray-500 text-base leading-none">–</span>
    if (status === 'running') return (
      <svg className="w-4 h-4 text-yellow-400 animate-spin" viewBox="0 0 24 24" fill="none">
        <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
        <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8v8z" />
      </svg>
    )
    return <span className="text-gray-600 text-base leading-none">○</span>
  }

  const doneOk = doneStatus === 'stopped'
  const completedCount = steps.filter(s => s.status === 'ok').length

  return (
    <div className="flex flex-col gap-3">
      {/* Header */}
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold text-gray-700 dark:text-gray-200">
          {t.provision.title}
        </h3>
        {!done && isConnected && runningStep != null && (
          <span className="text-xs text-yellow-500 font-mono">
            {completedCount}/{TOTAL_STEPS} {t.provision.status.running}
          </span>
        )}
      </div>

      {/* Step status area */}
      <div className="rounded-lg border border-gray-200 dark:border-gray-700 overflow-hidden">
        {steps.map((step, idx) => {
          const isLast = idx === steps.length - 1
          return (
            <div
              key={step.stepNum}
              className={[
                'flex items-center gap-3 px-4 py-2.5 text-sm',
                !isLast && 'border-b border-gray-100 dark:border-gray-800',
                step.status === 'running'
                  ? 'bg-yellow-50 dark:bg-yellow-900/10'
                  : step.status === 'ok'
                  ? 'bg-green-50/40 dark:bg-green-900/10'
                  : step.status === 'failed'
                  ? 'bg-red-50/40 dark:bg-red-900/10'
                  : 'bg-white dark:bg-gray-900',
              ].filter(Boolean).join(' ')}
            >
              <div className="w-4 h-4 flex items-center justify-center flex-shrink-0">
                <StepIcon status={step.status} />
              </div>
              <span
                className={[
                  'flex-1 font-mono',
                  step.status === 'pending' ? 'text-gray-400 dark:text-gray-600' : '',
                  step.status === 'running' ? 'text-yellow-700 dark:text-yellow-300 font-medium' : '',
                  step.status === 'ok' ? 'text-green-700 dark:text-green-400' : '',
                  step.status === 'failed' ? 'text-red-600 dark:text-red-400' : '',
                  step.status === 'skipped' ? 'text-gray-400 dark:text-gray-600 line-through' : '',
                ].filter(Boolean).join(' ')}
              >
                <span className="text-gray-400 dark:text-gray-600 mr-1">{step.stepNum}.</span>
                {stepName(step.stepNum)}
                {step.status === 'skipped' && (
                  <span className="ml-2 text-xs no-underline" style={{ textDecoration: 'none' }}>
                    ({t.provision.status.skipped})
                  </span>
                )}
              </span>
            </div>
          )
        })}
      </div>

      {/* xterm output area */}
      <div className="rounded-lg overflow-hidden border border-gray-200 dark:border-gray-700">
        <div className="flex items-center justify-between px-3 py-1.5 bg-gray-100 dark:bg-gray-800 border-b border-gray-200 dark:border-gray-700">
          <span className="text-xs text-gray-500 dark:text-gray-400 font-mono">output</span>
          <div className="flex items-center gap-1.5">
            <div className={`w-1.5 h-1.5 rounded-full ${
              done
                ? doneOk ? 'bg-green-400' : 'bg-red-400'
                : isConnected ? 'bg-yellow-400 animate-pulse' : 'bg-gray-400'
            }`} />
            <span className="text-xs text-gray-400 font-mono">
              {done
                ? (doneOk ? 'complete' : 'error')
                : (isConnected ? 'running' : 'connecting')}
            </span>
          </div>
        </div>
        <div ref={termRef} style={{ minHeight: '220px' }} />
      </div>

      {/* Done banner */}
      {done && (
        <div className={`px-4 py-3 rounded-lg text-sm font-medium text-center border ${
          doneOk
            ? 'bg-green-900/20 text-green-300 border-green-700'
            : 'bg-red-900/20 text-red-300 border-red-700'
        }`}>
          {doneOk ? t.provision.status.complete : t.provision.status.failed}
        </div>
      )}
    </div>
  )
}
