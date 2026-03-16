import { FormEvent, useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { adminUsersApi, type AdminUser } from '../../lib/api'
import { useI18n } from '../../hooks/useI18n'
import { Plus, Search, ChevronLeft, ChevronRight } from 'lucide-react'

const PER_PAGE = 15

export default function Users() {
  const { t } = useI18n()
  const queryClient = useQueryClient()

  const [search, setSearch] = useState('')
  const [page, setPage] = useState(1)
  const [showModal, setShowModal] = useState(false)
  const [form, setForm] = useState({ username: '', display_name: '', password: '' })
  const [error, setError] = useState('')

  const { data: users = [], isLoading } = useQuery({
    queryKey: ['admin-users'],
    queryFn: adminUsersApi.list,
  })

  const filtered = useMemo(() => {
    if (!search.trim()) return users
    const q = search.trim().toLowerCase()
    return users.filter(
      u => u.username.toLowerCase().includes(q) || u.display_name.toLowerCase().includes(q)
    )
  }, [users, search])

  const totalPages = Math.max(1, Math.ceil(filtered.length / PER_PAGE))
  const safePage = Math.min(page, totalPages)
  const paged = filtered.slice((safePage - 1) * PER_PAGE, safePage * PER_PAGE)

  const createMutation = useMutation({
    mutationFn: adminUsersApi.create,
    onSuccess: () => {
      setForm({ username: '', display_name: '', password: '' })
      setError('')
      setShowModal(false)
      void queryClient.invalidateQueries({ queryKey: ['admin-users'] })
    },
    onError: (err: unknown) => {
      setError(err instanceof Error ? err.message : t.common.error)
    },
  })

  const statusMutation = useMutation({
    mutationFn: ({ id, status }: { id: string; status: 'active' | 'disabled' }) =>
      adminUsersApi.updateStatus(id, status),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['admin-users'] })
    },
  })

  const unlockMutation = useMutation({
    mutationFn: (id: string) => adminUsersApi.unlock(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['admin-users'] })
    },
  })

  const resetMutation = useMutation({
    mutationFn: ({ id, pwd }: { id: string; pwd: string }) =>
      adminUsersApi.resetPassword(id, pwd),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['admin-users'] })
    },
  })

  function handleCreate(e: FormEvent) {
    e.preventDefault()
    setError('')
    createMutation.mutate({
      username: form.username.trim(),
      display_name: form.display_name.trim(),
      password: form.password,
      roles: ['member'],
    })
  }

  function handleToggleStatus(user: AdminUser) {
    const nextStatus = user.status === 'active' ? 'disabled' : 'active'
    statusMutation.mutate({ id: user.id, status: nextStatus })
  }

  function handleReset(user: AdminUser) {
    const value = window.prompt(t.users.resetPasswordPrompt, 'ChangeMe123!')
    if (!value) return
    resetMutation.mutate({ id: user.id, pwd: value })
  }

  function closeModal() {
    setShowModal(false)
    setForm({ username: '', display_name: '', password: '' })
    setError('')
  }

  return (
    <div className="p-8 space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold text-gray-900 dark:text-white">{t.users.title}</h1>
          <p className="text-gray-500 mt-1">{t.users.subtitle}</p>
        </div>
        <button className="btn-primary flex items-center gap-2" onClick={() => setShowModal(true)}>
          <Plus size={16} />
          {t.users.createUser}
        </button>
      </div>

      {/* Search + Table */}
      <div className="card">
        <div className="mb-4">
          <div className="relative max-w-sm">
            <Search size={16} className="absolute left-3 top-1/2 -translate-y-1/2 text-gray-400" />
            <input
              className="input pl-9"
              placeholder={t.users.searchPlaceholder}
              value={search}
              onChange={e => { setSearch(e.target.value); setPage(1) }}
            />
          </div>
        </div>

        {isLoading ? (
          <p className="text-gray-500 text-sm py-8 text-center">{t.common.loading}</p>
        ) : filtered.length === 0 ? (
          <p className="text-gray-500 text-sm py-8 text-center">{t.common.noData}</p>
        ) : (
          <>
            <div className="overflow-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="text-left text-gray-500 border-b border-gray-200 dark:border-gray-700">
                    <th className="py-2.5 pr-3 font-medium">{t.auth.username}</th>
                    <th className="py-2.5 pr-3 font-medium">{t.auth.displayName}</th>
                    <th className="py-2.5 pr-3 font-medium">{t.users.roles}</th>
                    <th className="py-2.5 pr-3 font-medium">{t.common.status}</th>
                    <th className="py-2.5 pr-3 font-medium">{t.users.locked}</th>
                    <th className="py-2.5 pr-3 font-medium">{t.common.actions}</th>
                  </tr>
                </thead>
                <tbody>
                  {paged.map(user => (
                    <tr key={user.id} className="border-b border-gray-100 dark:border-gray-800 hover:bg-gray-50 dark:hover:bg-gray-800/50">
                      <td className="py-2.5 pr-3 font-medium text-gray-900 dark:text-gray-100">{user.username}</td>
                      <td className="py-2.5 pr-3 text-gray-700 dark:text-gray-300">{user.display_name}</td>
                      <td className="py-2.5 pr-3">
                        <div className="flex flex-wrap gap-1">
                          {user.roles.map(role => (
                            <span key={role} className={role === 'admin' ? 'badge-indigo' : 'badge-gray'}>
                              {role}
                            </span>
                          ))}
                        </div>
                      </td>
                      <td className="py-2.5 pr-3">
                        <span className={user.status === 'active' ? 'badge-green' : 'badge-red'}>
                          {user.status === 'active' ? t.common.enabled : t.common.disabled}
                        </span>
                      </td>
                      <td className="py-2.5 pr-3">
                        {user.locked_until ? (
                          <span className="badge-yellow">{t.users.lockedYes}</span>
                        ) : (
                          <span className="text-gray-400">—</span>
                        )}
                      </td>
                      <td className="py-2.5 pr-3">
                        <div className="flex flex-wrap gap-1.5">
                          <button
                            className="btn-sm btn-secondary"
                            onClick={() => handleToggleStatus(user)}
                            disabled={statusMutation.isPending}
                          >
                            {user.status === 'active' ? t.users.disable : t.users.enable}
                          </button>
                          <button
                            className="btn-sm btn-secondary"
                            onClick={() => handleReset(user)}
                            disabled={resetMutation.isPending}
                          >
                            {t.users.resetPassword}
                          </button>
                          {user.locked_until && (
                            <button
                              className="btn-sm btn-secondary"
                              onClick={() => unlockMutation.mutate(user.id)}
                              disabled={unlockMutation.isPending}
                            >
                              {t.users.unlock}
                            </button>
                          )}
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>

            {/* Pagination */}
            {totalPages > 1 && (
              <div className="flex items-center justify-between pt-4 border-t border-gray-100 dark:border-gray-800 mt-4">
                <span className="text-sm text-gray-500">
                  {filtered.length} {t.users.totalUsers}
                </span>
                <div className="flex items-center gap-2">
                  <button
                    className="btn-sm btn-secondary"
                    onClick={() => setPage(p => Math.max(1, p - 1))}
                    disabled={safePage <= 1}
                  >
                    <ChevronLeft size={14} />
                  </button>
                  <span className="text-sm text-gray-600 dark:text-gray-400 min-w-[4rem] text-center">
                    {safePage} / {totalPages}
                  </span>
                  <button
                    className="btn-sm btn-secondary"
                    onClick={() => setPage(p => Math.min(totalPages, p + 1))}
                    disabled={safePage >= totalPages}
                  >
                    <ChevronRight size={14} />
                  </button>
                </div>
              </div>
            )}
          </>
        )}
      </div>

      {/* Create User Modal */}
      {showModal && (
        <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50 p-4">
          <div className="bg-white border border-gray-200 rounded-xl w-full max-w-lg dark:bg-gray-900 dark:border-gray-700">
            <div className="flex items-center justify-between px-6 py-4 border-b border-gray-200 dark:border-gray-700">
              <h3 className="font-semibold text-gray-800 dark:text-gray-100">{t.users.createUser}</h3>
              <button onClick={closeModal} className="text-gray-400 hover:text-gray-600 dark:hover:text-gray-300">✕</button>
            </div>
            <form onSubmit={handleCreate} className="px-6 py-5 space-y-4">
              {error && (
                <div className="px-3 py-2 bg-red-50 border border-red-300 rounded-lg text-red-600 text-sm dark:bg-red-900/50 dark:border-red-700 dark:text-red-300">
                  {error}
                </div>
              )}
              <div>
                <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">{t.auth.username}</label>
                <input
                  className="input w-full"
                  value={form.username}
                  onChange={e => setForm(v => ({ ...v, username: e.target.value }))}
                  required
                />
              </div>
              <div>
                <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">{t.auth.displayName}</label>
                <input
                  className="input w-full"
                  value={form.display_name}
                  onChange={e => setForm(v => ({ ...v, display_name: e.target.value }))}
                  required
                />
              </div>
              <div>
                <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">{t.auth.password}</label>
                <input
                  className="input w-full"
                  type="password"
                  value={form.password}
                  onChange={e => setForm(v => ({ ...v, password: e.target.value }))}
                  required
                />
              </div>
              <div className="flex justify-end gap-3 pt-2">
                <button type="button" className="btn-secondary" onClick={closeModal}>{t.common.cancel}</button>
                <button type="submit" className="btn-primary" disabled={createMutation.isPending}>
                  {createMutation.isPending ? t.users.creating : t.users.createUser}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  )
}
