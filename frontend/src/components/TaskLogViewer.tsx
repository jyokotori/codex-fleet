import { useEffect, useRef, useState, useCallback, useMemo } from 'react'
import {
  Copy, CheckCircle, XCircle, Loader, ChevronDown, ChevronRight,
  Terminal, FileText, MessageSquare, Brain, Globe, Wrench, Zap,
} from 'lucide-react'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { useWebSocket } from '../hooks/useWebSocket'
import { copyToClipboard } from '../lib/clipboard'

/* ── types ─────────────────────────────────────────────────────── */

interface BaseItem {
  id: string
  type: string
}

interface AgentMessageItem extends BaseItem {
  type: 'agent_message'
  text: string
}

interface CommandExecutionItem extends BaseItem {
  type: 'command_execution'
  command: string
  cwd?: string
  aggregated_output?: string
  exit_code?: number | null
  status: string
  durationMs?: number
}

interface FileChangeItem extends BaseItem {
  type: 'file_change'
  changes?: { path: string; kind: string; diff?: string }[]
  status: string
}

interface ReasoningItem extends BaseItem {
  type: 'reasoning'
  summary?: { text: string }[]
  content?: string
}

interface McpToolCallItem extends BaseItem {
  type: 'mcp_tool_call'
  server?: string
  tool?: string
  status: string
  arguments?: string
  result?: string
  error?: string
}

interface WebSearchItem extends BaseItem {
  type: 'web_search'
  query?: string
  action?: unknown
}

type ParsedItem =
  | AgentMessageItem
  | CommandExecutionItem
  | FileChangeItem
  | ReasoningItem
  | McpToolCallItem
  | WebSearchItem
  | BaseItem

interface TurnUsage {
  input_tokens?: number
  cached_input_tokens?: number
  output_tokens?: number
}

/* ── props ─────────────────────────────────────────────────────── */

interface TaskLogViewerProps {
  taskId: string
  className?: string
}

/* ── main component ────────────────────────────────────────────── */

export default function TaskLogViewer({ taskId, className = '' }: TaskLogViewerProps) {
  const [items, setItems] = useState<ParsedItem[]>([])
  const [threadId, setThreadId] = useState<string | null>(null)
  const [taskStatus, setTaskStatus] = useState<string>('agent_in_progress')
  const [usage, setUsage] = useState<TurnUsage | null>(null)
  const [copied, setCopied] = useState(false)
  const [autoScroll, setAutoScroll] = useState(true)
  const containerRef = useRef<HTMLDivElement>(null)
  const itemMapRef = useRef(new Map<string, ParsedItem>())
  const itemOrderRef = useRef<string[]>([])

  const flushItems = useCallback(() => {
    const ordered = itemOrderRef.current.map(id => itemMapRef.current.get(id)!).filter(Boolean)
    setItems(ordered)
  }, [])

  const { isConnected } = useWebSocket(`/ws/tasks/${taskId}/logs`, {
    onMessage: (data: string) => {
      // The WS can send multiple lines at once (replay)
      const lines = data.split('\n')
      let changed = false

      for (const line of lines) {
        const trimmed = line.trim()
        if (!trimmed) continue

        let evt: Record<string, unknown>
        try {
          evt = JSON.parse(trimmed)
        } catch {
          continue
        }

        const type = evt.type as string | undefined
        if (!type) continue

        // Control message from our backend
        if (type === 'task_done') {
          setTaskStatus(evt.status as string)
          continue
        }

        if (type === 'thread.started') {
          setThreadId((evt.thread_id as string) ?? null)
          continue
        }

        if (type === 'turn.completed') {
          const u = evt.usage as TurnUsage | undefined
          if (u) setUsage(u)
          continue
        }

        // Lifecycle events with items
        if ((type === 'item.started' || type === 'item.completed') && evt.item) {
          const item = evt.item as ParsedItem
          if (!item.id) continue

          const existing = itemMapRef.current.get(item.id)
          if (existing) {
            // Merge: item.completed overwrites item.started
            itemMapRef.current.set(item.id, { ...existing, ...item })
          } else {
            itemMapRef.current.set(item.id, item)
            itemOrderRef.current.push(item.id)
          }
          changed = true
        }
      }

      if (changed) flushItems()
    },
    onOpen: () => {
      itemMapRef.current.clear()
      itemOrderRef.current = []
      setItems([])
      setThreadId(null)
      setUsage(null)
    },
  })

  useEffect(() => {
    if (autoScroll && containerRef.current) {
      containerRef.current.scrollTop = containerRef.current.scrollHeight
    }
  }, [items, autoScroll])

  async function handleCopyThreadId() {
    if (!threadId) return
    await copyToClipboard(threadId)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  const statusIcon = (taskStatus === 'agent_completed' || taskStatus === 'human_approved')
    ? <CheckCircle size={14} className="text-green-400" />
    : (taskStatus === 'agent_failed' || taskStatus === 'human_rejected')
      ? <XCircle size={14} className="text-red-400" />
      : <Loader size={14} className="text-yellow-400 animate-spin" />

  const statusColor = (taskStatus === 'agent_completed' || taskStatus === 'human_approved') ? 'badge-green'
    : (taskStatus === 'agent_failed' || taskStatus === 'human_rejected') ? 'badge-red'
      : 'badge-yellow'

  // Filter out non-renderable lifecycle events (turn.started etc. are not items)
  const renderableItems = useMemo(() =>
    items.filter(i => ['agent_message', 'command_execution', 'file_change', 'reasoning', 'mcp_tool_call', 'web_search'].includes(i.type)),
    [items],
  )

  return (
    <div className={`flex flex-col gap-3 ${className}`}>
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          {statusIcon}
          <span className={statusColor}>{taskStatus}</span>
          {usage && (
            <span className="text-[11px] text-gray-400 font-mono ml-2">
              tokens: {((usage.input_tokens ?? 0) + (usage.output_tokens ?? 0)).toLocaleString()}
              {usage.cached_input_tokens ? ` (${usage.cached_input_tokens.toLocaleString()} cached)` : ''}
            </span>
          )}
        </div>
        <div className="flex items-center gap-2">
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
          <label className="flex items-center gap-1.5 text-xs text-gray-500 cursor-pointer select-none">
            <input type="checkbox" checked={autoScroll} onChange={e => setAutoScroll(e.target.checked)} className="w-3 h-3" />
            Auto-scroll
          </label>
          <div className={`w-2 h-2 rounded-full ${isConnected ? 'bg-green-400' : 'bg-gray-400'}`} />
        </div>
      </div>

      {/* Items */}
      <div
        ref={containerRef}
        className="overflow-auto rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-900"
        style={{ minHeight: '300px', maxHeight: '600px' }}
      >
        {renderableItems.length === 0 ? (
          <div className="flex items-center justify-center h-48 text-sm text-gray-400">
            {isConnected ? 'Waiting for output...' : 'Connecting...'}
          </div>
        ) : (
          <div className="divide-y divide-gray-100 dark:divide-gray-800">
            {renderableItems.map(item => (
              <ItemRenderer key={item.id} item={item} />
            ))}
          </div>
        )}
      </div>
    </div>
  )
}

/* ── item renderer ─────────────────────────────────────────────── */

function ItemRenderer({ item }: { item: ParsedItem }) {
  switch (item.type) {
    case 'agent_message':
      return <AgentMessageBlock item={item as AgentMessageItem} />
    case 'command_execution':
      return <CommandBlock item={item as CommandExecutionItem} />
    case 'file_change':
      return <FileChangeBlock item={item as FileChangeItem} />
    case 'reasoning':
      return <ReasoningBlock item={item as ReasoningItem} />
    case 'mcp_tool_call':
      return <McpToolCallBlock item={item as McpToolCallItem} />
    case 'web_search':
      return <WebSearchBlock item={item as WebSearchItem} />
    default:
      return <GenericBlock item={item} />
  }
}

/* ── agent message ─────────────────────────────────────────────── */

function AgentMessageBlock({ item }: { item: AgentMessageItem }) {
  return (
    <div className="px-4 py-3">
      <div className="flex items-start gap-2.5">
        <div className="mt-0.5 flex-shrink-0 w-6 h-6 rounded-full bg-purple-100 dark:bg-purple-900/30 flex items-center justify-center">
          <MessageSquare size={13} className="text-purple-500" />
        </div>
        <div className="min-w-0 flex-1 prose prose-sm dark:prose-invert max-w-none
          prose-p:my-1 prose-pre:my-2 prose-ul:my-1 prose-ol:my-1
          prose-code:text-xs prose-code:bg-gray-100 prose-code:dark:bg-gray-800 prose-code:px-1 prose-code:py-0.5 prose-code:rounded
          prose-pre:bg-gray-50 prose-pre:dark:bg-gray-800 prose-pre:text-xs prose-pre:rounded-lg prose-pre:p-3
          prose-a:text-sky-500 prose-a:no-underline hover:prose-a:underline
          text-gray-700 dark:text-gray-200 text-sm leading-relaxed">
          <ReactMarkdown remarkPlugins={[remarkGfm]}>{item.text || ''}</ReactMarkdown>
        </div>
      </div>
    </div>
  )
}

/* ── command execution ─────────────────────────────────────────── */

function CommandBlock({ item }: { item: CommandExecutionItem }) {
  const [expanded, setExpanded] = useState(false)
  const isRunning = item.status === 'in_progress'
  const failed = item.exit_code != null && item.exit_code !== 0
  const hasOutput = !!item.aggregated_output?.trim()

  // Extract the actual command from "/bin/bash -lc ..." wrapper
  const displayCommand = useMemo(() => {
    const cmd = item.command ?? ''
    const match = cmd.match(/^\/bin\/(?:ba)?sh\s+-\w*c\s+(.+)$/)
    return match ? match[1] : cmd
  }, [item.command])

  return (
    <div className="px-4 py-2.5">
      <button
        onClick={() => hasOutput && setExpanded(e => !e)}
        className="flex items-center gap-2 w-full text-left group"
      >
        <div className="flex-shrink-0 w-6 h-6 rounded-full bg-gray-100 dark:bg-gray-800 flex items-center justify-center">
          <Terminal size={13} className="text-gray-500" />
        </div>
        <code className="flex-1 text-xs font-mono text-gray-600 dark:text-gray-300 truncate">
          {displayCommand}
        </code>
        <div className="flex items-center gap-1.5 flex-shrink-0">
          {item.durationMs != null && (
            <span className="text-[10px] text-gray-400">{(item.durationMs / 1000).toFixed(1)}s</span>
          )}
          {isRunning ? (
            <Loader size={12} className="text-yellow-400 animate-spin" />
          ) : failed ? (
            <span className="text-[10px] font-mono text-red-400">exit {item.exit_code}</span>
          ) : (
            <CheckCircle size={12} className="text-green-400" />
          )}
          {hasOutput && (
            expanded
              ? <ChevronDown size={13} className="text-gray-400" />
              : <ChevronRight size={13} className="text-gray-400 group-hover:text-gray-300" />
          )}
        </div>
      </button>
      {expanded && hasOutput && (
        <pre className="mt-2 ml-8 text-[11px] font-mono text-gray-400 bg-gray-50 dark:bg-gray-800/60 rounded-md p-3 overflow-auto whitespace-pre-wrap leading-relaxed"
          style={{ maxHeight: '200px' }}>
          {item.aggregated_output}
        </pre>
      )}
    </div>
  )
}

/* ── file change ───────────────────────────────────────────────── */

function FileChangeBlock({ item }: { item: FileChangeItem }) {
  const [expanded, setExpanded] = useState(false)
  const changes = item.changes ?? []
  const isRunning = item.status === 'in_progress'

  return (
    <div className="px-4 py-2.5">
      <button
        onClick={() => changes.length > 0 && setExpanded(e => !e)}
        className="flex items-center gap-2 w-full text-left group"
      >
        <div className="flex-shrink-0 w-6 h-6 rounded-full bg-blue-100 dark:bg-blue-900/30 flex items-center justify-center">
          <FileText size={13} className="text-blue-500" />
        </div>
        <span className="flex-1 text-xs text-gray-600 dark:text-gray-300">
          {changes.length} file{changes.length !== 1 ? 's' : ''} changed
          {changes.length > 0 && (
            <span className="text-gray-400 ml-1.5 font-mono">
              {changes.map(c => c.path.split('/').pop()).join(', ')}
            </span>
          )}
        </span>
        <div className="flex items-center gap-1.5 flex-shrink-0">
          {isRunning ? (
            <Loader size={12} className="text-yellow-400 animate-spin" />
          ) : item.status === 'completed' ? (
            <CheckCircle size={12} className="text-green-400" />
          ) : item.status === 'failed' || item.status === 'declined' ? (
            <XCircle size={12} className="text-red-400" />
          ) : null}
          {changes.length > 0 && (
            expanded
              ? <ChevronDown size={13} className="text-gray-400" />
              : <ChevronRight size={13} className="text-gray-400 group-hover:text-gray-300" />
          )}
        </div>
      </button>
      {expanded && changes.length > 0 && (
        <div className="mt-2 ml-8 space-y-2">
          {changes.map((c, i) => (
            <div key={i} className="rounded-md border border-gray-200 dark:border-gray-700 overflow-hidden">
              <div className="px-3 py-1.5 bg-gray-50 dark:bg-gray-800 text-[11px] font-mono text-gray-500 flex items-center justify-between">
                <span>{c.path}</span>
                <span className="text-gray-400">{c.kind}</span>
              </div>
              {c.diff && (
                <pre className="text-[11px] font-mono p-3 overflow-auto whitespace-pre-wrap leading-relaxed bg-white dark:bg-gray-900"
                  style={{ maxHeight: '150px' }}>
                  <DiffHighlight diff={c.diff} />
                </pre>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

/* ── diff highlight ────────────────────────────────────────────── */

function DiffHighlight({ diff }: { diff: string }) {
  return (
    <>
      {diff.split('\n').map((line, i) => {
        const cls = line.startsWith('+') ? 'text-green-500 bg-green-50 dark:bg-green-900/20'
          : line.startsWith('-') ? 'text-red-500 bg-red-50 dark:bg-red-900/20'
            : line.startsWith('@@') ? 'text-blue-400'
              : 'text-gray-400'
        return <div key={i} className={cls}>{line}</div>
      })}
    </>
  )
}

/* ── reasoning ─────────────────────────────────────────────────── */

function ReasoningBlock({ item }: { item: ReasoningItem }) {
  const [expanded, setExpanded] = useState(false)
  const text = item.summary?.map(s => s.text).join('\n') ?? item.content ?? ''
  if (!text) return null

  return (
    <div className="px-4 py-2.5">
      <button
        onClick={() => setExpanded(e => !e)}
        className="flex items-center gap-2 w-full text-left group"
      >
        <div className="flex-shrink-0 w-6 h-6 rounded-full bg-amber-100 dark:bg-amber-900/30 flex items-center justify-center">
          <Brain size={13} className="text-amber-500" />
        </div>
        <span className="flex-1 text-xs text-gray-500 italic">Thinking...</span>
        {expanded
          ? <ChevronDown size={13} className="text-gray-400" />
          : <ChevronRight size={13} className="text-gray-400 group-hover:text-gray-300" />}
      </button>
      {expanded && (
        <pre className="mt-2 ml-8 text-[11px] font-mono text-gray-400 bg-gray-50 dark:bg-gray-800/60 rounded-md p-3 overflow-auto whitespace-pre-wrap leading-relaxed"
          style={{ maxHeight: '200px' }}>
          {text}
        </pre>
      )}
    </div>
  )
}

/* ── MCP tool call ─────────────────────────────────────────────── */

function McpToolCallBlock({ item }: { item: McpToolCallItem }) {
  const [expanded, setExpanded] = useState(false)
  const isRunning = item.status === 'in_progress'

  return (
    <div className="px-4 py-2.5">
      <button
        onClick={() => setExpanded(e => !e)}
        className="flex items-center gap-2 w-full text-left group"
      >
        <div className="flex-shrink-0 w-6 h-6 rounded-full bg-teal-100 dark:bg-teal-900/30 flex items-center justify-center">
          <Wrench size={13} className="text-teal-500" />
        </div>
        <span className="flex-1 text-xs text-gray-600 dark:text-gray-300 font-mono truncate">
          {item.server && <span className="text-gray-400">{item.server}/</span>}
          {item.tool}
        </span>
        <div className="flex items-center gap-1.5 flex-shrink-0">
          {isRunning ? (
            <Loader size={12} className="text-yellow-400 animate-spin" />
          ) : item.status === 'completed' ? (
            <CheckCircle size={12} className="text-green-400" />
          ) : (
            <XCircle size={12} className="text-red-400" />
          )}
          {expanded
            ? <ChevronDown size={13} className="text-gray-400" />
            : <ChevronRight size={13} className="text-gray-400 group-hover:text-gray-300" />}
        </div>
      </button>
      {expanded && (
        <div className="mt-2 ml-8 space-y-1">
          {item.arguments && (
            <pre className="text-[11px] font-mono text-gray-400 bg-gray-50 dark:bg-gray-800/60 rounded-md p-2 overflow-auto whitespace-pre-wrap"
              style={{ maxHeight: '120px' }}>
              {item.arguments}
            </pre>
          )}
          {item.result && (
            <pre className="text-[11px] font-mono text-green-400/80 bg-gray-50 dark:bg-gray-800/60 rounded-md p-2 overflow-auto whitespace-pre-wrap"
              style={{ maxHeight: '120px' }}>
              {item.result}
            </pre>
          )}
          {item.error && (
            <pre className="text-[11px] font-mono text-red-400 bg-gray-50 dark:bg-gray-800/60 rounded-md p-2 overflow-auto whitespace-pre-wrap">
              {item.error}
            </pre>
          )}
        </div>
      )}
    </div>
  )
}

/* ── web search ────────────────────────────────────────────────── */

function WebSearchBlock({ item }: { item: WebSearchItem }) {
  return (
    <div className="px-4 py-2.5 flex items-center gap-2">
      <div className="flex-shrink-0 w-6 h-6 rounded-full bg-sky-100 dark:bg-sky-900/30 flex items-center justify-center">
        <Globe size={13} className="text-sky-500" />
      </div>
      <span className="text-xs text-gray-500">
        Searching: <span className="text-gray-600 dark:text-gray-300 font-medium">{item.query}</span>
      </span>
    </div>
  )
}

/* ── generic fallback ──────────────────────────────────────────── */

function GenericBlock({ item }: { item: BaseItem }) {
  return (
    <div className="px-4 py-2 flex items-center gap-2">
      <div className="flex-shrink-0 w-6 h-6 rounded-full bg-gray-100 dark:bg-gray-800 flex items-center justify-center">
        <Zap size={13} className="text-gray-400" />
      </div>
      <span className="text-xs text-gray-400 font-mono">{item.type}</span>
    </div>
  )
}
