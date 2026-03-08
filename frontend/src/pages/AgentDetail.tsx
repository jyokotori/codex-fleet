import { useState, useEffect } from 'react'
import { useParams, useNavigate, useSearchParams } from 'react-router-dom'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Play, Square, RotateCcw, Trash2, ArrowLeft, Bot, Server, GitBranch, Terminal as TerminalIcon, Copy, Send, FileText } from 'lucide-react'
import { agentsApi, serversApi, tasksApi, type TaskSummary } from '../lib/api'
import { ChevronLeft, ChevronRight } from 'lucide-react'
import { useI18n } from '../hooks/useI18n'
import Terminal from '../components/Terminal'
import TaskLogViewer from '../components/TaskLogViewer'
import ProvisionLog from '../components/ProvisionLog'
import DeleteAgentDialog from '../components/DeleteAgentDialog'
import { canDispatchTask, getAgentRuntimeAction, type AgentRuntimeAction } from '../lib/agentRuntime'

type TabType = 'terminal' | 'tasks' | 'provision'

export default function AgentDetail() {
  const { id } = useParams<{ id: string }>()
  const navigate = useNavigate()
  const [searchParams] = useSearchParams()
  const qc = useQueryClient()
  const { t } = useI18n()
  const [activeTab, setActiveTab] = useState<TabType>(() => {
    const tab = searchParams.get('tab')
    return (tab === 'terminal' || tab === 'tasks' || tab === 'provision') ? tab : 'tasks'
  })
  const [showTaskModal, setShowTaskModal] = useState(false)
  const [taskInput, setTaskInput] = useState('')
  const [copyToast, setCopyToast] = useState(false)
  const [expandedTaskId, setExpandedTaskId] = useState<string | null>(() => searchParams.get('task'))
  const [taskPage, setTaskPage] = useState(1)
  const [showDeleteDialog, setShowDeleteDialog] = useState(false)
  const taskPerPage = 20

  const { data: agent, isLoading, refetch: refetchAgent } = useQuery({
    queryKey: ['agents', id],
    queryFn: () => agentsApi.list().then(agents => agents.find(a => a.id === id)),
    enabled: !!id,
    refetchInterval: 5000,
  })

  // Auto-select provision tab when agent is provisioning
  useEffect(() => {
    if (agent?.status === 'provisioning') {
      setActiveTab('provision')
    }
  }, [agent?.status])

  const { data: servers = [] } = useQuery({ queryKey: ['servers'], queryFn: serversApi.list })
  const { data: tasksData } = useQuery({
    queryKey: ['tasks', id, taskPage],
    queryFn: () => tasksApi.list(id!, taskPage, taskPerPage),
    enabled: !!id,
    refetchInterval: 3000,
  })
  const tasks = tasksData?.items ?? []
  const totalTasks = tasksData?.total ?? 0
  const totalPages = Math.ceil(totalTasks / taskPerPage)

  const runtimeMutation = useMutation({
    mutationFn: (action: AgentRuntimeAction) => {
      if (action === 'start') return agentsApi.start(id!)
      if (action === 'pause') return agentsApi.stop(id!)
      return agentsApi.restart(id!)
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['agents'] }),
  })
  const deleteMutation = useMutation({
    mutationFn: () => agentsApi.delete(id!),
    onSuccess: () => navigate('/agents'),
  })

  const createTaskMutation = useMutation({
    mutationFn: (description: string) => tasksApi.create(id!, description),
    onSuccess: (task) => {
      setTaskPage(1)
      qc.invalidateQueries({ queryKey: ['tasks', id] })
      setTaskInput('')
      setShowTaskModal(false)
      setExpandedTaskId(task.id)
      setActiveTab('tasks')
    },
  })

  async function handleCopyTerminalCommand() {
    try {
      const { local_cmd, ssh_cmd } = await agentsApi.getTerminalCommand(id!)
      const cmd = ssh_cmd ?? local_cmd
      await navigator.clipboard.writeText(cmd)
      setCopyToast(true)
      setTimeout(() => setCopyToast(false), 2000)
    } catch (e) {
      console.error('Failed to copy terminal command', e)
    }
  }

  function handleProvisionDone(status: string) {
    refetchAgent()
    if (status === 'stopped') {
      setActiveTab('terminal')
    }
  }

  if (isLoading) return <div className="p-8 text-gray-500">{t.common.loading}</div>
  if (!agent) return <div className="p-8 text-gray-500">{t.agentDetail.notFound}</div>

  const isProvisioning = agent.status === 'provisioning'
  const server = servers.find(s => s.id === agent.server_id)
  const statusMap: Record<string, string> = { running: 'badge-green', stopped: 'badge-gray', error: 'badge-red', provisioning: 'badge-yellow' }
  const statusLabel = t.status[agent.status as keyof typeof t.status] ?? agent.status

  const tabLabels: Record<TabType, string> = {
    terminal: t.agentDetail.terminal,
    tasks: t.agentDetail.tasks,
    provision: t.provision?.title ?? 'Provision',
  }

  const runtimeAction = getAgentRuntimeAction(agent)
  const RuntimeIcon = runtimeAction === 'start' ? Play : runtimeAction === 'pause' ? Square : RotateCcw
  const runtimeLabel = runtimeAction === 'start'
    ? t.agents.start
    : runtimeAction === 'pause'
      ? t.agents.pause
      : t.agents.restart

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="border-b border-gray-100 dark:border-gray-800 px-8 py-5">
        <div className="flex items-center gap-4 mb-4">
          <button onClick={() => navigate('/agents')} className="text-gray-400 hover:text-gray-700 dark:hover:text-gray-300 transition-colors">
            <ArrowLeft size={18} />
          </button>
          <div className="flex items-center gap-3 flex-1">
            <div className="w-9 h-9 rounded-lg bg-purple-600/20 flex items-center justify-center">
              <Bot size={18} className="text-purple-400" />
            </div>
            <div>
              <div className="flex items-center gap-2">
                <h1 className="font-bold text-gray-900 dark:text-white">{agent.name}</h1>
                <span className={statusMap[agent.status] ?? 'badge-gray'}>{statusLabel}</span>
                <span className="badge badge-blue">{agent.cli_type}</span>
              </div>
              <div className="flex items-center gap-4 text-xs text-gray-500 mt-0.5">
                <span className="flex items-center gap-1"><Server size={11} />{server?.name ?? agent.server_id}</span>
                <span className="flex items-center gap-1"><GitBranch size={11} />{agent.git_branch}</span>
                <span>{agent.docker_container_name}</span>
              </div>
            </div>
          </div>

          <div className="flex items-center gap-2">
            {/* Copy terminal command */}
            <div className="relative">
              <button
                onClick={handleCopyTerminalCommand}
                className="btn-secondary btn-sm flex items-center gap-1"
                title={t.agents.copyTerminalCommand}
              >
                <Copy size={13} />
                {copyToast ? t.agents.copied : t.agents.copyCommand}
              </button>
            </div>

            {/* Open terminal (placeholder) */}
            <button
              className="btn-secondary btn-sm flex items-center gap-1"
              title={t.agents.openTerminal}
              onClick={handleCopyTerminalCommand}
            >
              <TerminalIcon size={13} />
            </button>

            {/* Dispatch task */}
            {canDispatchTask(agent) && (
              <button
                onClick={() => setShowTaskModal(true)}
                className="btn-primary btn-sm flex items-center gap-1"
              >
                <Send size={13} />
                {t.agents.dispatchTask}
              </button>
            )}

            {runtimeAction && (
              <button
                onClick={() => runtimeMutation.mutate(runtimeAction)}
                className="btn-secondary btn-sm flex items-center gap-1"
                disabled={runtimeMutation.isPending}
              >
                <RuntimeIcon size={13} />
                {runtimeLabel}
              </button>
            )}
            <button onClick={() => setShowDeleteDialog(true)} className="btn-danger btn-sm">
              <Trash2 size={13} />
            </button>
          </div>
        </div>
      </div>

      {/* Tabs */}
      <div className="border-b border-gray-100 dark:border-gray-800 px-8">
        <div className="flex gap-1">
          {(['tasks', 'terminal', 'provision'] as TabType[]).map(tab => (
            <button key={tab} onClick={() => setActiveTab(tab)}
              className={`px-4 py-2.5 text-sm font-medium transition-colors ${activeTab === tab ? 'text-sky-500 border-b-2 border-sky-500' : 'text-gray-500 hover:text-gray-700 dark:hover:text-gray-300'}`}
            >
              {tabLabels[tab]}
              {tab === 'tasks' && totalTasks > 0 && (
                <span className="ml-1.5 px-1.5 py-0.5 bg-gray-200 dark:bg-gray-700 rounded text-xs">{totalTasks}</span>
              )}
            </button>
          ))}
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-auto p-8">
        {activeTab === 'terminal' && <Terminal agentId={id!} className="h-full" />}
        {activeTab === 'tasks' && (
          <div className="space-y-3">
            {tasks.length === 0 ? (
              <div className="text-center py-12 card">
                <p className="text-gray-500">{t.agentDetail.noTasks}</p>
                {canDispatchTask(agent) && <p className="text-gray-400 dark:text-gray-600 text-sm mt-1">{t.agentDetail.noTasksHint}</p>}
              </div>
            ) : (
              <>
                {tasks.map(task => (
                  <div key={task.id}>
                    <TaskCard
                      task={task}
                      t={t}
                      expanded={expandedTaskId === task.id}
                      onToggle={() => setExpandedTaskId(expandedTaskId === task.id ? null : task.id)}
                    />
                    {expandedTaskId === task.id && (
                      <div className="mt-2 ml-4">
                        <TaskLogViewer taskId={task.id} />
                      </div>
                    )}
                  </div>
                ))}
                {totalPages > 1 && (
                  <div className="flex items-center justify-center gap-2 pt-4">
                    <button
                      onClick={() => setTaskPage(p => Math.max(1, p - 1))}
                      disabled={taskPage <= 1}
                      className="btn-secondary btn-sm flex items-center gap-1"
                    >
                      <ChevronLeft size={14} />
                    </button>
                    <span className="text-sm text-gray-500">
                      {taskPage} / {totalPages}
                    </span>
                    <button
                      onClick={() => setTaskPage(p => Math.min(totalPages, p + 1))}
                      disabled={taskPage >= totalPages}
                      className="btn-secondary btn-sm flex items-center gap-1"
                    >
                      <ChevronRight size={14} />
                    </button>
                  </div>
                )}
              </>
            )}
          </div>
        )}
        {activeTab === 'provision' && (
          isProvisioning
            ? <ProvisionLog agentId={id!} onDone={handleProvisionDone} />
            : <ProvisionHistory agent={agent} t={t} />
        )}
      </div>

      {/* Dispatch task modal */}
      {showTaskModal && (
        <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50 p-4">
          <div className="bg-white border border-gray-200 rounded-xl w-full max-w-lg dark:bg-gray-900 dark:border-gray-700">
            <div className="flex items-center justify-between px-6 py-4 border-b border-gray-200 dark:border-gray-700">
              <h3 className="font-semibold text-gray-800 dark:text-gray-100">{t.agents.dispatchTask}</h3>
              <button onClick={() => setShowTaskModal(false)} className="text-gray-400 hover:text-gray-700 dark:hover:text-gray-300">✕</button>
            </div>
            <div className="p-6">
              <label className="block text-sm text-gray-600 dark:text-gray-400 mb-2">{t.agents.dispatchTaskDesc}</label>
              <textarea
                className="input w-full"
                rows={5}
                value={taskInput}
                onChange={e => setTaskInput(e.target.value)}
                placeholder={t.agentDetail.taskPlaceholder(agent.cli_type)}
                autoFocus
              />
              {createTaskMutation.error && (
                <div className="text-red-500 text-sm mt-2">{String(createTaskMutation.error.message)}</div>
              )}
              <div className="flex gap-3 justify-end pt-4">
                <button onClick={() => setShowTaskModal(false)} className="btn-secondary">{t.common.cancel}</button>
                <button
                  onClick={() => { if (taskInput.trim()) createTaskMutation.mutate(taskInput.trim()) }}
                  className="btn-primary flex items-center gap-2"
                  disabled={createTaskMutation.isPending || !taskInput.trim()}
                >
                  <Send size={14} />{t.common.send}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}

      <DeleteAgentDialog
        agent={agent}
        open={showDeleteDialog}
        pending={deleteMutation.isPending}
        onClose={() => setShowDeleteDialog(false)}
        onConfirm={(_agent) => deleteMutation.mutate()}
      />
    </div>
  )
}

function ProvisionHistory({ agent, t }: {
  agent: { provision_steps: Record<string, string>; provision_log: string }
  t: ReturnType<typeof useI18n>['t']
}) {
  const [showRawLog, setShowRawLog] = useState(false)
  const steps = Object.entries(agent.provision_steps ?? {}).sort(([a], [b]) => Number(a) - Number(b))
  const stepName = (n: number) => t.provision?.steps?.[n] ?? `Step ${n}`

  const statusIcon = (status: string) => {
    if (status === 'ok') return <span className="text-green-400">✓</span>
    if (status === 'failed') return <span className="text-red-400">✗</span>
    if (status === 'skipped') return <span className="text-gray-500">–</span>
    return <span className="text-gray-500">○</span>
  }

  return (
    <div className="space-y-4">
      {steps.length === 0 ? (
        <div className="text-center py-12 card">
          <p className="text-gray-500">{t.agentDetail.provisionHistoryEmpty}</p>
        </div>
      ) : (
        <div className="rounded-lg border border-gray-200 dark:border-gray-700 overflow-hidden">
          {steps.map(([num, status], idx) => (
            <div
              key={num}
              className={[
                'flex items-center gap-3 px-4 py-2.5 text-sm',
                idx < steps.length - 1 && 'border-b border-gray-100 dark:border-gray-800',
                status === 'ok' ? 'bg-green-50/40 dark:bg-green-900/10' :
                status === 'failed' ? 'bg-red-50/40 dark:bg-red-900/10' :
                'bg-white dark:bg-gray-900',
              ].filter(Boolean).join(' ')}
            >
              <div className="w-4 h-4 flex items-center justify-center flex-shrink-0">
                {statusIcon(status)}
              </div>
              <span className="flex-1 font-mono text-gray-700 dark:text-gray-300">
                <span className="text-gray-400 dark:text-gray-600 mr-1">{num}.</span>
                {stepName(Number(num))}
              </span>
              <span className={`text-xs font-mono ${
                status === 'ok' ? 'text-green-600 dark:text-green-400' :
                status === 'failed' ? 'text-red-600 dark:text-red-400' :
                'text-gray-500'
              }`}>{status}</span>
            </div>
          ))}
        </div>
      )}

      {agent.provision_log && (
        <div>
          <button
            onClick={() => setShowRawLog(v => !v)}
            className="text-xs text-gray-500 hover:text-gray-700 dark:hover:text-gray-400 font-mono"
          >
            {showRawLog ? `▼ ${t.agentDetail.hideRawLog}` : `▶ ${t.agentDetail.showRawLog}`}
          </button>
          {showRawLog && (
            <pre className="mt-2 p-4 rounded-lg bg-black text-gray-300 text-xs font-mono overflow-auto max-h-96 border border-gray-700">
              {agent.provision_log}
            </pre>
          )}
        </div>
      )}
    </div>
  )
}

function TaskCard({ task, t, expanded, onToggle }: {
  task: TaskSummary
  t: ReturnType<typeof useI18n>['t']
  expanded: boolean
  onToggle: () => void
}) {
  const statusMap: Record<string, string> = {
    pending: 'badge-gray', running: 'badge-yellow', completed: 'badge-green',
    failed: 'badge-red', cancelled: 'badge-gray',
  }
  return (
    <div className="card cursor-pointer" onClick={onToggle}>
      <div className="flex items-start justify-between gap-4">
        <div className="flex-1 min-w-0">
          <p className="text-sm text-gray-700 dark:text-gray-200 break-words">{task.description}</p>
          <p className="text-xs text-gray-500 mt-1">{new Date(task.created_at).toLocaleString()}</p>
          {task.task_dir && (
            <p className="text-xs text-gray-400 mt-0.5 font-mono">{task.task_dir}</p>
          )}
          {task.thread_id && (
            <p className="text-xs text-gray-400 mt-0.5 font-mono">thread: {task.thread_id}</p>
          )}
        </div>
        <div className="flex items-center gap-2 flex-shrink-0">
          <span className={statusMap[task.status] ?? 'badge-gray'}>{t.status[task.status as keyof typeof t.status] ?? task.status}</span>
          <button
            onClick={(e) => { e.stopPropagation(); onToggle() }}
            className="btn-secondary btn-sm flex items-center gap-1"
            title={expanded ? t.agentDetail.hideLogs : t.agentDetail.showLogs}
          >
            <FileText size={12} />
          </button>
        </div>
      </div>
    </div>
  )
}
