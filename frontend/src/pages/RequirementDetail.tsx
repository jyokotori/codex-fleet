import { useState } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { ArrowLeft, Plus, Trash2, Edit2, ChevronRight, Bot, User } from 'lucide-react'
import { projectsApi, workItemsApi, agentsApi, usersApi, notificationsApi, type WorkItem } from '../lib/api'
import { getAuth } from '../lib/auth'
import { useI18n } from '../hooks/useI18n'

const TYPE_COLORS: Record<string, string> = {
  epic: 'bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-400',
  story: 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400',
  task: 'bg-teal-100 text-teal-700 dark:bg-teal-900/30 dark:text-teal-400',
}

const PRIORITY_COLORS: Record<string, string> = {
  low: 'text-gray-400',
  medium: 'text-yellow-500',
  high: 'text-orange-500',
  urgent: 'text-red-500',
}

const STATUS_COLORS: Record<string, string> = {
  waiting: 'bg-gray-100 text-gray-600 dark:bg-gray-800 dark:text-gray-400',
  agent_in_progress: 'bg-sky-100 text-sky-700 dark:bg-sky-900/30 dark:text-sky-400',
  agent_completed: 'bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400',
  agent_failed: 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400',
  human_approved: 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400',
  human_rejected: 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400',
  cancelled: 'bg-gray-100 text-gray-400 dark:bg-gray-800 dark:text-gray-500',
  closed: 'bg-gray-100 text-gray-400 dark:bg-gray-800 dark:text-gray-500',
}

// Status machine: which manual transitions are available from each status
const STATUS_TRANSITIONS: Record<string, string[]> = {
  waiting: ['cancelled', 'closed'],
  agent_in_progress: ['cancelled', 'closed'],
  agent_completed: ['human_approved', 'human_rejected'],
  agent_failed: ['waiting', 'closed'],
  human_approved: ['closed'],
  human_rejected: ['waiting'],
  cancelled: [],
  closed: [],
}

interface WorkItemFormData {
  parent_id: string
  type: string
  title: string
  description: string
  priority: string
  assigned_agent_id: string
  assigned_user_id: string
  notification_ids: string[]
}

const currentUserId = getAuth()?.user?.id ?? ''

const defaultForm: WorkItemFormData = {
  parent_id: '', type: 'task', title: '', description: '', priority: 'medium', assigned_agent_id: '', assigned_user_id: currentUserId, notification_ids: [],
}

export default function RequirementDetail() {
  const { projectId } = useParams<{ projectId: string }>()
  const navigate = useNavigate()
  const qc = useQueryClient()
  const { t } = useI18n()
  const tr = t.requirements
  const twt = t.workItems

  const [filterStatus, setFilterStatus] = useState('')
  const [filterType, setFilterType] = useState('')
  const [showModal, setShowModal] = useState(false)
  const [editItem, setEditItem] = useState<WorkItem | null>(null)
  const [form, setForm] = useState<WorkItemFormData>(defaultForm)
  const [selectedItem, setSelectedItem] = useState<WorkItem | null>(null)

  const { data: project } = useQuery({
    queryKey: ['projects', projectId],
    queryFn: () => projectsApi.get(projectId!),
    enabled: !!projectId,
  })

  const { data: items = [], isLoading } = useQuery({
    queryKey: ['work-items', projectId, filterStatus, filterType],
    queryFn: () => projectsApi.listWorkItems(projectId!, {
      status: filterStatus || undefined,
      type: filterType || undefined,
    }),
    enabled: !!projectId,
  })

  const { data: agents = [] } = useQuery({
    queryKey: ['agents'],
    queryFn: agentsApi.list,
  })

  const { data: users = [] } = useQuery({
    queryKey: ['users'],
    queryFn: usersApi.list,
  })

  const { data: notifConfigs = [] } = useQuery({
    queryKey: ['notifications'],
    queryFn: notificationsApi.list,
  })

  const createMutation = useMutation({
    mutationFn: (data: WorkItemFormData) =>
      projectsApi.createWorkItem(projectId!, {
        parent_id: data.parent_id || undefined,
        type: data.type,
        title: data.title,
        description: data.description || undefined,
        priority: data.priority,
        assigned_agent_id: data.assigned_agent_id || undefined,
        assigned_user_id: data.assigned_user_id || undefined,
        notification_ids: data.notification_ids.length > 0 ? data.notification_ids : undefined,
      }),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['work-items', projectId] }); closeModal() },
  })

  const updateMutation = useMutation({
    mutationFn: ({ id, data }: { id: string; data: Parameters<typeof workItemsApi.update>[1] }) =>
      workItemsApi.update(id, data),
    onSuccess: (updated) => {
      qc.invalidateQueries({ queryKey: ['work-items', projectId] })
      setSelectedItem(updated)
      closeModal()
    },
  })

  const deleteMutation = useMutation({
    mutationFn: workItemsApi.delete,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['work-items', projectId] })
      setSelectedItem(null)
    },
  })

  function openCreate() { setEditItem(null); setForm(defaultForm); setShowModal(true) }
  function openEdit(item: WorkItem) {
    setEditItem(item)
    let parsedNotifIds: string[] = []
    try { parsedNotifIds = JSON.parse(item.notification_ids) } catch {}
    setForm({
      parent_id: item.parent_id ?? '',
      type: item.type,
      title: item.title,
      description: item.description,
      priority: item.priority,
      assigned_agent_id: item.assigned_agent_id ?? '',
      assigned_user_id: item.assigned_user_id ?? '',
      notification_ids: parsedNotifIds,
    })
    setShowModal(true)
  }
  function closeModal() { setShowModal(false); setEditItem(null); setForm(defaultForm) }

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    if (editItem) {
      updateMutation.mutate({ id: editItem.id, data: {
        title: form.title, description: form.description, priority: form.priority,
        assigned_user_id: form.assigned_user_id || '',
        notification_ids: form.notification_ids,
      }})
    } else {
      createMutation.mutate(form)
    }
  }

  function transitionStatus(item: WorkItem, newStatus: string) {
    updateMutation.mutate({ id: item.id, data: { status: newStatus } })
  }

  function assignAgent(item: WorkItem, agentId: string) {
    updateMutation.mutate({ id: item.id, data: { assigned_agent_id: agentId } })
  }

  const isPending = createMutation.isPending || updateMutation.isPending

  // Build tree: top-level items first, then children indented
  const topLevel = items.filter((i) => !i.parent_id)
  const childrenOf = (parentId: string) => items.filter((i) => i.parent_id === parentId)

  function renderItem(item: WorkItem, depth = 0): React.ReactNode {
    const children = childrenOf(item.id)
    const agent = agents.find((a) => a.id === item.assigned_agent_id)
    const assignedUser = users.find((u) => u.id === item.assigned_user_id)
    return (
      <div key={item.id}>
        <div
          className={`flex items-center gap-3 px-4 py-3 border-b border-gray-100 dark:border-gray-800 hover:bg-gray-50 dark:hover:bg-gray-800/50 cursor-pointer transition-colors ${
            selectedItem?.id === item.id ? 'bg-sky-50 dark:bg-sky-900/10' : ''
          }`}
          style={{ paddingLeft: `${16 + depth * 24}px` }}
          onClick={() => setSelectedItem(selectedItem?.id === item.id ? null : item)}
        >
          {depth > 0 && <ChevronRight size={12} className="text-gray-400 shrink-0" />}
          <span className={`text-xs px-1.5 py-0.5 rounded font-medium shrink-0 ${TYPE_COLORS[item.type] ?? ''}`}>
            {(twt as Record<string, string>)[item.type] ?? item.type}
          </span>
          <span className="flex-1 text-sm text-gray-900 dark:text-white truncate">{item.title}</span>
          {assignedUser && (
            <span className="flex items-center gap-1 text-xs text-gray-500 shrink-0">
              <User size={12} />
              {assignedUser.display_name}
            </span>
          )}
          <span className={`text-xs font-medium shrink-0 ${PRIORITY_COLORS[item.priority] ?? ''}`}>
            ● {(twt as Record<string, string>)[item.priority] ?? item.priority}
          </span>
          <span className={`text-xs px-2 py-0.5 rounded-full shrink-0 ${STATUS_COLORS[item.status] ?? ''}`}>
            {(twt as Record<string, string>)[item.status] ?? item.status}
          </span>
          {agent && (
            <span className="flex items-center gap-1 text-xs text-gray-500 shrink-0">
              <Bot size={12} />
              {agent.name}
            </span>
          )}
        </div>
        {children.map((child) => renderItem(child, depth + 1))}
      </div>
    )
  }

  const actionLabel: Record<string, string> = {
    human_approved: tr.actionApprove,
    human_rejected: tr.actionReject,
    closed: tr.actionClose,
    cancelled: tr.actionCancel,
    waiting: tr.actionRequeue,
  }

  return (
    <div className="flex h-full">
      {/* Main list */}
      <div className={`flex flex-col flex-1 min-w-0 ${selectedItem ? 'border-r border-gray-200 dark:border-gray-800' : ''}`}>
        {/* Header */}
        <div className="px-6 py-4 border-b border-gray-200 dark:border-gray-800 bg-white dark:bg-gray-900 shrink-0">
          <div className="flex items-center gap-3 mb-3">
            <button
              onClick={() => navigate('/requirements')}
              className="p-1.5 text-gray-400 hover:text-gray-700 dark:hover:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg transition-colors"
            >
              <ArrowLeft size={16} />
            </button>
            <div className="min-w-0">
              <h1 className="text-lg font-bold text-gray-900 dark:text-white truncate">{project?.name ?? '...'}</h1>
              {project?.description && (
                <p className="text-sm text-gray-500 truncate">{project.description}</p>
              )}
            </div>
          </div>
          <div className="flex items-center gap-3">
            <select
              value={filterStatus}
              onChange={(e) => setFilterStatus(e.target.value)}
              className="text-sm border border-gray-300 dark:border-gray-700 rounded-lg px-2 py-1.5 bg-white dark:bg-gray-800 text-gray-700 dark:text-gray-300"
            >
              <option value="">{tr.allStatuses}</option>
              {['waiting','agent_in_progress','agent_completed','agent_failed','human_approved','human_rejected','cancelled','closed'].map((s) => (
                <option key={s} value={s}>{(twt as Record<string, string>)[s] ?? s}</option>
              ))}
            </select>
            <select
              value={filterType}
              onChange={(e) => setFilterType(e.target.value)}
              className="text-sm border border-gray-300 dark:border-gray-700 rounded-lg px-2 py-1.5 bg-white dark:bg-gray-800 text-gray-700 dark:text-gray-300"
            >
              <option value="">{tr.allTypes}</option>
              {['epic','story','task'].map((tp) => (
                <option key={tp} value={tp}>{(twt as Record<string, string>)[tp] ?? tp}</option>
              ))}
            </select>
            <div className="flex-1" />
            <button
              onClick={openCreate}
              className="flex items-center gap-1.5 px-3 py-1.5 bg-sky-600 hover:bg-sky-700 text-white rounded-lg text-sm font-medium transition-colors"
            >
              <Plus size={14} />
              {tr.addWorkItem}
            </button>
          </div>
        </div>

        {/* List */}
        <div className="flex-1 overflow-auto">
          {isLoading ? (
            <div className="p-6 text-gray-500">{t.common.loading}</div>
          ) : items.length === 0 ? (
            <div className="text-center py-16">
              <p className="text-gray-500">{tr.noWorkItems}</p>
              <p className="text-sm text-gray-400 mt-1">{tr.noWorkItemsHint}</p>
            </div>
          ) : (
            <div>
              {topLevel.map((item) => renderItem(item))}
            </div>
          )}
        </div>
      </div>

      {/* Side drawer */}
      {selectedItem && (
        <div className="w-96 shrink-0 flex flex-col bg-white dark:bg-gray-900 overflow-auto">
          <div className="px-5 py-4 border-b border-gray-100 dark:border-gray-800 flex items-start justify-between gap-2">
            <div className="min-w-0">
              <div className="flex items-center gap-2 mb-1">
                <span className={`text-xs px-1.5 py-0.5 rounded font-medium ${TYPE_COLORS[selectedItem.type] ?? ''}`}>
                  {(twt as Record<string, string>)[selectedItem.type] ?? selectedItem.type}
                </span>
                <span className={`text-xs px-2 py-0.5 rounded-full ${STATUS_COLORS[selectedItem.status] ?? ''}`}>
                  {(twt as Record<string, string>)[selectedItem.status] ?? selectedItem.status}
                </span>
              </div>
              <h2 className="font-semibold text-gray-900 dark:text-white">{selectedItem.title}</h2>
            </div>
            <div className="flex items-center gap-1 shrink-0">
              <button
                onClick={() => openEdit(selectedItem)}
                className="p-1.5 text-gray-400 hover:text-sky-600 hover:bg-sky-50 dark:hover:bg-sky-900/20 rounded-lg transition-colors"
              >
                <Edit2 size={14} />
              </button>
              <button
                onClick={() => {
                  if (confirm(`${t.common.delete} "${selectedItem.title}"?`)) {
                    deleteMutation.mutate(selectedItem.id)
                  }
                }}
                className="p-1.5 text-gray-400 hover:text-red-600 hover:bg-red-50 dark:hover:bg-red-900/20 rounded-lg transition-colors"
              >
                <Trash2 size={14} />
              </button>
              <button
                onClick={() => setSelectedItem(null)}
                className="p-1.5 text-gray-400 hover:text-gray-600 dark:hover:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg transition-colors ml-1"
              >
                ✕
              </button>
            </div>
          </div>

          <div className="p-5 space-y-4 flex-1 overflow-auto">
            {selectedItem.description && (
              <div>
                <p className="text-sm text-gray-500 mb-1">{tr.itemDescription}</p>
                <p className="text-sm text-gray-700 dark:text-gray-300 whitespace-pre-wrap">{selectedItem.description}</p>
              </div>
            )}

            {/* Priority */}
            <div className="flex items-center gap-2">
              <span className="text-sm text-gray-500 w-24">{tr.itemPriority}</span>
              <span className={`text-sm font-medium ${PRIORITY_COLORS[selectedItem.priority] ?? ''}`}>
                {(twt as Record<string, string>)[selectedItem.priority] ?? selectedItem.priority}
              </span>
            </div>

            {/* Assign User */}
            <div>
              <p className="text-sm text-gray-500 mb-1">{tr.itemUser}</p>
              <select
                value={selectedItem.assigned_user_id ?? ''}
                onChange={(e) => updateMutation.mutate({ id: selectedItem.id, data: { assigned_user_id: e.target.value } })}
                className="w-full text-sm border border-gray-300 dark:border-gray-700 rounded-lg px-2 py-1.5 bg-white dark:bg-gray-800 text-gray-700 dark:text-gray-300"
              >
                <option value="">{tr.noUser}</option>
                {users.map((u) => (
                  <option key={u.id} value={u.id}>{u.display_name}</option>
                ))}
              </select>
            </div>

            {/* Assign Agent */}
            <div>
              <p className="text-sm text-gray-500 mb-1">{tr.itemAgent}</p>
              <select
                value={selectedItem.assigned_agent_id ?? ''}
                onChange={(e) => assignAgent(selectedItem, e.target.value)}
                disabled={selectedItem.status === 'agent_in_progress'}
                className="w-full text-sm border border-gray-300 dark:border-gray-700 rounded-lg px-2 py-1.5 bg-white dark:bg-gray-800 text-gray-700 dark:text-gray-300 disabled:opacity-50"
              >
                <option value="">{tr.noAgent}</option>
                {agents.map((a) => (
                  <option key={a.id} value={a.id}>{a.name}{a.is_busy ? ` (${tr.busy})` : ''}</option>
                ))}
              </select>
            </div>

            {/* Execution link */}
            {selectedItem.execution_id && (
              <div>
                <p className="text-sm text-gray-500 mb-1">{tr.itemExecution}</p>
                <button
                  onClick={() => {
                    const agent = agents.find(a => a.id === selectedItem.assigned_agent_id)
                    if (agent) navigate(`/agents/${agent.id}?tab=tasks&task=${selectedItem.execution_id}`)
                  }}
                  className="text-sm text-sky-600 hover:text-sky-700 dark:text-sky-400 dark:hover:text-sky-300 underline"
                >
                  {selectedItem.execution_id.slice(0, 8)}...
                </button>
              </div>
            )}

            {/* Notification configs */}
            {(() => {
              let ids: string[] = []
              try { ids = JSON.parse(selectedItem.notification_ids) } catch {}
              const linked = notifConfigs.filter(n => ids.includes(n.id))
              return linked.length > 0 ? (
                <div>
                  <p className="text-sm text-gray-500 mb-1">{t.notifications.selectNotifications}</p>
                  <div className="flex flex-wrap gap-1">
                    {linked.map(n => (
                      <span key={n.id} className="badge badge-blue text-xs">{n.name}</span>
                    ))}
                  </div>
                </div>
              ) : null
            })()}

            {/* Status transitions */}
            <div>
              <p className="text-sm text-gray-500 mb-2">{tr.itemStatus}</p>
              <div className="flex flex-wrap gap-2">
                {(STATUS_TRANSITIONS[selectedItem.status] ?? []).map((nextStatus) => (
                  <button
                    key={nextStatus}
                    onClick={() => transitionStatus(selectedItem, nextStatus)}
                    className={`px-3 py-1.5 text-xs font-medium rounded-lg border transition-colors ${
                      nextStatus === 'closed' || nextStatus === 'human_rejected' || nextStatus === 'cancelled'
                        ? 'border-red-300 text-red-600 hover:bg-red-50 dark:border-red-700 dark:text-red-400 dark:hover:bg-red-900/20'
                        : nextStatus === 'human_approved'
                        ? 'border-green-300 text-green-600 hover:bg-green-50 dark:border-green-700 dark:text-green-400 dark:hover:bg-green-900/20'
                        : 'border-sky-300 text-sky-600 hover:bg-sky-50 dark:border-sky-700 dark:text-sky-400 dark:hover:bg-sky-900/20'
                    }`}
                  >
                    {actionLabel[nextStatus] ?? nextStatus}
                  </button>
                ))}
              </div>
            </div>

            {/* Timestamps */}
            <div className="pt-2 border-t border-gray-100 dark:border-gray-800 text-xs text-gray-400 space-y-1">
              <div>{t.common.createdAt}: {new Date(selectedItem.created_at).toLocaleString()}</div>
              <div>{t.common.updatedAt}: {new Date(selectedItem.updated_at).toLocaleString()}</div>
            </div>
          </div>
        </div>
      )}

      {/* Modal: create/edit work item */}
      {showModal && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4">
          <div className="bg-white dark:bg-gray-900 rounded-xl shadow-xl w-full max-w-md">
            <div className="flex items-center justify-between p-6 border-b border-gray-100 dark:border-gray-800">
              <h2 className="text-lg font-semibold text-gray-900 dark:text-white">
                {editItem ? tr.editWorkItem : tr.addWorkItem}
              </h2>
              <button onClick={closeModal} className="text-gray-400 hover:text-gray-600 dark:hover:text-gray-200">✕</button>
            </div>
            <form onSubmit={handleSubmit} className="p-6 space-y-4">
              {/* Type (only for create) */}
              {!editItem && (
                <div>
                  <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">{tr.itemType}</label>
                  <select
                    value={form.type}
                    onChange={(e) => setForm({ ...form, type: e.target.value })}
                    className="w-full px-3 py-2 border border-gray-300 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-white text-sm"
                  >
                    {['epic','story','task'].map((tp) => (
                      <option key={tp} value={tp}>{(twt as Record<string, string>)[tp] ?? tp}</option>
                    ))}
                  </select>
                </div>
              )}

              {/* Parent (only for create) */}
              {!editItem && (
                <div>
                  <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">{tr.itemParent}</label>
                  <select
                    value={form.parent_id}
                    onChange={(e) => setForm({ ...form, parent_id: e.target.value })}
                    className="w-full px-3 py-2 border border-gray-300 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-white text-sm"
                  >
                    <option value="">{tr.noParent}</option>
                    {items.filter((i) => i.type !== 'task').map((i) => (
                      <option key={i.id} value={i.id}>{i.title}</option>
                    ))}
                  </select>
                </div>
              )}

              {/* Title */}
              <div>
                <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">{tr.itemTitle}</label>
                <input
                  type="text"
                  value={form.title}
                  onChange={(e) => setForm({ ...form, title: e.target.value })}
                  required
                  className="w-full px-3 py-2 border border-gray-300 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-white text-sm focus:ring-2 focus:ring-sky-500 focus:border-transparent"
                />
              </div>

              {/* Description */}
              <div>
                <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">{tr.itemDescription}</label>
                <textarea
                  value={form.description}
                  onChange={(e) => setForm({ ...form, description: e.target.value })}
                  rows={3}
                  className="w-full px-3 py-2 border border-gray-300 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-white text-sm focus:ring-2 focus:ring-sky-500 focus:border-transparent resize-none"
                />
              </div>

              {/* Priority */}
              <div>
                <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">{tr.itemPriority}</label>
                <select
                  value={form.priority}
                  onChange={(e) => setForm({ ...form, priority: e.target.value })}
                  className="w-full px-3 py-2 border border-gray-300 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-white text-sm"
                >
                  {['low','medium','high','urgent'].map((p) => (
                    <option key={p} value={p}>{(twt as Record<string, string>)[p] ?? p}</option>
                  ))}
                </select>
              </div>

              {/* Assigned Agent */}
              <div>
                <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">{tr.itemAgent}</label>
                <select
                  value={form.assigned_agent_id}
                  onChange={(e) => setForm({ ...form, assigned_agent_id: e.target.value })}
                  className="w-full px-3 py-2 border border-gray-300 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-white text-sm"
                >
                  <option value="">{tr.noAgent}</option>
                  {agents.map((a) => (
                    <option key={a.id} value={a.id}>{a.name}{a.is_busy ? ` (${tr.busy})` : ''}</option>
                  ))}
                </select>
              </div>

              {/* Assigned User */}
              <div>
                <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">{tr.itemUser}</label>
                <select
                  value={form.assigned_user_id}
                  onChange={(e) => setForm({ ...form, assigned_user_id: e.target.value })}
                  className="w-full px-3 py-2 border border-gray-300 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-white text-sm"
                >
                  <option value="">{tr.noUser}</option>
                  {users.map((u) => (
                    <option key={u.id} value={u.id}>{u.display_name}</option>
                  ))}
                </select>
              </div>

              {/* Notifications */}
              {notifConfigs.filter(n => n.enabled).length > 0 && (
                <div>
                  <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">{t.notifications.selectNotifications}</label>
                  <div className="space-y-1.5">
                    {notifConfigs.filter(n => n.enabled).map(n => (
                      <label key={n.id} className="flex items-center gap-2 cursor-pointer">
                        <input
                          type="checkbox"
                          checked={form.notification_ids.includes(n.id)}
                          onChange={e => {
                            if (e.target.checked) setForm(f => ({ ...f, notification_ids: [...f.notification_ids, n.id] }))
                            else setForm(f => ({ ...f, notification_ids: f.notification_ids.filter(x => x !== n.id) }))
                          }}
                          className="w-4 h-4 rounded"
                        />
                        <span className="text-sm text-gray-700 dark:text-gray-300">{n.name}</span>
                      </label>
                    ))}
                  </div>
                </div>
              )}

              <div className="flex gap-3 pt-2">
                <button type="button" onClick={closeModal} className="flex-1 px-4 py-2 border border-gray-300 dark:border-gray-700 rounded-lg text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors">
                  {t.common.cancel}
                </button>
                <button type="submit" disabled={isPending} className="flex-1 px-4 py-2 bg-sky-600 hover:bg-sky-700 text-white rounded-lg text-sm font-medium transition-colors disabled:opacity-60">
                  {isPending ? t.common.loading : (editItem ? t.common.save : t.common.create)}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  )
}
