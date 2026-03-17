import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Plus, Trash2, RefreshCw, Edit2, Server, Info } from 'lucide-react'
import { serversApi, type Server as ServerType } from '../lib/api'
import { useI18n } from '../hooks/useI18n'
import { translateSshError } from '../lib/i18n'

interface ServerFormData {
  name: string; ip: string; port: string; username: string; password: string; save_password: boolean; os_type: string
}

const defaultForm: ServerFormData = {
  name: '', ip: '', port: '22', username: '', password: '', save_password: true, os_type: 'linux',
}

export default function Servers() {
  const qc = useQueryClient()
  const { t } = useI18n()
  const [showModal, setShowModal] = useState(false)
  const [editServer, setEditServer] = useState<ServerType | null>(null)
  const [form, setForm] = useState<ServerFormData>(defaultForm)
  const [testResults, setTestResults] = useState<Record<string, { status: string; message: string }>>({})

  const { data: servers = [], isLoading } = useQuery({ queryKey: ['servers'], queryFn: serversApi.list })

  const createMutation = useMutation({
    mutationFn: (data: ServerFormData) => serversApi.create({
      name: data.name,
      ip: data.ip,
      port: parseInt(data.port),
      username: data.username,
      password: data.password || undefined,
      save_password: data.save_password,
      os_type: data.os_type,
    }),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['servers'] }); closeModal() },
  })

  const updateMutation = useMutation({
    mutationFn: ({ id, data }: { id: string; data: ServerFormData }) =>
      serversApi.update(id, {
        name: data.name, ip: data.ip, port: parseInt(data.port), username: data.username, os_type: data.os_type,
        password: data.password || undefined,
        save_password: data.password ? data.save_password : undefined,
      }),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['servers'] }); closeModal() },
  })

  const deleteMutation = useMutation({
    mutationFn: serversApi.delete,
    onSuccess: () => qc.invalidateQueries({ queryKey: ['servers'] }),
  })

  const testMutation = useMutation({
    mutationFn: async (id: string) => {
      const result = await serversApi.test(id)
      setTestResults(prev => ({ ...prev, [id]: result }))
      qc.invalidateQueries({ queryKey: ['servers'] })
      return result
    },
  })

  function openCreate() { setEditServer(null); setForm(defaultForm); setShowModal(true) }
  function openEdit(server: ServerType) {
    setEditServer(server)
    setForm({ name: server.name, ip: server.ip, port: String(server.port), username: server.username, password: '', save_password: true, os_type: server.os_type ?? 'linux' })
    setShowModal(true)
  }
  function closeModal() { setShowModal(false); setEditServer(null); setForm(defaultForm) }

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    const trimmed = { ...form, name: form.name.trim(), ip: form.ip.trim(), username: form.username.trim() }
    if (editServer) updateMutation.mutate({ id: editServer.id, data: trimmed })
    else createMutation.mutate(trimmed)
  }

  const isPending = createMutation.isPending || updateMutation.isPending

  return (
    <div className="p-8">
      <div className="flex items-center justify-between mb-8">
        <div>
          <h1 className="text-2xl font-bold text-gray-900 dark:text-white">{t.servers.title}</h1>
          <p className="text-gray-500 mt-1">{t.servers.subtitle}</p>
        </div>
        <button onClick={openCreate} className="btn-primary flex items-center gap-2">
          <Plus size={16} />{t.servers.addServer}
        </button>
      </div>

      {isLoading ? (
        <div className="text-center py-12 text-gray-500">{t.common.loading}</div>
      ) : servers.length === 0 ? (
        <div className="text-center py-12 card">
          <Server size={40} className="mx-auto text-gray-400 dark:text-gray-600 mb-3" />
          <p className="text-gray-500 dark:text-gray-400">{t.servers.noServers}</p>
          <p className="text-gray-400 dark:text-gray-600 text-sm mt-1">{t.servers.noServersHint}</p>
        </div>
      ) : (
        <div className="grid gap-4">
          {servers.map(server => (
            <div key={server.id} className="card flex items-center gap-4">
              <div className="w-10 h-10 rounded-lg bg-blue-600/20 flex items-center justify-center">
                <Server size={18} className="text-blue-400" />
              </div>
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2">
                  <p className="font-medium text-gray-800 dark:text-gray-100">{server.name}</p>
                  <StatusBadge status={server.status} t={t} />
                  <span className="badge badge-gray">{server.os_type === 'mac' ? t.servers.osTypeMac : t.servers.osTypeLinux}</span>
                </div>
                <p className="text-sm text-gray-500">
                  {server.username}@{server.ip}:{server.port}
                </p>
                {testResults[server.id] && (
                  <p className={`text-xs mt-1 ${testResults[server.id].status === 'online' ? 'text-green-500' : 'text-red-500'}`}>
                    {testResults[server.id].status === 'online' ? testResults[server.id].message : translateSshError(testResults[server.id].message, t)}
                  </p>
                )}
              </div>
              <div className="flex items-center gap-2">
                {(server.status === 'offline' || server.status === 'unknown') && (
                  <button onClick={() => testMutation.mutate(server.id)} disabled={testMutation.isPending} className="btn-secondary btn-sm flex items-center gap-1">
                    <RefreshCw size={14} className={testMutation.isPending ? 'animate-spin' : ''} />{t.servers.reconnect}
                  </button>
                )}
                <button onClick={() => openEdit(server)} className="btn-secondary btn-sm"><Edit2 size={14} /></button>
                <button onClick={() => { if (confirm(`${t.common.delete} "${server.name}"?`)) deleteMutation.mutate(server.id) }} className="btn-danger btn-sm"><Trash2 size={14} /></button>
              </div>
            </div>
          ))}
        </div>
      )}

      {showModal && (
        <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50 p-4">
          <div className="bg-white border border-gray-200 rounded-xl w-full max-w-lg dark:bg-gray-900 dark:border-gray-700">
            <div className="flex items-center justify-between px-6 py-4 border-b border-gray-200 dark:border-gray-700">
              <h3 className="font-semibold text-gray-800 dark:text-gray-100">{editServer ? t.servers.editServer : t.servers.addServer}</h3>
              <button onClick={closeModal} className="text-gray-400 hover:text-gray-700 dark:hover:text-gray-300">✕</button>
            </div>
            <form onSubmit={handleSubmit} className="p-6 space-y-4">
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.common.name}</label>
                  <input className="input" value={form.name} onChange={e => setForm(f => ({ ...f, name: e.target.value }))} placeholder="prod-server-01" required />
                </div>
                <div>
                  <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.servers.ipAddress}</label>
                  <input className="input" value={form.ip} onChange={e => setForm(f => ({ ...f, ip: e.target.value }))} placeholder="192.168.1.100" required />
                </div>
              </div>
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.servers.sshPort}</label>
                  <input type="number" className="input" value={form.port} onChange={e => setForm(f => ({ ...f, port: e.target.value }))} />
                </div>
                <div>
                  <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.servers.sshUsername}</label>
                  <input className="input" value={form.username} onChange={e => setForm(f => ({ ...f, username: e.target.value }))} placeholder="ubuntu" required />
                </div>
              </div>

              <div>
                <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.servers.osType}</label>
                <div className="flex gap-3">
                  {(['linux', 'mac'] as const).map(os => (
                    <label key={os} className="flex items-center gap-1.5 cursor-pointer">
                      <input
                        type="radio"
                        name="os_type"
                        value={os}
                        checked={form.os_type === os}
                        onChange={() => setForm(f => ({ ...f, os_type: os }))}
                        className="accent-sky-500"
                      />
                      <span className="text-sm text-gray-700 dark:text-gray-300">
                        {os === 'linux' ? t.servers.osTypeLinux : t.servers.osTypeMac}
                      </span>
                    </label>
                  ))}
                </div>
              </div>

              <div>
                <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">
                  {t.servers.password}
                  <span className="ml-1.5 text-gray-400 dark:text-gray-600 font-normal">({editServer ? t.servers.passwordEditHint : t.common.optionalIfPasswordless})</span>
                </label>
                <input
                  type="password"
                  className="input"
                  value={form.password}
                  onChange={e => setForm(f => ({ ...f, password: e.target.value }))}
                  placeholder="••••••"
                />
                {form.password && (
                  <label className="flex items-center gap-2 mt-2 cursor-pointer">
                    <input
                      type="checkbox"
                      checked={form.save_password}
                      onChange={e => setForm(f => ({ ...f, save_password: e.target.checked }))}
                      className="accent-sky-500"
                    />
                    <span className="text-sm text-gray-600 dark:text-gray-400">{t.servers.savePassword}</span>
                  </label>
                )}
              </div>

              {/* Info tip */}
              <div className="flex gap-2.5 p-3 rounded-lg bg-sky-50 border border-sky-200 dark:bg-sky-900/20 dark:border-sky-800">
                <Info size={15} className="text-sky-500 shrink-0 mt-0.5" />
                <p className="text-xs text-sky-700 dark:text-sky-300 leading-relaxed">
                  {t.servers.sshTip}
                </p>
              </div>

              {(createMutation.error || updateMutation.error) && (
                <div className="text-red-500 dark:text-red-400 text-sm">
                  {translateSshError(String((createMutation.error || updateMutation.error)?.message), t)}
                </div>
              )}
              <div className="flex gap-3 justify-end pt-2">
                <button type="button" onClick={closeModal} className="btn-secondary">{t.common.cancel}</button>
                <button type="submit" className="btn-primary" disabled={isPending}>
                  {isPending ? t.common.loading : editServer ? t.common.update : t.common.create}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  )
}

function StatusBadge({ status, t }: { status: string; t: ReturnType<typeof useI18n>['t'] }) {
  const map: Record<string, string> = { online: 'badge-green', offline: 'badge-red', unknown: 'badge-gray' }
  return <span className={map[status] ?? 'badge-gray'}>{t.status[status as keyof typeof t.status] ?? status}</span>
}
