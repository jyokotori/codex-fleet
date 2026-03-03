import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Plus, Trash2, Edit2, Bell, ToggleLeft, ToggleRight } from 'lucide-react'
import { notificationsApi, type NotificationConfig } from '../lib/api'
import { useI18n } from '../hooks/useI18n'

interface NotifFormData {
  name: string; type: string; webhook_url: string; enabled: boolean; events: string[]
}

const EVENT_OPTIONS = ['task_completed', 'task_failed', 'agent_started', 'agent_stopped']

const defaultForm: NotifFormData = {
  name: '', type: 'webhook', webhook_url: '', enabled: true,
  events: ['task_completed', 'task_failed'],
}

export default function Notifications() {
  const qc = useQueryClient()
  const { t } = useI18n()
  const [showModal, setShowModal] = useState(false)
  const [editNotif, setEditNotif] = useState<NotificationConfig | null>(null)
  const [form, setForm] = useState<NotifFormData>(defaultForm)

  const { data: notifs = [], isLoading } = useQuery({ queryKey: ['notifications'], queryFn: notificationsApi.list })

  const createMutation = useMutation({
    mutationFn: (data: NotifFormData) =>
      notificationsApi.create({
        name: data.name, type: data.type,
        config_json: JSON.stringify({ url: data.webhook_url }),
        enabled: data.enabled, events_json: JSON.stringify(data.events),
      }),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['notifications'] }); closeModal() },
  })

  const updateMutation = useMutation({
    mutationFn: ({ id, data }: { id: string; data: NotifFormData }) =>
      notificationsApi.update(id, {
        name: data.name, config_json: JSON.stringify({ url: data.webhook_url }),
        enabled: data.enabled, events_json: JSON.stringify(data.events),
      }),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['notifications'] }); closeModal() },
  })

  const deleteMutation = useMutation({
    mutationFn: notificationsApi.delete,
    onSuccess: () => qc.invalidateQueries({ queryKey: ['notifications'] }),
  })

  const toggleMutation = useMutation({
    mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) => notificationsApi.update(id, { enabled }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['notifications'] }),
  })

  function openCreate() { setEditNotif(null); setForm(defaultForm); setShowModal(true) }
  function openEdit(notif: NotificationConfig) {
    setEditNotif(notif)
    let config: Record<string, string> = {}
    try { config = JSON.parse(notif.config_json) } catch {}
    let events: string[] = []
    try { events = JSON.parse(notif.events_json) } catch {}
    setForm({ name: notif.name, type: notif.type, webhook_url: config.url ?? '', enabled: notif.enabled, events })
    setShowModal(true)
  }
  function closeModal() { setShowModal(false); setEditNotif(null); setForm(defaultForm) }
  function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    if (editNotif) updateMutation.mutate({ id: editNotif.id, data: form })
    else createMutation.mutate(form)
  }

  const isPending = createMutation.isPending || updateMutation.isPending

  return (
    <div className="p-8">
      <div className="flex items-center justify-between mb-8">
        <div>
          <h1 className="text-2xl font-bold text-gray-900 dark:text-white">{t.notifications.title}</h1>
          <p className="text-gray-500 mt-1">{t.notifications.subtitle}</p>
        </div>
        <button onClick={openCreate} className="btn-primary flex items-center gap-2">
          <Plus size={16} />{t.notifications.addWebhook}
        </button>
      </div>

      {isLoading ? (
        <div className="text-center py-12 text-gray-500">{t.common.loading}</div>
      ) : notifs.length === 0 ? (
        <div className="text-center py-12 card">
          <Bell size={40} className="mx-auto text-gray-400 dark:text-gray-600 mb-3" />
          <p className="text-gray-500 dark:text-gray-400">{t.notifications.noNotifications}</p>
          <p className="text-gray-400 dark:text-gray-600 text-sm mt-1">{t.notifications.noNotificationsHint}</p>
        </div>
      ) : (
        <div className="grid gap-4">
          {notifs.map(notif => {
            let config: Record<string, string> = {}
            try { config = JSON.parse(notif.config_json) } catch {}
            let events: string[] = []
            try { events = JSON.parse(notif.events_json) } catch {}

            return (
              <div key={notif.id} className="card flex items-center gap-4">
                <div className={`w-10 h-10 rounded-lg flex items-center justify-center ${notif.enabled ? 'bg-green-600/20' : 'bg-gray-100 dark:bg-gray-700'}`}>
                  <Bell size={18} className={notif.enabled ? 'text-green-500' : 'text-gray-400 dark:text-gray-500'} />
                </div>
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <p className="font-medium text-gray-800 dark:text-gray-100">{notif.name}</p>
                    <span className="badge badge-blue">{notif.type}</span>
                    {!notif.enabled && <span className="badge badge-gray">{t.common.disabled}</span>}
                  </div>
                  <p className="text-xs text-gray-500 truncate">{config.url}</p>
                  <div className="flex gap-1 mt-1">
                    {events.map(ev => <span key={ev} className="badge badge-gray text-xs">{ev}</span>)}
                  </div>
                </div>
                <div className="flex items-center gap-2">
                  <button
                    onClick={() => toggleMutation.mutate({ id: notif.id, enabled: !notif.enabled })}
                    className="text-gray-400 hover:text-gray-700 dark:hover:text-gray-200 transition-colors"
                    title={notif.enabled ? t.common.disabled : t.common.enabled}
                  >
                    {notif.enabled ? <ToggleRight size={22} className="text-green-500" /> : <ToggleLeft size={22} />}
                  </button>
                  <button onClick={() => openEdit(notif)} className="btn-secondary btn-sm"><Edit2 size={13} /></button>
                  <button onClick={() => { if (confirm(`${t.common.delete} "${notif.name}"?`)) deleteMutation.mutate(notif.id) }} className="btn-danger btn-sm"><Trash2 size={13} /></button>
                </div>
              </div>
            )
          })}
        </div>
      )}

      {showModal && (
        <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50 p-4">
          <div className="bg-white border border-gray-200 rounded-xl w-full max-w-lg dark:bg-gray-900 dark:border-gray-700">
            <div className="flex items-center justify-between px-6 py-4 border-b border-gray-200 dark:border-gray-700">
              <h3 className="font-semibold text-gray-800 dark:text-gray-100">
                {editNotif ? t.notifications.editNotification : t.notifications.addNotification}
              </h3>
              <button onClick={closeModal} className="text-gray-400 hover:text-gray-700 dark:hover:text-gray-300">✕</button>
            </div>
            <form onSubmit={handleSubmit} className="p-6 space-y-4">
              <div>
                <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.common.name}</label>
                <input className="input" value={form.name} onChange={e => setForm(f => ({ ...f, name: e.target.value }))} placeholder="Slack Alerts" required />
              </div>
              <div>
                <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.notifications.webhookUrl}</label>
                <input type="url" className="input" value={form.webhook_url} onChange={e => setForm(f => ({ ...f, webhook_url: e.target.value }))} placeholder="https://hooks.slack.com/..." required />
              </div>
              <div>
                <label className="block text-sm text-gray-600 dark:text-gray-400 mb-2">{t.notifications.events}</label>
                <div className="space-y-2">
                  {EVENT_OPTIONS.map(event => (
                    <label key={event} className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={form.events.includes(event)}
                        onChange={e => {
                          if (e.target.checked) setForm(f => ({ ...f, events: [...f.events, event] }))
                          else setForm(f => ({ ...f, events: f.events.filter(ev => ev !== event) }))
                        }}
                        className="w-4 h-4 rounded"
                      />
                      <span className="text-sm text-gray-700 dark:text-gray-300">{event}</span>
                    </label>
                  ))}
                </div>
              </div>
              <div>
                <label className="flex items-center gap-2 cursor-pointer">
                  <input type="checkbox" checked={form.enabled} onChange={e => setForm(f => ({ ...f, enabled: e.target.checked }))} className="w-4 h-4 rounded" />
                  <span className="text-sm text-gray-700 dark:text-gray-300">{t.common.enabled}</span>
                </label>
              </div>
              {(createMutation.error || updateMutation.error) && (
                <div className="text-red-500 dark:text-red-400 text-sm">{String((createMutation.error || updateMutation.error)?.message)}</div>
              )}
              <div className="flex gap-3 justify-end pt-2">
                <button type="button" onClick={closeModal} className="btn-secondary">{t.common.cancel}</button>
                <button type="submit" className="btn-primary" disabled={isPending}>
                  {isPending ? t.notifications.saving : editNotif ? t.common.update : t.common.create}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  )
}
