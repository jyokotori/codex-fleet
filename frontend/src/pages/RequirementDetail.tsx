import { useState, useRef, useEffect } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { ArrowLeft, Plus, Trash2, Edit2, Bot, User, X, MoreHorizontal, Eye, EyeOff } from 'lucide-react'
import { projectsApi, workItemsApi, agentsApi, usersApi, notificationsApi, type WorkItem } from '../lib/api'
import { getAuth } from '../lib/auth'
import { useI18n } from '../hooks/useI18n'

// ── Column definitions ──────────────────────────────────────────────────────

interface ColumnDef {
  key: string
  color: string
  defaultVisible: boolean
}

const COLUMNS: ColumnDef[] = [
  { key: 'backlog', color: '#94a3b8', defaultVisible: true },
  { key: 'waiting', color: '#3b82f6', defaultVisible: true },
  { key: 'agent_in_progress', color: '#f59e0b', defaultVisible: true },
  { key: 'agent_completed', color: '#a855f7', defaultVisible: true },
  { key: 'human_rejected', color: '#f97316', defaultVisible: true },
  { key: 'agent_failed', color: '#ef4444', defaultVisible: false },
  { key: 'human_approved', color: '#22c55e', defaultVisible: false },
  { key: 'cancelled', color: '#6b7280', defaultVisible: false },
]

const ALL_STATUSES = COLUMNS.map(c => c.key)

const PRIORITY_COLORS: Record<string, string> = {
  low: 'text-gray-400',
  medium: 'text-yellow-500',
  high: 'text-orange-500',
  urgent: 'text-red-500',
}

// ── Helpers ─────────────────────────────────────────────────────────────────

function relativeTime(dateStr: string): string {
  const now = Date.now()
  const then = new Date(dateStr).getTime()
  const diff = now - then
  const mins = Math.floor(diff / 60000)
  if (mins < 1) return 'just now'
  if (mins < 60) return `${mins}m ago`
  const hours = Math.floor(mins / 60)
  if (hours < 24) return `${hours}h ago`
  const days = Math.floor(hours / 24)
  if (days < 30) return `${days}d ago`
  return new Date(dateStr).toLocaleDateString()
}

function getVisibleColumns(projectId: string): string[] {
  try {
    const stored = localStorage.getItem(`kanban-cols-${projectId}`)
    if (stored) return JSON.parse(stored)
  } catch {}
  return COLUMNS.filter(c => c.defaultVisible).map(c => c.key)
}

function setVisibleColumns(projectId: string, cols: string[]) {
  localStorage.setItem(`kanban-cols-${projectId}`, JSON.stringify(cols))
}

// ── Main Component ──────────────────────────────────────────────────────────

const currentUserId = getAuth()?.user?.id ?? ''

interface WorkItemFormData {
  title: string
  description: string
  priority: string
  assigned_agent_id: string
  assigned_user_id: string
  notification_ids: string[]
}

const defaultForm: WorkItemFormData = {
  title: '', description: '', priority: 'medium', assigned_agent_id: '', assigned_user_id: currentUserId, notification_ids: [],
}

export default function RequirementDetail() {
  const { projectId } = useParams<{ projectId: string }>()
  const navigate = useNavigate()
  const qc = useQueryClient()
  const { t } = useI18n()
  const tr = t.requirements
  const twt = t.workItems

  const [visibleCols, setVisibleCols] = useState<string[]>(() => getVisibleColumns(projectId ?? ''))
  const [selectedItem, setSelectedItem] = useState<WorkItem | null>(null)
  const [creatingInColumn, setCreatingInColumn] = useState<string | null>(null)
  const [quickTitle, setQuickTitle] = useState('')
  const quickInputRef = useRef<HTMLInputElement>(null)
  const [showModal, setShowModal] = useState(false)
  const [editItem, setEditItem] = useState<WorkItem | null>(null)
  const [createStatus, setCreateStatus] = useState<string | null>(null)
  const [form, setForm] = useState<WorkItemFormData>(defaultForm)

  const { data: project } = useQuery({
    queryKey: ['projects', projectId],
    queryFn: () => projectsApi.get(projectId!),
    enabled: !!projectId,
  })

  const { data: items = [], isLoading } = useQuery({
    queryKey: ['work-items', projectId],
    queryFn: () => projectsApi.listWorkItems(projectId!),
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
    mutationFn: (data: { title: string; status?: string; description?: string; priority?: string; assigned_agent_id?: string; assigned_user_id?: string; notification_ids?: string[] }) =>
      projectsApi.createWorkItem(projectId!, data),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['work-items', projectId] }) },
  })

  const updateMutation = useMutation({
    mutationFn: ({ id, data }: { id: string; data: Parameters<typeof workItemsApi.update>[1] }) =>
      workItemsApi.update(id, data),
    onSuccess: (updated) => {
      qc.invalidateQueries({ queryKey: ['work-items', projectId] })
      if (selectedItem?.id === updated.id) setSelectedItem(updated)
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

  // Focus quick-create input when opening
  useEffect(() => {
    if (creatingInColumn && quickInputRef.current) {
      quickInputRef.current.focus()
    }
  }, [creatingInColumn])

  function toggleColumn(key: string) {
    const next = visibleCols.includes(key)
      ? visibleCols.filter(c => c !== key)
      : [...visibleCols, key]
    setVisibleCols(next)
    setVisibleColumns(projectId ?? '', next)
  }

  function quickCreate(status: string) {
    if (!quickTitle.trim()) return
    let projectNotifIds: string[] = []
    try { projectNotifIds = JSON.parse(project?.notification_ids ?? '[]') } catch {}
    createMutation.mutate({
      title: quickTitle.trim(),
      status,
      assigned_user_id: currentUserId || undefined,
      notification_ids: projectNotifIds.length > 0 ? projectNotifIds : undefined,
    })
    setQuickTitle('')
  }

  function openCreate(status?: string) {
    setEditItem(null)
    setCreateStatus(status ?? 'backlog')
    let projectNotifIds: string[] = []
    try { projectNotifIds = JSON.parse(project?.notification_ids ?? '[]') } catch {}
    setForm({ ...defaultForm, notification_ids: projectNotifIds })
    setShowModal(true)
  }

  function openEdit(item: WorkItem) {
    setEditItem(item)
    setCreateStatus(null)
    let parsedNotifIds: string[] = []
    try { parsedNotifIds = JSON.parse(item.notification_ids) } catch {}
    setForm({
      title: item.title,
      description: item.description,
      priority: item.priority,
      assigned_agent_id: item.assigned_agent_id ?? '',
      assigned_user_id: item.assigned_user_id ?? '',
      notification_ids: parsedNotifIds,
    })
    setShowModal(true)
  }

  function closeModal() { setShowModal(false); setEditItem(null); setCreateStatus(null); setForm(defaultForm) }

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    if (editItem) {
      updateMutation.mutate({ id: editItem.id, data: {
        title: form.title, description: form.description, priority: form.priority,
        assigned_agent_id: form.assigned_agent_id || '',
        assigned_user_id: form.assigned_user_id || '',
        notification_ids: form.notification_ids,
      }})
    } else if (createStatus) {
      createMutation.mutate({
        title: form.title,
        description: form.description || undefined,
        status: createStatus,
        priority: form.priority,
        assigned_agent_id: form.assigned_agent_id || undefined,
        assigned_user_id: form.assigned_user_id || undefined,
        notification_ids: form.notification_ids.length > 0 ? form.notification_ids : undefined,
      }, { onSuccess: () => closeModal() })
    }
  }

  const isPending = createMutation.isPending || updateMutation.isPending

  // Group items by status
  const itemsByStatus: Record<string, WorkItem[]> = {}
  for (const col of COLUMNS) itemsByStatus[col.key] = []
  for (const item of items) {
    if (itemsByStatus[item.status]) itemsByStatus[item.status].push(item)
  }

  const hiddenCols = COLUMNS.filter(c => !visibleCols.includes(c.key))

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <div className="px-6 py-3 border-b border-gray-200 dark:border-gray-800 bg-white dark:bg-gray-900 shrink-0 flex items-center gap-3">
        <button
          onClick={() => navigate('/requirements')}
          className="p-1.5 text-gray-400 hover:text-gray-700 dark:hover:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg transition-colors"
        >
          <ArrowLeft size={16} />
        </button>
        <div className="min-w-0 flex-1">
          <h1 className="text-lg font-bold text-gray-900 dark:text-white truncate">{project?.name ?? '...'}</h1>
          {project?.description && (
            <p className="text-sm text-gray-500 truncate">{project.description}</p>
          )}
        </div>
        <button
          onClick={() => openCreate()}
          className="shrink-0 flex items-center gap-1.5 px-3 py-1.5 bg-blue-600 hover:bg-blue-700 text-white text-sm font-medium rounded-lg transition-colors"
        >
          <Plus size={14} />
          {tr.addWorkItem}
        </button>
      </div>

      {/* Board */}
      <div className="flex-1 overflow-x-auto overflow-y-hidden">
        {isLoading ? (
          <div className="p-6 text-gray-500">{t.common.loading}</div>
        ) : (
          <div className="flex h-full p-4 gap-4">
            {/* Visible columns */}
            {COLUMNS.filter(c => visibleCols.includes(c.key)).map(col => {
              const colItems = itemsByStatus[col.key] ?? []
              return (
                <div
                  key={col.key}
                  className="w-72 shrink-0 flex flex-col bg-gray-50 dark:bg-gray-950 rounded-xl"
                >
                  {/* Column header */}
                  <div className="flex items-center gap-2 px-3 py-2.5 shrink-0">
                    <span
                      className="w-3 h-3 rounded-full shrink-0"
                      style={{ backgroundColor: col.color }}
                    />
                    <span className="text-sm font-medium text-gray-700 dark:text-gray-300">
                      {(twt as Record<string, string>)[col.key] ?? col.key}
                    </span>
                    <span className="text-xs text-gray-400 bg-gray-200 dark:bg-gray-800 px-1.5 py-0.5 rounded-full min-w-[1.25rem] text-center">
                      {colItems.length}
                    </span>
                    <div className="ml-auto flex items-center gap-0.5">
                      <ColumnMenu
                        onHide={() => toggleColumn(col.key)}
                        hideLabel={tr.hideColumn}
                      />
                      <button
                        onClick={() => { setCreatingInColumn(col.key); setQuickTitle('') }}
                        className="p-1 text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-800 rounded transition-colors"
                      >
                        <Plus size={14} />
                      </button>
                    </div>
                  </div>

                  {/* Cards */}
                  <div className="flex-1 overflow-y-auto px-2 pb-2 space-y-2">
                    {/* Inline quick create */}
                    {creatingInColumn === col.key && (
                      <div className="bg-white dark:bg-gray-900 border-2 border-blue-400 dark:border-blue-600 rounded-lg p-3 shadow-sm">
                        <input
                          ref={quickInputRef}
                          type="text"
                          placeholder={tr.itemTitle + '...'}
                          value={quickTitle}
                          onChange={(e) => setQuickTitle(e.target.value)}
                          onKeyDown={(e) => {
                            if (e.key === 'Enter' && quickTitle.trim()) { quickCreate(col.key) }
                            if (e.key === 'Escape') { setCreatingInColumn(null); setQuickTitle('') }
                          }}
                          className="w-full text-sm bg-transparent text-gray-900 dark:text-white placeholder-gray-400 outline-none"
                        />
                        <div className="flex items-center gap-2 mt-2">
                          <button
                            onClick={() => quickCreate(col.key)}
                            disabled={!quickTitle.trim() || createMutation.isPending}
                            className="px-2.5 py-1 text-xs font-medium bg-blue-600 hover:bg-blue-700 text-white rounded-md transition-colors disabled:opacity-50"
                          >
                            {t.common.create}
                          </button>
                          <button
                            onClick={() => { setCreatingInColumn(null); setQuickTitle('') }}
                            className="px-2.5 py-1 text-xs text-gray-500 hover:text-gray-700 dark:hover:text-gray-300"
                          >
                            {t.common.cancel}
                          </button>
                        </div>
                      </div>
                    )}

                    {colItems.map(item => {
                      const agent = agents.find(a => a.id === item.assigned_agent_id)
                      return (
                        <div
                          key={item.id}
                          onClick={() => setSelectedItem(selectedItem?.id === item.id ? null : item)}
                          className={`bg-white dark:bg-gray-900 border rounded-lg p-3 cursor-pointer transition-all hover:shadow-md ${
                            selectedItem?.id === item.id
                              ? 'border-blue-400 dark:border-blue-600 shadow-sm ring-1 ring-blue-400/30'
                              : 'border-gray-200 dark:border-gray-800 hover:border-gray-300 dark:hover:border-gray-700'
                          }`}
                        >
                          <div className="text-[11px] font-mono text-gray-400 dark:text-gray-500 mb-0.5">
                            {item.id.slice(0, 8).toUpperCase()}
                          </div>
                          <p className="text-sm font-medium text-gray-900 dark:text-white leading-snug line-clamp-2">
                            {item.title}
                          </p>
                          <div className="flex items-center gap-2 mt-2 text-xs text-gray-400 dark:text-gray-500">
                            <span className={`${PRIORITY_COLORS[item.priority] ?? 'text-gray-400'}`}>●</span>
                            {agent && (
                              <span className="flex items-center gap-0.5 truncate">
                                <Bot size={10} />
                                {agent.name}
                              </span>
                            )}
                            <span className="ml-auto shrink-0">{relativeTime(item.updated_at)}</span>
                          </div>
                        </div>
                      )
                    })}

                    {/* Bottom + button */}
                    {creatingInColumn !== col.key && (
                      <button
                        onClick={() => { setCreatingInColumn(col.key); setQuickTitle('') }}
                        className="w-full py-2 text-sm text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-900 rounded-lg transition-colors flex items-center justify-center gap-1"
                      >
                        <Plus size={14} />
                      </button>
                    )}
                  </div>
                </div>
              )
            })}

            {/* Hidden columns panel */}
            {hiddenCols.length > 0 && (
              <div className="w-52 shrink-0">
                <div className="text-xs font-medium text-gray-500 dark:text-gray-400 mb-2 px-1 flex items-center gap-1">
                  <EyeOff size={12} />
                  {tr.hiddenColumns}
                </div>
                <div className="space-y-1.5">
                  {hiddenCols.map(col => {
                    const count = (itemsByStatus[col.key] ?? []).length
                    return (
                      <button
                        key={col.key}
                        onClick={() => toggleColumn(col.key)}
                        className="w-full flex items-center gap-2 px-3 py-2 bg-gray-50 dark:bg-gray-950 hover:bg-gray-100 dark:hover:bg-gray-900 rounded-lg transition-colors text-left"
                      >
                        <span
                          className="w-2.5 h-2.5 rounded-full shrink-0"
                          style={{ backgroundColor: col.color }}
                        />
                        <span className="text-sm text-gray-600 dark:text-gray-400">
                          {(twt as Record<string, string>)[col.key] ?? col.key}
                        </span>
                        {count > 0 && (
                          <span className="text-xs text-gray-400 ml-auto">{count}</span>
                        )}
                        <Eye size={12} className="text-gray-400 ml-auto shrink-0" />
                      </button>
                    )
                  })}
                </div>
              </div>
            )}
          </div>
        )}
      </div>

      {/* Detail drawer */}
      {selectedItem && (
        <DetailDrawer
          item={selectedItem}
          agents={agents}
          users={users}
          notifConfigs={notifConfigs}
          t={t}
          onClose={() => setSelectedItem(null)}
          onEdit={() => openEdit(selectedItem)}
          onDelete={() => {
            if (confirm(`${t.common.delete} "${selectedItem.title}"?`)) {
              deleteMutation.mutate(selectedItem.id)
            }
          }}
          onUpdateStatus={(status) => updateMutation.mutate({ id: selectedItem.id, data: { status } })}
          onUpdateAgent={(agentId) => updateMutation.mutate({ id: selectedItem.id, data: { assigned_agent_id: agentId } })}
          onUpdateUser={(userId) => updateMutation.mutate({ id: selectedItem.id, data: { assigned_user_id: userId } })}
          onNavigate={navigate}
        />
      )}

      {/* Create/Edit modal */}
      {showModal && (editItem || createStatus) && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4">
          <div className="bg-white dark:bg-gray-900 rounded-xl shadow-xl w-full max-w-md">
            <div className="flex items-center justify-between p-6 border-b border-gray-100 dark:border-gray-800">
              <h2 className="text-lg font-semibold text-gray-900 dark:text-white">
                {editItem ? tr.editWorkItem : tr.addWorkItem}
              </h2>
              <button onClick={closeModal} className="text-gray-400 hover:text-gray-600 dark:hover:text-gray-200">
                <X size={16} />
              </button>
            </div>
            <form onSubmit={handleSubmit} className="p-6 space-y-4">
              {/* Title */}
              <div>
                <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">{tr.itemTitle}</label>
                <input
                  type="text"
                  value={form.title}
                  onChange={(e) => setForm({ ...form, title: e.target.value })}
                  required
                  className="w-full px-3 py-2 border border-gray-300 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-white text-sm focus:ring-2 focus:ring-blue-500 focus:border-transparent"
                />
              </div>

              {/* Description */}
              <div>
                <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">{tr.itemDescription}</label>
                <textarea
                  value={form.description}
                  onChange={(e) => setForm({ ...form, description: e.target.value })}
                  rows={3}
                  className="w-full px-3 py-2 border border-gray-300 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-white text-sm focus:ring-2 focus:ring-blue-500 focus:border-transparent resize-none"
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
              {notifConfigs.length > 0 && (
                <div>
                  <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">{t.notifications.selectNotifications}</label>
                  <div className="space-y-1.5">
                    {notifConfigs.map(n => (
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
                        <span className={`text-sm ${n.enabled ? 'text-gray-700 dark:text-gray-300' : 'text-gray-400 dark:text-gray-500'}`}>
                          {n.name}
                          {!n.enabled && <span className="ml-1 text-xs">({t.common.disabled})</span>}
                        </span>
                      </label>
                    ))}
                  </div>
                </div>
              )}

              <div className="flex gap-3 pt-2">
                <button type="button" onClick={closeModal} className="flex-1 px-4 py-2 border border-gray-300 dark:border-gray-700 rounded-lg text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors">
                  {t.common.cancel}
                </button>
                <button type="submit" disabled={isPending} className="flex-1 px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg text-sm font-medium transition-colors disabled:opacity-60">
                  {isPending ? t.common.loading : t.common.save}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  )
}

// ── Column Menu ─────────────────────────────────────────────────────────────

function ColumnMenu({ onHide, hideLabel }: { onHide: () => void; hideLabel: string }) {
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    function handleClick(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false)
    }
    if (open) document.addEventListener('mousedown', handleClick)
    return () => document.removeEventListener('mousedown', handleClick)
  }, [open])

  return (
    <div className="relative" ref={ref}>
      <button
        onClick={() => setOpen(!open)}
        className="p-1 text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-800 rounded transition-colors"
      >
        <MoreHorizontal size={14} />
      </button>
      {open && (
        <div className="absolute right-0 top-full mt-1 bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-700 rounded-lg shadow-lg py-1 z-20 min-w-[120px]">
          <button
            onClick={() => { onHide(); setOpen(false) }}
            className="w-full px-3 py-1.5 text-sm text-left text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 flex items-center gap-2"
          >
            <EyeOff size={12} />
            {hideLabel}
          </button>
        </div>
      )}
    </div>
  )
}

// ── Detail Drawer ───────────────────────────────────────────────────────────

function DetailDrawer({
  item,
  agents,
  users,
  notifConfigs,
  t,
  onClose,
  onEdit,
  onDelete,
  onUpdateStatus,
  onUpdateAgent,
  onUpdateUser,
  onNavigate,
}: {
  item: WorkItem
  agents: { id: string; name: string; is_busy: boolean }[]
  users: { id: string; display_name: string }[]
  notifConfigs: { id: string; name: string; enabled: boolean }[]
  t: Record<string, any>
  onClose: () => void
  onEdit: () => void
  onDelete: () => void
  onUpdateStatus: (status: string) => void
  onUpdateAgent: (agentId: string) => void
  onUpdateUser: (userId: string) => void
  onNavigate: (path: string) => void
}) {
  const tr = t.requirements
  const twt = t.workItems

  const agent = agents.find(a => a.id === item.assigned_agent_id)
  let notifIds: string[] = []
  try { notifIds = JSON.parse(item.notification_ids) } catch {}
  const linkedNotifs = notifConfigs.filter(n => notifIds.includes(n.id))

  return (
    <div className="fixed right-0 top-0 bottom-0 w-96 bg-white dark:bg-gray-900 border-l border-gray-200 dark:border-gray-800 shadow-xl z-30 flex flex-col">
      {/* Header */}
      <div className="px-5 py-4 border-b border-gray-100 dark:border-gray-800 flex items-start justify-between gap-2 shrink-0">
        <div className="min-w-0">
          <div className="text-[11px] font-mono text-gray-400 dark:text-gray-500 mb-1">
            {item.id.slice(0, 8).toUpperCase()}
          </div>
          <h2 className="font-semibold text-gray-900 dark:text-white leading-snug">{item.title}</h2>
        </div>
        <div className="flex items-center gap-1 shrink-0">
          <button
            onClick={onEdit}
            className="p-1.5 text-gray-400 hover:text-blue-600 hover:bg-blue-50 dark:hover:bg-blue-900/20 rounded-lg transition-colors"
          >
            <Edit2 size={14} />
          </button>
          <button
            onClick={onDelete}
            className="p-1.5 text-gray-400 hover:text-red-600 hover:bg-red-50 dark:hover:bg-red-900/20 rounded-lg transition-colors"
          >
            <Trash2 size={14} />
          </button>
          <button
            onClick={onClose}
            className="p-1.5 text-gray-400 hover:text-gray-600 dark:hover:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg transition-colors ml-1"
          >
            <X size={14} />
          </button>
        </div>
      </div>

      {/* Body */}
      <div className="p-5 space-y-4 flex-1 overflow-auto">
        {item.description && (
          <div>
            <p className="text-sm text-gray-500 mb-1">{tr.itemDescription}</p>
            <p className="text-sm text-gray-700 dark:text-gray-300 whitespace-pre-wrap">{item.description}</p>
          </div>
        )}

        {/* Status */}
        <div>
          <p className="text-sm text-gray-500 mb-1">{tr.itemStatus}</p>
          <select
            value={item.status}
            onChange={(e) => {
              if (e.target.value === 'waiting' && !item.description.trim()) {
                alert(tr.descriptionRequiredForWaiting)
                return
              }
              onUpdateStatus(e.target.value)
            }}
            className="w-full text-sm border border-gray-300 dark:border-gray-700 rounded-lg px-2 py-1.5 bg-white dark:bg-gray-800 text-gray-700 dark:text-gray-300"
          >
            {ALL_STATUSES.map(s => (
              <option key={s} value={s}>{(twt as Record<string, string>)[s] ?? s}</option>
            ))}
          </select>
        </div>

        {/* Priority */}
        <div className="flex items-center gap-2">
          <span className="text-sm text-gray-500 w-24">{tr.itemPriority}</span>
          <span className={`text-sm font-medium ${PRIORITY_COLORS[item.priority] ?? ''}`}>
            ● {(twt as Record<string, string>)[item.priority] ?? item.priority}
          </span>
        </div>

        {/* Assign User */}
        <div>
          <p className="text-sm text-gray-500 mb-1">{tr.itemUser}</p>
          <select
            value={item.assigned_user_id ?? ''}
            onChange={(e) => onUpdateUser(e.target.value)}
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
            value={item.assigned_agent_id ?? ''}
            onChange={(e) => onUpdateAgent(e.target.value)}
            className="w-full text-sm border border-gray-300 dark:border-gray-700 rounded-lg px-2 py-1.5 bg-white dark:bg-gray-800 text-gray-700 dark:text-gray-300"
          >
            <option value="">{tr.noAgent}</option>
            {agents.map((a) => (
              <option key={a.id} value={a.id}>{a.name}{a.is_busy ? ` (${tr.busy})` : ''}</option>
            ))}
          </select>
        </div>

        {/* Execution link */}
        {item.execution_id && (
          <div>
            <p className="text-sm text-gray-500 mb-1">{tr.itemExecution}</p>
            <button
              onClick={() => {
                if (agent) onNavigate(`/agents/${agent.id}?tab=tasks&task=${item.execution_id}`)
              }}
              className="text-sm text-blue-600 hover:text-blue-700 dark:text-blue-400 dark:hover:text-blue-300 underline"
            >
              {item.execution_id.slice(0, 8)}...
            </button>
          </div>
        )}

        {/* Notification configs */}
        {linkedNotifs.length > 0 && (
          <div>
            <p className="text-sm text-gray-500 mb-1">{t.notifications.selectNotifications}</p>
            <div className="flex flex-wrap gap-1">
              {linkedNotifs.map(n => (
                <span key={n.id} className="badge badge-blue text-xs">{n.name}</span>
              ))}
            </div>
          </div>
        )}

        {/* Timestamps */}
        <div className="pt-2 border-t border-gray-100 dark:border-gray-800 text-xs text-gray-400 space-y-1">
          <div>{t.common.createdAt}: {new Date(item.created_at).toLocaleString()}</div>
          <div>{t.common.updatedAt}: {new Date(item.updated_at).toLocaleString()}</div>
        </div>
      </div>
    </div>
  )
}
