import { FormEvent, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { adminUsersApi, type AdminUser } from '../../lib/api'
import { useI18n } from '../../hooks/useI18n'

export default function Users() {
  const { t } = useI18n()
  const queryClient = useQueryClient()

  const [form, setForm] = useState({ username: '', display_name: '', password: '' })
  const [error, setError] = useState('')

  const { data: users = [], isLoading } = useQuery({
    queryKey: ['admin-users'],
    queryFn: adminUsersApi.list,
  })

  const createMutation = useMutation({
    mutationFn: adminUsersApi.create,
    onSuccess: () => {
      setForm({ username: '', display_name: '', password: '' })
      setError('')
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

  return (
    <div className="p-8 space-y-6">
      <div>
        <h1 className="text-2xl font-bold text-gray-900 dark:text-white">{t.users.title}</h1>
        <p className="text-gray-500 mt-1">{t.users.subtitle}</p>
      </div>

      <div className="card">
        <h2 className="text-lg font-semibold text-gray-800 dark:text-gray-100 mb-4">{t.users.createUser}</h2>
        {error && (
          <div className="mb-4 px-3 py-2 bg-red-50 border border-red-300 rounded-lg text-red-600 text-sm dark:bg-red-900/50 dark:border-red-700 dark:text-red-300">
            {error}
          </div>
        )}
        <form onSubmit={handleCreate} className="grid grid-cols-1 md:grid-cols-4 gap-3">
          <input
            className="input"
            placeholder={t.auth.username}
            value={form.username}
            onChange={e => setForm(v => ({ ...v, username: e.target.value }))}
            required
          />
          <input
            className="input"
            placeholder={t.auth.displayName}
            value={form.display_name}
            onChange={e => setForm(v => ({ ...v, display_name: e.target.value }))}
            required
          />
          <input
            className="input"
            type="password"
            placeholder={t.auth.password}
            value={form.password}
            onChange={e => setForm(v => ({ ...v, password: e.target.value }))}
            required
          />
          <button type="submit" className="btn-primary" disabled={createMutation.isPending}>
            {createMutation.isPending ? t.users.creating : t.users.createUser}
          </button>
        </form>
      </div>

      <div className="card overflow-auto">
        <h2 className="text-lg font-semibold text-gray-800 dark:text-gray-100 mb-4">{t.users.userList}</h2>
        {isLoading ? (
          <p className="text-gray-500 text-sm">{t.common.loading}</p>
        ) : users.length === 0 ? (
          <p className="text-gray-500 text-sm">{t.common.noData}</p>
        ) : (
          <table className="w-full text-sm">
            <thead>
              <tr className="text-left text-gray-500 border-b border-gray-200 dark:border-gray-700">
                <th className="py-2 pr-3">{t.auth.username}</th>
                <th className="py-2 pr-3">{t.auth.displayName}</th>
                <th className="py-2 pr-3">{t.users.roles}</th>
                <th className="py-2 pr-3">{t.common.status}</th>
                <th className="py-2 pr-3">{t.users.locked}</th>
                <th className="py-2 pr-3">{t.common.actions}</th>
              </tr>
            </thead>
            <tbody>
              {users.map(user => (
                <tr key={user.id} className="border-b border-gray-100 dark:border-gray-800">
                  <td className="py-2 pr-3">{user.username}</td>
                  <td className="py-2 pr-3">{user.display_name}</td>
                  <td className="py-2 pr-3">{user.roles.join(', ')}</td>
                  <td className="py-2 pr-3">{user.status}</td>
                  <td className="py-2 pr-3">{user.locked_until ? t.users.lockedYes : t.users.lockedNo}</td>
                  <td className="py-2 pr-3 flex flex-wrap gap-2">
                    <button
                      className="btn-secondary"
                      onClick={() => handleToggleStatus(user)}
                      disabled={statusMutation.isPending}
                    >
                      {user.status === 'active' ? t.users.disable : t.users.enable}
                    </button>
                    <button
                      className="btn-secondary"
                      onClick={() => handleReset(user)}
                      disabled={resetMutation.isPending}
                    >
                      {t.users.resetPassword}
                    </button>
                    <button
                      className="btn-secondary"
                      onClick={() => unlockMutation.mutate(user.id)}
                      disabled={unlockMutation.isPending}
                    >
                      {t.users.unlock}
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  )
}
