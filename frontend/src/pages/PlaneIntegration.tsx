import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Plus, Trash2, ToggleLeft, ToggleRight, Plane, Pencil, ChevronDown, ChevronRight, Copy } from 'lucide-react'
import {
  planeApi,
  agentGroupsApi,
  type PlaneWorkspace,
  type PlaneBinding,
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
  const [bindingModal, setBindingModal] = useState<PlaneWorkspace | null>(null)
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
                  onAddBinding={() => setBindingModal(ws)}
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
          workspace={bindingModal}
          onClose={() => setBindingModal(null)}
          onSaved={() => {
            qc.invalidateQueries({ queryKey: ['plane-bindings', bindingModal.id] })
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
}: {
  workspace: PlaneWorkspace
  expanded: boolean
  onToggleExpand: () => void
  onEdit: () => void
  onToggle: () => void
  onDelete: () => void
  onAddBinding: () => void
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
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b dark:border-gray-700">
                    <th className="text-left py-2 px-2 font-medium text-gray-500 dark:text-gray-400">{t.plane.planeProject}</th>
                    <th className="text-left py-2 px-2 font-medium text-gray-500 dark:text-gray-400">{t.plane.agentGroup}</th>
                    <th className="text-center py-2 px-2 font-medium text-gray-500 dark:text-gray-400">Status</th>
                    <th className="text-right py-2 px-2"></th>
                  </tr>
                </thead>
                <tbody>
                  {bindings.map((b: PlaneBinding) => (
                    <tr key={b.id} className="border-b dark:border-gray-700/50">
                      <td className="py-2 px-2 dark:text-white">
                        <span className="font-medium">{b.plane_project_name}</span>
                        {b.plane_project_identifier && (
                          <span className="text-gray-400 ml-2 text-xs">{b.plane_project_identifier}</span>
                        )}
                      </td>
                      <td className="py-2 px-2 dark:text-gray-300">{b.agent_group_name || b.agent_group_id.slice(0, 8)}</td>
                      <td className="py-2 px-2 text-center">
                        <button onClick={() => toggleBindingMut.mutate(b.id)} className="inline-flex items-center gap-1">
                          {b.enabled ? (
                            <ToggleRight size={18} className="text-green-500" />
                          ) : (
                            <ToggleLeft size={18} className="text-gray-400" />
                          )}
                        </button>
                      </td>
                      <td className="py-2 px-2 text-right">
                        <button
                          onClick={() => { if (confirm('Delete binding?')) deleteBindingMut.mutate(b.id) }}
                          className="p-1 text-gray-500 hover:text-red-600"
                        >
                          <Trash2 size={14} />
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
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

// ── Binding Create Modal (scoped to workspace) ──

function BindingModal({
  workspace,
  onClose,
  onSaved,
}: {
  workspace: PlaneWorkspace
  onClose: () => void
  onSaved: () => void
}) {
  const { t } = useI18n()
  const [projectId, setProjectId] = useState('')
  const [groupId, setGroupId] = useState('')

  const { data: projects = [], isError: projectsError } = useQuery({
    queryKey: ['plane-projects', workspace.id],
    queryFn: () => planeApi.listWorkspaceProjects(workspace.id),
  })
  const { data: groups = [] } = useQuery({ queryKey: ['agent-groups'], queryFn: agentGroupsApi.list })

  const mut = useMutation({
    mutationFn: () => {
      const project = projects.find((p: PlaneProject) => p.id === projectId)
      if (!project) throw new Error('Select a project')
      return planeApi.createBinding(workspace.id, {
        plane_project_id: project.id,
        plane_project_name: project.name,
        plane_project_identifier: project.identifier,
        agent_group_id: groupId,
      })
    },
    onSuccess: onSaved,
  })

  return (
    <div className="fixed inset-0 bg-black/40 flex items-center justify-center z-50" onClick={onClose}>
      <div className="bg-white dark:bg-gray-800 rounded-xl shadow-xl p-6 w-full max-w-md" onClick={e => e.stopPropagation()}>
        <h2 className="text-lg font-bold mb-1 dark:text-white">{t.plane.addBinding}</h2>
        <p className="text-xs text-gray-500 dark:text-gray-400 mb-4">{workspace.name}</p>
        {projectsError && (
          <p className="text-amber-600 dark:text-amber-400 text-sm mb-4">{t.plane.projectsFetchFailed}</p>
        )}
        <div className="space-y-4">
          <Field label={t.plane.planeProject}>
            <select
              value={projectId}
              onChange={e => setProjectId(e.target.value)}
              className="w-full px-3 py-2 border rounded-lg dark:bg-gray-700 dark:border-gray-600 dark:text-white"
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
        <div className="flex justify-end gap-2 mt-6">
          <button onClick={onClose} className="px-4 py-2 text-sm text-gray-600 dark:text-gray-400">{t.common.cancel}</button>
          <button
            onClick={() => mut.mutate()}
            disabled={!projectId || !groupId || mut.isPending}
            className="px-4 py-2 text-sm bg-sky-600 text-white rounded-lg hover:bg-sky-700 disabled:opacity-50"
          >
            {t.common.create}
          </button>
        </div>
      </div>
    </div>
  )
}
