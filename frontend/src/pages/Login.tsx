import { useState, FormEvent } from 'react'
import { useNavigate } from 'react-router-dom'
import { Zap, Languages } from 'lucide-react'
import { authApi } from '../lib/api'
import { saveAuth } from '../lib/auth'
import { useI18n } from '../hooks/useI18n'

export default function Login() {
  const navigate = useNavigate()
  const { t, locale, setLocale } = useI18n()
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(false)

  async function handleSubmit(e: FormEvent) {
    e.preventDefault()
    setError('')
    setLoading(true)
    try {
      const data = await authApi.login(username, password)
      saveAuth(data)
      navigate('/')
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : t.auth.signIn + ' failed')
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="min-h-screen flex items-center justify-center bg-gray-50 dark:bg-gray-950 p-4">
      <button
        onClick={() => setLocale(locale === 'en' ? 'zh' : 'en')}
        className="absolute top-4 right-4 flex items-center gap-1.5 text-sm text-gray-500 hover:text-gray-700 dark:hover:text-gray-300 transition-colors"
        title="Switch language / 切换语言"
      >
        <Languages size={15} />
        {locale === 'en' ? '中文' : 'English'}
      </button>

      <div className="w-full max-w-sm">
        <div className="text-center mb-8">
          <div className="flex justify-center mb-3">
            <div className="w-12 h-12 bg-sky-600 rounded-xl flex items-center justify-center">
              <Zap size={24} className="text-white" />
            </div>
          </div>
          <h1 className="text-2xl font-bold text-gray-900 dark:text-white">Codex Fleet</h1>
          <p className="text-gray-500 mt-1 text-sm">Multi-VM Agent Management</p>
        </div>

        <div className="card">
          <h2 className="text-lg font-semibold text-gray-800 dark:text-gray-100 mb-6">{t.auth.signIn}</h2>

          {error && (
            <div className="mb-4 px-3 py-2 bg-red-50 border border-red-300 rounded-lg text-red-600 text-sm dark:bg-red-900/50 dark:border-red-700 dark:text-red-300">
              {error}
            </div>
          )}

          <form onSubmit={handleSubmit} className="space-y-4">
            <div>
              <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.auth.username}</label>
              <input
                type="text"
                className="input"
                value={username}
                onChange={e => setUsername(e.target.value)}
                placeholder="codex"
                autoFocus
                required
              />
            </div>
            <div>
              <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.auth.password}</label>
              <input
                type="password"
                className="input"
                value={password}
                onChange={e => setPassword(e.target.value)}
                placeholder="••••••"
                required
              />
            </div>
            <button type="submit" className="btn-primary w-full" disabled={loading}>
              {loading ? t.auth.signingIn : t.auth.signIn}
            </button>
          </form>

        </div>

        <p className="text-center text-xs text-gray-400 dark:text-gray-600 mt-4">{t.auth.defaultHint}</p>
      </div>
    </div>
  )
}
