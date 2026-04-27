import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Plus, Trash2, ToggleLeft, ToggleRight, Plane, Pencil, ChevronDown, ChevronRight, Copy } from 'lucide-react'
import {
  planeApi,
  agentGroupsApi,
  clisApi,
  type PlaneWorkspace,
  type PlaneBinding,
  type PlaneBindingLabelInput,
  type PlaneProject,
  type PlaneTask,
} from '../lib/api'
import { useI18n } from '../hooks/useI18n'

type Tab = 'workspaces' | 'tasks'

export default function PlaneIntegration() {
  const { t } = useI18n()
  const qc = useQueryClient()
  const [tab, setTab] = useState<Tab>('workspaces')
  const [wsModal, setWsModal] = useState<{ mode: 'create' } | { mode: 'edit'; ws: PlaneWorkspace } | null>(null)
  const [bindingModal, setBindingModal] = useState<{ workspace: PlaneWorkspace; binding?: PlaneBinding } | null>(null)
  const [expanded, setExpanded] = useState<Set<string>>(new Set())

  const { data: workspaces = [] } = useQuery({ queryKey: ['plane-workspaces'], queryFn: planeApi.listWorkspaces })
  const { data: tasks = [] } = useQuery({
    queryKey: ['plane-tasks'],
    queryFn: planeApi.listTasks,
    refetchInterval: 5000,
  })

  const toggleWorkspaceMut = useMutation({
    mutationFn: (id: string) => planeApi.toggleWorkspace(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['plane-workspaces'] }),
  })

  const deleteWorkspaceMut = useMutation({
    mutationFn: (id: string) => planeApi.deleteWorkspace(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['plane-workspaces'] })
      qc.invalidateQueries({ queryKey: ['plane-bindings'] })
    },
  })

  const toggleExpanded = (id: string) => {
    setExpanded(prev => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  const statusColors: Record<string, string> = {
    pending: 'bg-yellow-100 text-yellow-800 dark:bg-yellow-900/30 dark:text-yellow-300',
    dispatched: 'bg-blue-100 text-blue-800 dark:bg-blue-900/30 dark:text-blue-300',
    completed: 'bg-green-100 text-green-800 dark:bg-green-900/30 dark:text-green-300',
    failed: 'bg-red-100 text-red-800 dark:bg-red-900/30 dark:text-red-300',
    cancelled: 'bg-gray-100 text-gray-600 dark:bg-gray-700 dark:text-gray-400',
  }

  return (
    <div className="p-6 max-w-5xl mx-auto">
      <h1 className="text-2xl font-bold flex items-center gap-2 mb-6 dark:text-white">
        <Plane size={24} /> {t.plane.title}
      </h1>

      {/* Tabs */}
      <div className="flex gap-1 mb-6 border-b dark:border-gray-700">
        <button
          onClick={() => setTab('workspaces')}
          className={`px-4 py-2 text-sm font-medium border-b-2 transition-colors ${
            tab === 'workspaces'
              ? 'border-sky-600 text-sky-600 dark:text-sky-400'
              : 'border-transparent text-gray-500 hover:text-gray-700 dark:text-gray-400'
          }`}
        >
          {t.plane.workspaces}
        </button>
        <button
          onClick={() => setTab('tasks')}
          className={`px-4 py-2 text-sm font-medium border-b-2 transition-colors ${
            tab === 'tasks'
              ? 'border-sky-600 text-sky-600 dark:text-sky-400'
              : 'border-transparent text-gray-500 hover:text-gray-700 dark:text-gray-400'
          }`}
        >
          {t.plane.taskQueue} ({tasks.length})
        </button>
      </div>

      {/* Workspaces Tab */}
      {tab === 'workspaces' && (
        <div>
          <div className="flex justify-end mb-4">
            <button
              onClick={() => setWsModal({ mode: 'create' })}
              className="flex items-center gap-2 px-4 py-2 bg-sky-600 text-white rounded-lg hover:bg-sky-700 text-sm"
            >
              <Plus size={16} /> {t.plane.addWorkspace}
            </button>
          </div>

          {workspaces.length === 0 ? (
            <p className="text-gray-500 dark:text-gray-400 text-center py-12">{t.plane.noWorkspaces}</p>
          ) : (
            <div className="space-y-3">
              {workspaces.map(ws => (
                <WorkspaceCard
                  key={ws.id}
                  workspace={ws}
                  expanded={expanded.has(ws.id)}
                  onToggleExpand={() => toggleExpanded(ws.id)}
                  onEdit={() => setWsModal({ mode: 'edit', ws })}
                  onToggle={() => toggleWorkspaceMut.mutate(ws.id)}
                  onDelete={() => {
                    if (confirm(`Delete workspace "${ws.name}" and all its bindings?`)) {
                      deleteWorkspaceMut.mutate(ws.id)
                    }
                  }}
                  onAddBinding={() => setBindingModal({ workspace: ws })}
                  onEditBinding={(b) => setBindingModal({ workspace: ws, binding: b })}
                />
              ))}
            </div>
          )}
        </div>
      )}

      {/* Tasks Tab */}
      {tab === 'tasks' && (
        <div>
          {tasks.length === 0 ? (
            <p className="text-gray-500 dark:text-gray-400 text-center py-12">{t.plane.noTasks}</p>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b dark:border-gray-700">
                    <th className="text-left py-3 px-4 font-medium text-gray-500 dark:text-gray-400">Title</th>
                    <th className="text-left py-3 px-4 font-medium text-gray-500 dark:text-gray-400">Assignee</th>
                    <th className="text-left py-3 px-4 font-medium text-gray-500 dark:text-gray-400">Status</th>
                    <th className="text-left py-3 px-4 font-medium text-gray-500 dark:text-gray-400">Time</th>
                  </tr>
                </thead>
                <tbody>
                  {tasks.map((task: PlaneTask) => (
                    <tr key={task.id} className="border-b dark:border-gray-700 hover:bg-gray-50 dark:hover:bg-gray-800/50">
                      <td className="py-3 px-4 dark:text-white font-medium">{task.title}</td>
                      <td className="py-3 px-4 dark:text-gray-300 text-xs">{task.assignee_email || '-'}</td>
                      <td className="py-3 px-4">
                        <span className={`px-2 py-1 rounded text-xs font-medium ${statusColors[task.status] || ''}`}>
                          {task.status}
                        </span>
                      </td>
                      <td className="py-3 px-4 text-xs text-gray-500 dark:text-gray-400">
                        {new Date(task.created_at).toLocaleString()}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      )}

      {wsModal && (
        <WorkspaceModal
          mode={wsModal.mode}
          workspace={wsModal.mode === 'edit' ? wsModal.ws : undefined}
          onClose={() => setWsModal(null)}
          onSaved={() => {
            qc.invalidateQueries({ queryKey: ['plane-workspaces'] })
            setWsModal(null)
          }}
        />
      )}

      {bindingModal && (
        <BindingModal
          workspace={bindingModal.workspace}
          binding={bindingModal.binding}
          onClose={() => setBindingModal(null)}
          onSaved={() => {
            qc.invalidateQueries({ queryKey: ['plane-bindings', bindingModal.workspace.id] })
            setBindingModal(null)
          }}
        />
      )}
    </div>
  )
}

// ── Workspace card with nested bindings ──

function WorkspaceCard({
  workspace,
  expanded,
  onToggleExpand,
  onEdit,
  onToggle,
  onDelete,
  onAddBinding,
  onEditBinding,
}: {
  workspace: PlaneWorkspace
  expanded: boolean
  onToggleExpand: () => void
  onEdit: () => void
  onToggle: () => void
  onDelete: () => void
  onAddBinding: () => void
  onEditBinding: (b: PlaneBinding) => void
}) {
  const { t } = useI18n()
  const qc = useQueryClient()

  const { data: bindings = [] } = useQuery({
    queryKey: ['plane-bindings', workspace.id],
    queryFn: () => planeApi.listWorkspaceBindings(workspace.id),
    enabled: expanded,
  })

  const toggleBindingMut = useMutation({
    mutationFn: (id: string) => planeApi.toggleBinding(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['plane-bindings', workspace.id] }),
  })
  const deleteBindingMut = useMutation({
    mutationFn: (id: string) => planeApi.deleteBinding(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['plane-bindings', workspace.id] }),
  })

  const webhookUrl = `${window.location.origin}/api/webhooks/plane/${workspace.id}`
  const workspaceDisplayUrl = workspace.workspace_url

  const copyWebhook = () => {
    navigator.clipboard.writeText(webhookUrl).catch(() => {})
  }

  return (
    <div className="border rounded-lg dark:border-gray-700 bg-white dark:bg-gray-800">
      <div className="flex items-center gap-3 p-4">
        <button onClick={onToggleExpand} className="text-gray-400 hover:text-gray-600">
          {expanded ? <ChevronDown size={18} /> : <ChevronRight size={18} />}
        </button>
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <span className="font-semibold dark:text-white">{workspace.name}</span>
            {!workspace.enabled && (
              <span className="text-xs px-2 py-0.5 bg-gray-200 dark:bg-gray-700 text-gray-600 dark:text-gray-400 rounded">
                {t.plane.disabled}
              </span>
            )}
          </div>
          <div className="text-xs text-gray-500 dark:text-gray-400 mt-1 truncate">
            {workspaceDisplayUrl}
          </div>
        </div>
        <button
          onClick={onToggle}
          className="inline-flex items-center gap-1"
          title={workspace.enabled ? t.plane.enabled : t.plane.disabled}
        >
          {workspace.enabled ? (
            <ToggleRight size={20} className="text-green-500" />
          ) : (
            <ToggleLeft size={20} className="text-gray-400" />
          )}
        </button>
        <button onClick={onEdit} className="p-1.5 text-gray-500 hover:text-sky-600" title="Edit">
          <Pencil size={16} />
        </button>
        <button onClick={onDelete} className="p-1.5 text-gray-500 hover:text-red-600" title="Delete">
          <Trash2 size={16} />
        </button>
      </div>

      {expanded && (
        <div className="border-t dark:border-gray-700 p-4 space-y-4 bg-gray-50 dark:bg-gray-900/30">
          {/* Metadata */}
          <div className="grid grid-cols-1 md:grid-cols-2 gap-3 text-xs">
            <KV label={t.plane.apiKey} value={workspace.api_key_masked || '-'} mono />
            <KV label={t.plane.webhookSecret} value={workspace.webhook_secret_masked || '-'} mono />
            <div className="md:col-span-2">
              <div className="text-gray-500 dark:text-gray-400 mb-1">{t.plane.webhookUrl}</div>
              <div className="flex items-center gap-2">
                <code className="flex-1 px-2 py-1 bg-white dark:bg-gray-800 border dark:border-gray-700 rounded text-xs truncate dark:text-gray-200">
                  {webhookUrl}
                </code>
                <button onClick={copyWebhook} className="p-1.5 text-gray-500 hover:text-sky-600" title="Copy">
                  <Copy size={14} />
                </button>
              </div>
            </div>
          </div>

          {/* Bindings */}
          <div>
            <div className="flex items-center justify-between mb-2">
              <h3 className="text-sm font-semibold dark:text-white">{t.plane.bindings}</h3>
              <button
                onClick={onAddBinding}
                className="flex items-center gap-1 px-3 py-1.5 text-xs bg-sky-600 text-white rounded hover:bg-sky-700"
              >
                <Plus size={14} /> {t.plane.addBinding}
              </button>
            </div>
            {bindings.length === 0 ? (
              <p className="text-sm text-gray-500 dark:text-gray-400 py-4 text-center">{t.plane.noBindings}</p>
            ) : (
              <div className="space-y-2">
                {bindings.map((b: PlaneBinding) => (
                  <div key={b.id} className="border rounded-lg dark:border-gray-700 bg-white dark:bg-gray-800 p-3">
                    <div className="flex items-start gap-3">
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2 mb-1">
                          <span className="font-medium dark:text-white">{b.plane_project_name}</span>
                          {b.plane_project_identifier && (
                            <span className="text-gray-400 text-xs">{b.plane_project_identifier}</span>
                          )}
                          <span className="text-xs px-2 py-0.5 rounded bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-300">
                            {b.agent_group_name || b.agent_group_id.slice(0, 8)}
                          </span>
                        </div>
                        <div className="text-xs text-gray-500 dark:text-gray-400 mb-1.5">
                          <span className="font-mono">{b.accept_state_name}</span>
                          <span className="mx-1.5">→</span>
                          <span className="font-mono">{b.in_progress_state_name}</span>
                          <span className="mx-1.5">→</span>
                          <span className="font-mono">{b.completion_state_name}</span>
                        </div>
                        {b.labels.length > 0 && (
                          <div className="flex flex-wrap gap-1.5">
                            {b.labels.map(lb => (
                              <span
                                key={lb.label_id}
                                className="inline-flex items-center gap-1 text-xs px-1.5 py-0.5 rounded bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-200"
                              >
                                <span className="font-medium">{lb.label_name}</span>
                                <span className="text-gray-400">→</span>
                                <span className="font-mono">{lb.cli_type}</span>
                                <span className="text-[10px] text-gray-400">p{lb.priority}</span>
                              </span>
                            ))}
                          </div>
                        )}
                      </div>
                      <div className="flex items-center gap-1 shrink-0">
                        <button onClick={() => toggleBindingMut.mutate(b.id)} title={b.enabled ? 'Disable' : 'Enable'}>
                          {b.enabled ? (
                            <ToggleRight size={20} className="text-green-500" />
                          ) : (
                            <ToggleLeft size={20} className="text-gray-400" />
                          )}
                        </button>
                        <button onClick={() => onEditBinding(b)} className="p-1 text-gray-500 hover:text-sky-600" title="Edit">
                          <Pencil size={14} />
                        </button>
                        <button
                          onClick={() => { if (confirm('Delete binding?')) deleteBindingMut.mutate(b.id) }}
                          className="p-1 text-gray-500 hover:text-red-600"
                          title="Delete"
                        >
                          <Trash2 size={14} />
                        </button>
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  )
}

function KV({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div>
      <div className="text-gray-500 dark:text-gray-400 mb-1">{label}</div>
      <div className={`dark:text-gray-200 ${mono ? 'font-mono' : ''}`}>{value}</div>
    </div>
  )
}

// ── Workspace Create/Edit Modal ──

function WorkspaceModal({
  mode,
  workspace,
  onClose,
  onSaved,
}: {
  mode: 'create' | 'edit'
  workspace?: PlaneWorkspace
  onClose: () => void
  onSaved: () => void
}) {
  const { t } = useI18n()
  const [name, setName] = useState(workspace?.name ?? '')
  const [workspaceUrl, setWorkspaceUrl] = useState(workspace?.workspace_url ?? '')
  const [apiKey, setApiKey] = useState('')
  const [webhookSecret, setWebhookSecret] = useState('')
  const [error, setError] = useState<string | null>(null)

  const mut = useMutation({
    mutationFn: async () => {
      setError(null)
      if (mode === 'create') {
        return planeApi.createWorkspace({
          name: name.trim(),
          workspace_url: workspaceUrl.trim(),
          api_key: apiKey.trim(),
          webhook_secret: webhookSecret.trim(),
        })
      } else {
        return planeApi.updateWorkspace(workspace!.id, {
          name: name.trim(),
          workspace_url: workspaceUrl.trim(),
          api_key: apiKey.trim() || undefined,
          webhook_secret: webhookSecret,
        })
      }
    },
    onSuccess: onSaved,
    onError: (e: Error) => setError(e.message),
  })

  const canSubmit = name.trim() && workspaceUrl.trim() && (mode === 'edit' || apiKey.trim())

  return (
    <div className="fixed inset-0 bg-black/40 flex items-center justify-center z-50" onClick={onClose}>
      <div className="bg-white dark:bg-gray-800 rounded-xl shadow-xl p-6 w-full max-w-md" onClick={e => e.stopPropagation()}>
        <h2 className="text-lg font-bold mb-4 dark:text-white">
          {mode === 'create' ? t.plane.addWorkspace : t.plane.editWorkspace}
        </h2>
        <div className="space-y-4">
          <Field label={t.plane.workspaceName}>
            <input
              type="text"
              value={name}
              onChange={e => setName(e.target.value)}
              placeholder="Magician"
              className="w-full px-3 py-2 border rounded-lg dark:bg-gray-700 dark:border-gray-600 dark:text-white"
            />
          </Field>
          <Field label={t.plane.workspaceUrl} hint={t.plane.workspaceUrlHint}>
            <input
              type="text"
              value={workspaceUrl}
              onChange={e => setWorkspaceUrl(e.target.value)}
              placeholder="http://192.168.14.63/magician"
              className="w-full px-3 py-2 border rounded-lg dark:bg-gray-700 dark:border-gray-600 dark:text-white"
            />
          </Field>
          <Field
            label={t.plane.apiKey}
            hint={mode === 'edit' ? t.plane.leaveBlankToKeep + ` (${workspace?.api_key_masked || '-'})` : undefined}
          >
            <input
              type="password"
              value={apiKey}
              onChange={e => setApiKey(e.target.value)}
              placeholder="plane_api_..."
              className="w-full px-3 py-2 border rounded-lg dark:bg-gray-700 dark:border-gray-600 dark:text-white font-mono"
            />
          </Field>
          <Field
            label={t.plane.webhookSecret}
            hint={mode === 'edit' ? t.plane.leaveBlankToKeep + ` (${workspace?.webhook_secret_masked || '-'})` : undefined}
          >
            <input
              type="password"
              value={webhookSecret}
              onChange={e => setWebhookSecret(e.target.value)}
              placeholder="plane_wh_..."
              className="w-full px-3 py-2 border rounded-lg dark:bg-gray-700 dark:border-gray-600 dark:text-white font-mono"
            />
          </Field>
          {error && <p className="text-sm text-red-600">{error}</p>}
        </div>
        <div className="flex justify-end gap-2 mt-6">
          <button onClick={onClose} className="px-4 py-2 text-sm text-gray-600 dark:text-gray-400">
            {t.common.cancel}
          </button>
          <button
            onClick={() => mut.mutate()}
            disabled={!canSubmit || mut.isPending}
            className="px-4 py-2 text-sm bg-sky-600 text-white rounded-lg hover:bg-sky-700 disabled:opacity-50"
          >
            {mode === 'create' ? t.common.create : t.common.save}
          </button>
        </div>
      </div>
    </div>
  )
}

function Field({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div>
      <label className="block text-sm font-medium mb-1 dark:text-gray-300">{label}</label>
      {children}
      {hint && <p className="text-xs text-gray-500 dark:text-gray-400 mt-1">{hint}</p>}
    </div>
  )
}

// ── Binding Create/Edit Modal (scoped to workspace) ──

interface LabelRow {
  label_id: string
  cli_type: string
  priority: number
}

function BindingModal({
  workspace,
  binding,
  onClose,
  onSaved,
}: {
  workspace: PlaneWorkspace
  binding?: PlaneBinding
  onClose: () => void
  onSaved: () => void
}) {
  const { t } = useI18n()
  const isEdit = !!binding
  const [projectId, setProjectId] = useState(binding?.plane_project_id ?? '')
  const [groupId, setGroupId] = useState(binding?.agent_group_id ?? '')
  const [acceptStateId, setAcceptStateId] = useState(binding?.accept_state_id ?? '')
  const [inProgressStateId, setInProgressStateId] = useState(binding?.in_progress_state_id ?? '')
  const [completionStateId, setCompletionStateId] = useState(binding?.completion_state_id ?? '')
  const [labelRows, setLabelRows] = useState<LabelRow[]>(
    binding?.labels.map(l => ({ label_id: l.label_id, cli_type: l.cli_type, priority: l.priority })) ?? []
  )
  const [error, setError] = useState<string | null>(null)

  const { data: projects = [], isError: projectsError } = useQuery({
    queryKey: ['plane-projects', workspace.id],
    queryFn: () => planeApi.listWorkspaceProjects(workspace.id),
  })
  const { data: groups = [] } = useQuery({ queryKey: ['agent-groups'], queryFn: agentGroupsApi.list })
  const { data: clis = [] } = useQuery({ queryKey: ['clis'], queryFn: clisApi.list })

  const { data: states = [], isLoading: statesLoading } = useQuery({
    queryKey: ['plane-states', workspace.id, projectId],
    queryFn: () => planeApi.listProjectStates(workspace.id, projectId),
    enabled: !!projectId,
  })
  const { data: labels = [], isLoading: labelsLoading } = useQuery({
    queryKey: ['plane-labels', workspace.id, projectId],
    queryFn: () => planeApi.listProjectLabels(workspace.id, projectId),
    enabled: !!projectId,
  })

  // When project changes (in create mode), reset state/label selections
  function handleProjectChange(pid: string) {
    setProjectId(pid)
    if (!isEdit) {
      setAcceptStateId('')
      setInProgressStateId('')
      setCompletionStateId('')
      setLabelRows([])
    }
  }

  function addLabelRow() {
    const usedIds = new Set(labelRows.map(r => r.label_id))
    const firstUnused = labels.find(l => !usedIds.has(l.id))
    const nextPriority = labelRows.length === 0 ? 0 : Math.max(...labelRows.map(r => r.priority)) + 1
    setLabelRows([
      ...labelRows,
      { label_id: firstUnused?.id ?? '', cli_type: 'codex', priority: nextPriority },
    ])
  }
  function updateLabelRow(idx: number, patch: Partial<LabelRow>) {
    setLabelRows(rows => rows.map((r, i) => (i === idx ? { ...r, ...patch } : r)))
  }
  function removeLabelRow(idx: number) {
    setLabelRows(rows => rows.filter((_, i) => i !== idx))
  }

  const mut = useMutation({
    mutationFn: async () => {
      setError(null)
      const project = projects.find(p => p.id === projectId)
      if (!project) throw new Error('Select a project')
      const accept = states.find(s => s.id === acceptStateId)
      const inProgress = states.find(s => s.id === inProgressStateId)
      const completion = states.find(s => s.id === completionStateId)
      if (!accept || !inProgress || !completion) throw new Error('Select all three states')

      const labelInputs: PlaneBindingLabelInput[] = labelRows.map(r => {
        const lb = labels.find(l => l.id === r.label_id)
        if (!lb) throw new Error(`Unknown label ${r.label_id}`)
        return {
          label_id: lb.id,
          label_name: lb.name,
          cli_type: r.cli_type,
          priority: r.priority,
        }
      })

      if (labelInputs.length === 0) throw new Error('At least one label is required')
      const seenIds = new Set<string>()
      const seenPri = new Set<number>()
      for (const li of labelInputs) {
        if (seenIds.has(li.label_id)) throw new Error(`Duplicate label: ${li.label_name}`)
        if (seenPri.has(li.priority)) throw new Error(`Duplicate priority: ${li.priority}`)
        seenIds.add(li.label_id)
        seenPri.add(li.priority)
      }
      const runnableCli = clis.find(c => !c.wip)?.value ?? 'codex'
      if (!labelInputs.some(li => clis.find(c => c.value === li.cli_type && !c.wip))) {
        throw new Error(`At least one label must use a runnable CLI (e.g. ${runnableCli})`)
      }

      if (isEdit && binding) {
        return planeApi.updateBinding(binding.id, {
          agent_group_id: groupId,
          accept_state_id: accept.id,
          accept_state_name: accept.name,
          in_progress_state_id: inProgress.id,
          in_progress_state_name: inProgress.name,
          completion_state_id: completion.id,
          completion_state_name: completion.name,
          labels: labelInputs,
        })
      }
      return planeApi.createBinding(workspace.id, {
        plane_project_id: project.id,
        plane_project_name: project.name,
        plane_project_identifier: project.identifier,
        agent_group_id: groupId,
        accept_state_id: accept.id,
        accept_state_name: accept.name,
        in_progress_state_id: inProgress.id,
        in_progress_state_name: inProgress.name,
        completion_state_id: completion.id,
        completion_state_name: completion.name,
        labels: labelInputs,
      })
    },
    onSuccess: onSaved,
    onError: (e: Error) => setError(e.message),
  })

  const stateOption = (s: { id: string; name: string; group: string }) =>
    `${s.name}${s.group ? ` (${s.group})` : ''}`
  const usedLabelIds = new Set(labelRows.map(r => r.label_id))

  const canSubmit =
    !!projectId && !!groupId && !!acceptStateId && !!inProgressStateId && !!completionStateId && labelRows.length > 0

  return (
    <div className="fixed inset-0 bg-black/40 flex items-center justify-center z-50 p-4 overflow-y-auto" onClick={onClose}>
      <div
        className="bg-white dark:bg-gray-800 rounded-xl shadow-xl p-6 w-full max-w-2xl my-8"
        onClick={e => e.stopPropagation()}
      >
        <h2 className="text-lg font-bold mb-1 dark:text-white">
          {isEdit ? 'Edit binding' : t.plane.addBinding}
        </h2>
        <p className="text-xs text-gray-500 dark:text-gray-400 mb-4">{workspace.name}</p>
        {projectsError && (
          <p className="text-amber-600 dark:text-amber-400 text-sm mb-4">{t.plane.projectsFetchFailed}</p>
        )}
        <div className="space-y-4">
          <div className="grid grid-cols-2 gap-3">
            <Field label={t.plane.planeProject}>
              <select
                value={projectId}
                onChange={e => handleProjectChange(e.target.value)}
                disabled={isEdit}
                className="w-full px-3 py-2 border rounded-lg dark:bg-gray-700 dark:border-gray-600 dark:text-white disabled:opacity-60"
              >
                <option value="">{t.plane.selectProject}</option>
                {projects.map((p: PlaneProject) => (
                  <option key={p.id} value={p.id}>{p.name} ({p.identifier})</option>
                ))}
              </select>
            </Field>
            <Field label={t.plane.agentGroup}>
              <select
                value={groupId}
                onChange={e => setGroupId(e.target.value)}
                className="w-full px-3 py-2 border rounded-lg dark:bg-gray-700 dark:border-gray-600 dark:text-white"
              >
                <option value="">{t.plane.selectGroup}</option>
                {groups.map(g => (
                  <option key={g.id} value={g.id}>{g.name}</option>
                ))}
              </select>
            </Field>
          </div>

          {projectId && (
            <>
              <div>
                <label className="block text-sm font-medium mb-2 dark:text-gray-300">States</label>
                {statesLoading ? (
                  <p className="text-xs text-gray-500">Loading states…</p>
                ) : (
                  <div className="grid grid-cols-3 gap-2">
                    <Field label="Accept (entry)">
                      <select
                        value={acceptStateId}
                        onChange={e => setAcceptStateId(e.target.value)}
                        className="w-full px-2 py-1.5 text-sm border rounded-lg dark:bg-gray-700 dark:border-gray-600 dark:text-white"
                      >
                        <option value="">— select —</option>
                        {states.map(s => <option key={s.id} value={s.id}>{stateOption(s)}</option>)}
                      </select>
                    </Field>
                    <Field label="In progress">
                      <select
                        value={inProgressStateId}
                        onChange={e => setInProgressStateId(e.target.value)}
                        className="w-full px-2 py-1.5 text-sm border rounded-lg dark:bg-gray-700 dark:border-gray-600 dark:text-white"
                      >
                        <option value="">— select —</option>
                        {states.map(s => <option key={s.id} value={s.id}>{stateOption(s)}</option>)}
                      </select>
                    </Field>
                    <Field label="Completion">
                      <select
                        value={completionStateId}
                        onChange={e => setCompletionStateId(e.target.value)}
                        className="w-full px-2 py-1.5 text-sm border rounded-lg dark:bg-gray-700 dark:border-gray-600 dark:text-white"
                      >
                        <option value="">— select —</option>
                        {states.map(s => <option key={s.id} value={s.id}>{stateOption(s)}</option>)}
                      </select>
                    </Field>
                  </div>
                )}
              </div>

              <div>
                <div className="flex items-center justify-between mb-2">
                  <label className="block text-sm font-medium dark:text-gray-300">Labels → CLI</label>
                  <button
                    type="button"
                    onClick={addLabelRow}
                    disabled={labelsLoading || labels.length === 0 || usedLabelIds.size >= labels.length}
                    className="text-xs text-sky-600 dark:text-sky-400 hover:underline disabled:opacity-50"
                  >
                    + Add label
                  </button>
                </div>
                {labelsLoading ? (
                  <p className="text-xs text-gray-500">Loading labels…</p>
                ) : labels.length === 0 ? (
                  <p className="text-xs text-amber-600 dark:text-amber-400">
                    No labels in this project. Create a label in Plane first.
                  </p>
                ) : labelRows.length === 0 ? (
                  <p className="text-xs text-gray-500 dark:text-gray-400 italic">
                    Add at least one label, with a non-WIP CLI.
                  </p>
                ) : (
                  <div className="space-y-2">
                    {labelRows.map((row, idx) => {
                      const otherIds = new Set(labelRows.filter((_, i) => i !== idx).map(r => r.label_id))
                      const lb = labels.find(l => l.id === row.label_id)
                      return (
                        <div
                          key={idx}
                          className="flex items-center gap-2 border rounded-lg dark:border-gray-700 bg-gray-50 dark:bg-gray-900/40 px-2 py-2"
                        >
                          {lb?.color && (
                            <span
                              className="w-3 h-3 rounded-full shrink-0"
                              style={{ backgroundColor: lb.color }}
                              title={lb.color}
                            />
                          )}
                          <select
                            value={row.label_id}
                            onChange={e => updateLabelRow(idx, { label_id: e.target.value })}
                            className="flex-1 px-2 py-1 text-sm border rounded dark:bg-gray-700 dark:border-gray-600 dark:text-white"
                          >
                            <option value="">— label —</option>
                            {labels.map(l => (
                              <option key={l.id} value={l.id} disabled={otherIds.has(l.id)}>
                                {l.name}
                              </option>
                            ))}
                          </select>
                          <select
                            value={row.cli_type}
                            onChange={e => updateLabelRow(idx, { cli_type: e.target.value })}
                            className="w-40 px-2 py-1 text-sm border rounded dark:bg-gray-700 dark:border-gray-600 dark:text-white"
                          >
                            {clis.map(c => (
                              <option key={c.value} value={c.value}>
                                {c.label}{c.wip ? ' (WIP)' : ''}
                              </option>
                            ))}
                          </select>
                          <input
                            type="number"
                            value={row.priority}
                            onChange={e => updateLabelRow(idx, { priority: parseInt(e.target.value, 10) || 0 })}
                            className="w-16 px-2 py-1 text-sm border rounded dark:bg-gray-700 dark:border-gray-600 dark:text-white"
                            title="Priority (lower = higher priority on conflict)"
                          />
                          <button
                            type="button"
                            onClick={() => removeLabelRow(idx)}
                            className="text-gray-400 hover:text-red-500 dark:hover:text-red-400 text-sm"
                          >
                            ✕
                          </button>
                        </div>
                      )
                    })}
                  </div>
                )}
              </div>
            </>
          )}
          {error && <p className="text-sm text-red-600 dark:text-red-400">{error}</p>}
        </div>
        <div className="flex justify-end gap-2 mt-6">
          <button onClick={onClose} className="px-4 py-2 text-sm text-gray-600 dark:text-gray-400">
            {t.common.cancel}
          </button>
          <button
            onClick={() => mut.mutate()}
            disabled={!canSubmit || mut.isPending}
            className="px-4 py-2 text-sm bg-sky-600 text-white rounded-lg hover:bg-sky-700 disabled:opacity-50"
          >
            {mut.isPending ? '...' : isEdit ? t.common.save : t.common.create}
          </button>
        </div>
      </div>
    </div>
  )
}
