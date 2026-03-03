import { useState, FormEvent } from 'react'
import { useNavigate, Link } from 'react-router-dom'
import { Zap, Languages } from 'lucide-react'
import { authApi } from '../lib/api'
import { useI18n } from '../hooks/useI18n'

export default function Register() {
  const navigate = useNavigate()
  const { t, locale, setLocale } = useI18n()
  const [form, setForm] = useState({ username: '', display_name: '', password: '' })
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(false)

  async function handleSubmit(e: FormEvent) {
    e.preventDefault()
    setError('')
    setLoading(true)
    try {
      await authApi.register(form)
      navigate('/login')
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Registration failed')
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
        </div>

        <div className="card">
          <h2 className="text-lg font-semibold text-gray-800 dark:text-gray-100 mb-6">{t.auth.createAccount}</h2>

          {error && (
            <div className="mb-4 px-3 py-2 bg-red-50 border border-red-300 rounded-lg text-red-600 text-sm dark:bg-red-900/50 dark:border-red-700 dark:text-red-300">
              {error}
            </div>
          )}

          <form onSubmit={handleSubmit} className="space-y-4">
            <div>
              <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.auth.username}</label>
              <input className="input" value={form.username} onChange={e => setForm(f => ({ ...f, username: e.target.value }))} placeholder="johndoe" required />
            </div>
            <div>
              <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.auth.displayName}</label>
              <input className="input" value={form.display_name} onChange={e => setForm(f => ({ ...f, display_name: e.target.value }))} placeholder="John Doe" required />
            </div>
            <div>
              <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.auth.password}</label>
              <input type="password" className="input" value={form.password} onChange={e => setForm(f => ({ ...f, password: e.target.value }))} placeholder="••••••" required />
            </div>
            <button type="submit" className="btn-primary w-full" disabled={loading}>
              {loading ? t.auth.creating : t.auth.createAccount}
            </button>
          </form>

          <p className="mt-4 text-center text-sm text-gray-500">
            {t.auth.alreadyHaveAccount}{' '}
            <Link to="/login" className="text-sky-500 hover:underline">{t.auth.signIn}</Link>
          </p>
        </div>
      </div>
    </div>
  )
}
