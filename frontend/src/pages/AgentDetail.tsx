import React, { useState, useEffect, useRef } from 'react'
import { useParams, useNavigate, useSearchParams } from 'react-router-dom'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { ArrowLeft, Bot, Server, GitBranch, Copy, Send, Terminal as TerminalIcon, Tag, X, StopCircle } from 'lucide-react'
import { agentsApi, serversApi, tasksApi, workItemsApi, notificationsApi, type TaskSummary } from '../lib/api'
import { ChevronLeft, ChevronRight } from 'lucide-react'
import { useI18n } from '../hooks/useI18n'
import Terminal from '../components/Terminal'
import TaskLogViewer from '../components/TaskLogViewer'
import ProvisionLog from '../components/ProvisionLog'
import { canDispatchTask } from '../lib/agentRuntime'
import { isAdmin as hasAdminRole } from '../lib/auth'
import { copyToClipboard } from '../lib/clipboard'

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
  const [terminalMounted, setTerminalMounted] = useState(() => searchParams.get('tab') === 'terminal')
  const [showTaskModal, setShowTaskModal] = useState(false)
  const [taskTitleInput, setTaskTitleInput] = useState('')
  const [taskInput, setTaskInput] = useState('')
  const [copyToast, setCopyToast] = useState(false)
  const [expandedTaskId, setExpandedTaskId] = useState<string | null>(() => searchParams.get('task'))
  const [taskPage, setTaskPage] = useState(1)
  const [selectedNotifIds, setSelectedNotifIds] = useState<string[]>([])
  const [resumeConfirm, setResumeConfirm] = useState<{ threadId: string; command: string; title: string } | null>(null)
  const [closeConfirm, setCloseConfirm] = useState<string | null>(null)
  const taskPerPage = 20
  const hasResumeTabs = resumeTabs.length > 0
  const resumeTabsRef = useRef(resumeTabs)
  resumeTabsRef.current = resumeTabs
  const [pendingNav, setPendingNav] = useState<string | null>(null)
  const isAdmin = hasAdminRole()

  // Warn on browser tab close / refresh when resume tabs are open
  useEffect(() => {
    if (!hasResumeTabs) return
    const handler = (e: BeforeUnloadEvent) => { e.preventDefault() }
    globalThis.addEventListener('beforeunload', handler)
    return () => globalThis.removeEventListener('beforeunload', handler)
  }, [hasResumeTabs])

  // Guarded navigate: shows confirmation if resume tabs are open
  function guardedNavigate(to: string) {
    if (resumeTabsRef.current.length > 0) {
      setPendingNav(to)
    } else {
      navigate(to)
    }
  }

  const { data: agent, isLoading, refetch: refetchAgent } = useQuery({
    queryKey: ['agent', id],
    queryFn: () => agentsApi.get(id!),
    enabled: !!id,
  })

  // Auto-select provision tab when agent is provisioning or provision failed
  useEffect(() => {
    if (agent?.status === 'provisioning' || agent?.status === 'provision_failed') {
      setActiveTab('provision')
    }
  }, [agent?.status])

  const { data: servers = [] } = useQuery({
    queryKey: ['servers'],
    queryFn: serversApi.list,
    enabled: isAdmin,
    retry: false,
  })
  const visibleServers = isAdmin ? servers : []
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
      await copyToClipboard(cmd)
      setCopyToast(true)
      setTimeout(() => setCopyToast(false), 2000)
    } catch (e) {
      console.error('Failed to copy terminal command', e)
    }
  }

  function handleProvisionDone(status: string) {
    refetchAgent()
    // Invalidate agents list so returning to list shows correct status
    qc.invalidateQueries({ queryKey: ['agents'] })
    if (status === 'stopped' || status === 'running') {
      setTerminalMounted(true)
      setActiveTab('terminal')
    }
  }

  if (isLoading) return <div className="p-8 text-gray-500">{t.common.loading}</div>
  if (!agent) return <div className="p-8 text-gray-500">{t.agentDetail.notFound}</div>

  const isProvisioning = agent.status === 'provisioning'
  const provisionFailed = agent.status === 'provision_failed'
  const server = visibleServers.find(s => s.id === agent.server_id)
  const statusMap: Record<string, string> = { running: 'badge-green', stopped: 'badge-gray', error: 'badge-red', provisioning: 'badge-yellow', provision_failed: 'badge-red' }
  const statusLabel = t.status[agent.status as keyof typeof t.status] ?? agent.status

  const fixedTabs = (isProvisioning || provisionFailed)
    ? [{ key: 'provision', label: t.provision?.title ?? 'Provision' }]
    : [
        { key: 'tasks', label: t.agentDetail.tasks },
        { key: 'terminal', label: t.agentDetail.terminal },
      ]

  async function handleResumeRequest(threadId: string, command: string, title: string) {
    // Part 1: Reuse existing tab
    const existing = resumeTabs.find(rt => rt.id === threadId)
    if (existing) {
      setActiveTab(`resume-${threadId}`)
      return
    }
    // Part 2: Check for running process, warn if found
    try {
      const { running } = await agentsApi.checkResumeProcess(id!, threadId)
      if (running) {
        setResumeConfirm({ threadId, command, title })
        return
      }
    } catch (e) {
      console.error('Failed to check resume process', e)
    }
    addResumeTab(threadId, command, title)
  }

  function addResumeTab(threadId: string, command: string, title: string) {
    const label = title.length > 16 ? title.slice(0, 16) + '…' : title
    setResumeTabs(prev => {
      if (prev.find(rt => rt.id === threadId)) return prev
      return [...prev, { id: threadId, label, command }]
    })
    setActiveTab(`resume-${threadId}`)
  }

  function closeResumeTab(tabId: string) {
    setCloseConfirm(tabId)
  }

  function confirmCloseResumeTab() {
    if (!closeConfirm) return
    setResumeTabs(prev => prev.filter(rt => rt.id !== closeConfirm))
    if (activeTab === `resume-${closeConfirm}`) setActiveTab('tasks')
    setCloseConfirm(null)
  }

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="border-b border-gray-100 dark:border-gray-800 px-8 py-5">
        <div className="flex items-center gap-4 mb-4">
          <button onClick={() => guardedNavigate('/agents')} className="text-gray-400 hover:text-gray-700 dark:hover:text-gray-300 transition-colors">
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
                <span className={agent.cli_type === 'codex' ? 'badge badge-indigo' : 'badge badge-blue'}>{agent.cli_type}</span>
                <span className={agent.use_docker ? 'badge badge-blue' : 'badge badge-gray'}>
                  {agent.use_docker ? t.agents.dockerBadge : t.agents.noDockerBadge}
                </span>
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
          </div>
        </div>
      </div>

      {/* Tabs */}
      <div className="border-b border-gray-100 dark:border-gray-800 px-8">
        <div className="flex gap-1">
          {fixedTabs.map((tab, idx) => (
            <React.Fragment key={tab.key}>
              <button onClick={() => { setActiveTab(tab.key); if (tab.key === 'terminal') setTerminalMounted(true) }}
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
        {/* Terminals: stay mounted once activated, hidden when inactive to keep WS alive */}
        {terminalMounted && (
          <div className={activeTab === 'terminal' ? 'h-full' : 'hidden'}>
            <Terminal agentId={id!} className="h-full" />
          </div>
        )}
        {resumeTabs.map(rt => (
          <div key={`resume-${rt.id}`} className={activeTab === `resume-${rt.id}` ? 'h-full' : 'hidden'}>
            <Terminal agentId={id!} className="h-full" initialCommand={rt.command} resumeThreadId={rt.id} />
          </div>
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
                      onOpenResumeTerminal={(threadId, cmd, title) => handleResumeRequest(threadId, cmd, title)}
                      onAbort={async () => {
                        try {
                          await tasksApi.abort(task.id)
                          qc.invalidateQueries({ queryKey: ['tasks', id] })
                        } catch (e) {
                          console.error('Failed to abort task', e)
                        }
                      }}
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
          <ProvisionLog agentId={id!} onDone={isProvisioning ? handleProvisionDone : undefined} />
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
              {notifConfigs.length > 0 && (
                <div>
                  <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.notifications.selectNotifications}</label>
                  <div className="space-y-1.5">
                    {notifConfigs.map(n => (
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
                        <span className={`text-sm ${n.enabled ? 'text-gray-700 dark:text-gray-300' : 'text-gray-400 dark:text-gray-500'}`}>
                          {n.name}
                          {!n.enabled && <span className="ml-1 text-xs">({t.common.disabled})</span>}
                        </span>
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

      {/* Resume process warning dialog */}
      {resumeConfirm && (
        <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50 p-4">
          <div className="bg-white border border-gray-200 rounded-xl w-full max-w-md dark:bg-gray-900 dark:border-gray-700">
            <div className="px-6 py-4 border-b border-gray-200 dark:border-gray-700">
              <h3 className="font-semibold text-gray-800 dark:text-gray-100">{t.agents.resumeProcessRunning}</h3>
            </div>
            <div className="p-6 space-y-4">
              <p className="text-sm text-gray-600 dark:text-gray-400">{t.agents.resumeProcessRunningDesc}</p>
              <div className="flex gap-3 justify-end">
                <button onClick={() => setResumeConfirm(null)} className="btn-secondary">{t.common.cancel}</button>
                <button
                  onClick={() => {
                    addResumeTab(resumeConfirm.threadId, resumeConfirm.command, resumeConfirm.title)
                    setResumeConfirm(null)
                  }}
                  className="btn-primary"
                >{t.agents.continueAnyway}</button>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Close resume tab confirmation dialog */}
      {closeConfirm && (
        <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50 p-4">
          <div className="bg-white border border-gray-200 rounded-xl w-full max-w-md dark:bg-gray-900 dark:border-gray-700">
            <div className="px-6 py-4 border-b border-gray-200 dark:border-gray-700">
              <h3 className="font-semibold text-gray-800 dark:text-gray-100">{t.agents.closeResumeTab}</h3>
            </div>
            <div className="p-6 space-y-4">
              <p className="text-sm text-gray-600 dark:text-gray-400">{t.agents.closeResumeTabDesc}</p>
              <div className="flex gap-3 justify-end">
                <button onClick={() => setCloseConfirm(null)} className="btn-secondary">{t.common.cancel}</button>
                <button onClick={confirmCloseResumeTab} className="btn-danger">{t.agents.closeAndKill}</button>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Navigation blocker dialog when resume tabs are open */}
      {pendingNav && (
        <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50 p-4">
          <div className="bg-white border border-gray-200 rounded-xl w-full max-w-md dark:bg-gray-900 dark:border-gray-700">
            <div className="px-6 py-4 border-b border-gray-200 dark:border-gray-700">
              <h3 className="font-semibold text-gray-800 dark:text-gray-100">{t.agents.leavePageTitle}</h3>
            </div>
            <div className="p-6 space-y-4">
              <p className="text-sm text-gray-600 dark:text-gray-400">{t.agents.leavePageDesc}</p>
              <div className="flex gap-3 justify-end">
                <button onClick={() => setPendingNav(null)} className="btn-secondary">{t.common.cancel}</button>
                <button onClick={() => { setPendingNav(null); navigate(pendingNav) }} className="btn-danger">{t.agents.leaveAndKill}</button>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

function TaskCard({ task, agentId, t, expanded, onToggle, onApprove, onReject, onOpenResumeTerminal, onAbort }: {
  task: TaskSummary
  agentId: string
  t: ReturnType<typeof useI18n>['t']
  expanded: boolean
  onToggle: () => void
  onApprove?: () => void
  onReject?: () => void
  onOpenResumeTerminal?: (threadId: string, command: string, title: string) => void
  onAbort?: () => void
}) {
  const [resumeCopied, setResumeCopied] = useState(false)
  const [aborting, setAborting] = useState(false)
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
      await copyToClipboard(ssh_cmd ?? local_cmd)
      setResumeCopied(true)
      setTimeout(() => setResumeCopied(false), 2000)
    } catch (err) {
      console.error('Failed to copy resume command', err)
    }
  }

  async function handleOpenResumeTerminal(e: React.MouseEvent) {
    e.stopPropagation()
    if (task.thread_id && onOpenResumeTerminal) {
      try {
        const { terminal_input_cmd, local_cmd } = await agentsApi.getResumeCommand(agentId, task.thread_id)
        onOpenResumeTerminal(task.thread_id, terminal_input_cmd ?? local_cmd, task.title)
      } catch {
        onOpenResumeTerminal(task.thread_id, `codex resume ${task.thread_id}`, task.title)
      }
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
          {task.status === 'agent_in_progress' && onAbort && (
            <button
              onClick={(e) => {
                e.stopPropagation()
                if (!aborting) {
                  setAborting(true)
                  onAbort()
                }
              }}
              disabled={aborting}
              className="px-2 py-1 text-xs font-medium rounded border border-red-300 text-red-600 hover:bg-red-50 dark:border-red-700 dark:text-red-400 dark:hover:bg-red-900/20 flex items-center gap-1"
              title={t.agents.abortTask ?? 'Abort'}
            >
              <StopCircle size={12} />
              {aborting ? (t.common.loading ?? '...') : (t.agents.abortTask ?? 'Abort')}
            </button>
          )}
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
