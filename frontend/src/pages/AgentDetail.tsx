import React, { useState, useEffect } from 'react'
import { useParams, useNavigate, useSearchParams } from 'react-router-dom'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Play, Square, RotateCcw, Trash2, ArrowLeft, Bot, Server, GitBranch, Copy, Send, Terminal as TerminalIcon, Tag, X } from 'lucide-react'
import { agentsApi, serversApi, tasksApi, workItemsApi, notificationsApi, type TaskSummary } from '../lib/api'
import { ChevronLeft, ChevronRight } from 'lucide-react'
import { useI18n } from '../hooks/useI18n'
import Terminal from '../components/Terminal'
import TaskLogViewer from '../components/TaskLogViewer'
import ProvisionLog from '../components/ProvisionLog'
import DeleteAgentDialog from '../components/DeleteAgentDialog'
import { canDispatchTask, getAgentRuntimeAction, type AgentRuntimeAction } from '../lib/agentRuntime'

interface ResumeTab {
  id: string
  label: string
  command: string
}

export default function AgentDetail() {
  const { id } = useParams<{ id: string }>()
  const navigate = useNavigate()
  const [searchParams] = useSearchParams()
  const qc = useQueryClient()
  const { t } = useI18n()
  const [activeTab, setActiveTab] = useState<string>(() => {
    const tab = searchParams.get('tab')
    return (tab === 'terminal' || tab === 'tasks' || tab === 'provision') ? tab : 'tasks'
  })
  const [resumeTabs, setResumeTabs] = useState<ResumeTab[]>([])
  const [showTaskModal, setShowTaskModal] = useState(false)
  const [taskTitleInput, setTaskTitleInput] = useState('')
  const [taskInput, setTaskInput] = useState('')
  const [copyToast, setCopyToast] = useState(false)
  const [expandedTaskId, setExpandedTaskId] = useState<string | null>(() => searchParams.get('task'))
  const [taskPage, setTaskPage] = useState(1)
  const [showDeleteDialog, setShowDeleteDialog] = useState(false)
  const [selectedNotifIds, setSelectedNotifIds] = useState<string[]>([])
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
  const { data: notifConfigs = [] } = useQuery({ queryKey: ['notifications'], queryFn: notificationsApi.list })
  const { data: tasksData } = useQuery({
    queryKey: ['tasks', id, taskPage],
    queryFn: () => tasksApi.list(id!, taskPage, taskPerPage),
    enabled: !!id,
    refetchInterval: 3000,
  })
  const tasks = tasksData?.items ?? []
  const totalTasks = tasksData?.total ?? 0
  const totalPages = Math.ceil(totalTasks / taskPerPage)

  const reviewMutation = useMutation({
    mutationFn: ({ workItemId, status }: { workItemId: string; status: string }) =>
      workItemsApi.update(workItemId, { status }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['tasks', id] })
      qc.invalidateQueries({ queryKey: ['agents'] })
    },
  })

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
    mutationFn: ({ title, description, notification_ids }: { title: string; description: string; notification_ids?: string[] }) =>
      tasksApi.create(id!, title, description, notification_ids),
    onSuccess: (task) => {
      setTaskPage(1)
      qc.invalidateQueries({ queryKey: ['tasks', id] })
      setTaskTitleInput('')
      setTaskInput('')
      setShowTaskModal(false)
      setSelectedNotifIds([])
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

  const fixedTabs = [
    { key: 'tasks', label: t.agentDetail.tasks },
    { key: 'terminal', label: t.agentDetail.terminal },
    { key: 'provision', label: t.provision?.title ?? 'Provision' },
  ]

  function addResumeTab(threadId: string, command: string) {
    const existing = resumeTabs.find(rt => rt.id === threadId)
    if (existing) {
      setActiveTab(`resume-${threadId}`)
      return
    }
    const shortId = threadId.length > 8 ? threadId.slice(-8) : threadId
    setResumeTabs(prev => [...prev, { id: threadId, label: `Resume ${shortId}`, command }])
    setActiveTab(`resume-${threadId}`)
  }

  function closeResumeTab(tabId: string) {
    setResumeTabs(prev => prev.filter(rt => rt.id !== tabId))
    if (activeTab === `resume-${tabId}`) setActiveTab('tasks')
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
                {agent.is_busy && <span className="badge-yellow">{t.requirements.busy}</span>}
              </div>
              <div className="flex items-center gap-4 text-xs text-gray-500 mt-0.5">
                <span className="flex items-center gap-1"><Server size={11} />{server?.name ?? agent.server_id}</span>
                <span className="flex items-center gap-1"><GitBranch size={11} />{agent.git_branch}</span>
                <span>{agent.docker_container_name}</span>
              </div>
            </div>
          </div>

          <div className="flex items-center gap-2">
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
          {fixedTabs.map((tab, idx) => (
            <React.Fragment key={tab.key}>
              <button onClick={() => setActiveTab(tab.key)}
                className={`px-4 py-2.5 text-sm font-medium transition-colors ${activeTab === tab.key ? 'text-sky-500 border-b-2 border-sky-500' : 'text-gray-500 hover:text-gray-700 dark:hover:text-gray-300'}`}
              >
                {tab.label}
                {tab.key === 'tasks' && totalTasks > 0 && (
                  <span className="ml-1.5 px-1.5 py-0.5 bg-gray-200 dark:bg-gray-700 rounded text-xs">{totalTasks}</span>
                )}
              </button>
              {/* Insert resume tabs after the "tasks" tab */}
              {idx === 0 && resumeTabs.map(rt => {
                const tabKey = `resume-${rt.id}`
                return (
                  <button key={tabKey} onClick={() => setActiveTab(tabKey)}
                    className={`group px-3 py-2.5 text-sm font-medium transition-colors flex items-center gap-1.5 ${activeTab === tabKey ? 'text-sky-500 border-b-2 border-sky-500' : 'text-gray-500 hover:text-gray-700 dark:hover:text-gray-300'}`}
                  >
                    <TerminalIcon size={12} />
                    {rt.label}
                    <span
                      onClick={(e) => { e.stopPropagation(); closeResumeTab(rt.id) }}
                      className="ml-0.5 p-0.5 rounded hover:bg-gray-200 dark:hover:bg-gray-700 opacity-0 group-hover:opacity-100 transition-opacity"
                    >
                      <X size={11} />
                    </span>
                  </button>
                )
              })}
            </React.Fragment>
          ))}
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-auto p-8">
        {activeTab === 'terminal' && <Terminal agentId={id!} className="h-full" />}
        {resumeTabs.map(rt => (
          activeTab === `resume-${rt.id}` && (
            <Terminal key={`resume-${rt.id}`} agentId={id!} className="h-full" initialCommand={rt.command} />
          )
        ))}
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
                      agentId={id!}
                      t={t}
                      expanded={expandedTaskId === task.id}
                      onToggle={() => setExpandedTaskId(expandedTaskId === task.id ? null : task.id)}
                      onApprove={() => {
                        if (task.work_item_id) reviewMutation.mutate({ workItemId: task.work_item_id, status: 'human_approved' })
                      }}
                      onReject={() => {
                        if (task.work_item_id) reviewMutation.mutate({ workItemId: task.work_item_id, status: 'human_rejected' })
                      }}
                      onOpenResumeTerminal={(threadId, cmd) => addResumeTab(threadId, cmd)}
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
              <button onClick={() => { setShowTaskModal(false); setSelectedNotifIds([]) }} className="text-gray-400 hover:text-gray-700 dark:hover:text-gray-300">✕</button>
            </div>
            <div className="p-6 space-y-4">
              <div>
                <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1">{t.agents.dispatchTaskTitle}</label>
                <input
                  className="input w-full"
                  value={taskTitleInput}
                  onChange={e => setTaskTitleInput(e.target.value)}
                  placeholder={t.agents.dispatchTaskTitle}
                  autoFocus
                />
              </div>
              <div>
                <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1">{t.agents.dispatchTaskDesc}</label>
                <textarea
                  className="input w-full"
                  rows={5}
                  value={taskInput}
                  onChange={e => setTaskInput(e.target.value)}
                  placeholder={t.agentDetail.taskPlaceholder(agent.cli_type)}
                />
              </div>
              {/* Notification configs */}
              {notifConfigs.filter(n => n.enabled).length > 0 && (
                <div>
                  <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.notifications.selectNotifications}</label>
                  <div className="space-y-1.5">
                    {notifConfigs.filter(n => n.enabled).map(n => (
                      <label key={n.id} className="flex items-center gap-2 cursor-pointer">
                        <input
                          type="checkbox"
                          checked={selectedNotifIds.includes(n.id)}
                          onChange={e => {
                            if (e.target.checked) setSelectedNotifIds(prev => [...prev, n.id])
                            else setSelectedNotifIds(prev => prev.filter(x => x !== n.id))
                          }}
                          className="w-4 h-4 rounded"
                        />
                        <span className="text-sm text-gray-700 dark:text-gray-300">{n.name}</span>
                      </label>
                    ))}
                  </div>
                </div>
              )}
              {createTaskMutation.error && (
                <div className="text-red-500 text-sm mt-2">{String(createTaskMutation.error.message)}</div>
              )}
              <div className="flex gap-3 justify-end pt-2">
                <button onClick={() => { setShowTaskModal(false); setSelectedNotifIds([]) }} className="btn-secondary">{t.common.cancel}</button>
                <button
                  onClick={() => { if (taskInput.trim()) createTaskMutation.mutate({ title: taskTitleInput.trim(), description: taskInput.trim(), notification_ids: selectedNotifIds.length > 0 ? selectedNotifIds : undefined }) }}
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

function TaskCard({ task, agentId, t, expanded, onToggle, onApprove, onReject, onOpenResumeTerminal }: {
  task: TaskSummary
  agentId: string
  t: ReturnType<typeof useI18n>['t']
  expanded: boolean
  onToggle: () => void
  onApprove?: () => void
  onReject?: () => void
  onOpenResumeTerminal?: (threadId: string, command: string) => void
}) {
  const [resumeCopied, setResumeCopied] = useState(false)
  const statusMap: Record<string, string> = {
    waiting: 'badge-gray', agent_in_progress: 'badge-yellow', agent_completed: 'badge-green',
    agent_failed: 'badge-red', human_approved: 'badge-green', human_rejected: 'badge-red',
    cancelled: 'badge-gray', closed: 'badge-gray',
  }
  const isFromWorkItem = !!task.work_item_id
  const showReviewActions = isFromWorkItem && task.status === 'agent_completed'
  const agentDone = ['agent_completed', 'agent_failed', 'human_approved', 'human_rejected'].includes(task.status)
  const canResume = agentDone && !!task.thread_id

  async function handleCopyResumeCommand(e: React.MouseEvent) {
    e.stopPropagation()
    try {
      const { ssh_cmd, local_cmd } = await agentsApi.getResumeCommand(agentId, task.thread_id!)
      await navigator.clipboard.writeText(ssh_cmd ?? local_cmd)
      setResumeCopied(true)
      setTimeout(() => setResumeCopied(false), 2000)
    } catch (err) {
      console.error('Failed to copy resume command', err)
    }
  }

  function handleOpenResumeTerminal(e: React.MouseEvent) {
    e.stopPropagation()
    if (task.thread_id && onOpenResumeTerminal) {
      onOpenResumeTerminal(task.thread_id, `codex resume ${task.thread_id}`)
    }
  }

  return (
    <div className="card cursor-pointer" onClick={onToggle}>
      <div className="flex items-start justify-between gap-4">
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <p className="text-sm font-medium text-gray-700 dark:text-gray-200 break-words">
              {task.title}
            </p>
            {isFromWorkItem && (
              <span className="inline-flex items-center gap-1 px-1.5 py-0.5 text-[10px] font-medium rounded bg-sky-100 text-sky-600 dark:bg-sky-900/30 dark:text-sky-400 flex-shrink-0">
                <Tag size={9} />
                {t.agents.fromWorkItem}
              </span>
            )}
          </div>
          <p className="text-xs text-gray-500 mt-1">{new Date(task.created_at).toLocaleString()}</p>
          {task.thread_id && (
            <p className="text-xs text-gray-400 mt-0.5 font-mono">thread: {task.thread_id}</p>
          )}
        </div>
        <div className="flex items-center gap-2 flex-shrink-0">
          {canResume && (
            <>
              <button
                onClick={handleCopyResumeCommand}
                className="px-2 py-1 text-xs font-medium rounded border border-gray-300 text-gray-600 hover:bg-gray-50 dark:border-gray-600 dark:text-gray-400 dark:hover:bg-gray-800"
                title={t.agents.copyResumeCommand}
              >
                {resumeCopied ? t.agents.copied : <Copy size={12} />}
              </button>
              <button
                onClick={handleOpenResumeTerminal}
                className="px-2 py-1 text-xs font-medium rounded border border-gray-300 text-gray-600 hover:bg-gray-50 dark:border-gray-600 dark:text-gray-400 dark:hover:bg-gray-800"
                title={t.agents.openResumeTerminal}
              >
                <TerminalIcon size={12} />
              </button>
            </>
          )}
          {showReviewActions && (
            <>
              <button
                onClick={(e) => { e.stopPropagation(); onApprove?.() }}
                className="px-2 py-1 text-xs font-medium rounded border border-green-300 text-green-600 hover:bg-green-50 dark:border-green-700 dark:text-green-400 dark:hover:bg-green-900/20"
              >
                {t.requirements.actionApprove}
              </button>
              <button
                onClick={(e) => { e.stopPropagation(); onReject?.() }}
                className="px-2 py-1 text-xs font-medium rounded border border-red-300 text-red-600 hover:bg-red-50 dark:border-red-700 dark:text-red-400 dark:hover:bg-red-900/20"
              >
                {t.requirements.actionReject}
              </button>
            </>
          )}
          <span className={statusMap[task.status] ?? 'badge-gray'}>{t.status[task.status as keyof typeof t.status] ?? task.status}</span>
        </div>
      </div>
    </div>
  )
}
