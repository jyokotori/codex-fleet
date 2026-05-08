import { useEffect, useState, type FormEvent } from 'react'
import { Plug, Save } from 'lucide-react'
import { integrationsApi, type IntegrationConfig } from '../../lib/api'
import { useI18n } from '../../hooks/useI18n'
import { useIntegrations } from '../../lib/integrations'

interface DingTalkForm {
  app_key: string
  app_secret: string
  robot_code: string
}

const EMPTY_FORM: DingTalkForm = { app_key: '', app_secret: '', robot_code: '' }

export default function Integrations() {
  const { t } = useI18n()
  const { refresh } = useIntegrations()
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState('')
  const [savedAt, setSavedAt] = useState<number | null>(null)
  const [form, setForm] = useState<DingTalkForm>(EMPTY_FORM)
  const [enabled, setEnabled] = useState(false)

  useEffect(() => {
    let cancelled = false
    integrationsApi
      .get('dingtalk')
      .then((cfg: IntegrationConfig) => {
        if (cancelled) return
        const c = (cfg.config ?? {}) as Partial<DingTalkForm>
        setForm({
          app_key: c.app_key ?? '',
          app_secret: c.app_secret ?? '',
          robot_code: c.robot_code ?? '',
        })
        setEnabled(cfg.enabled)
      })
      .catch((e: unknown) => {
        setError(e instanceof Error ? e.message : t.common.error)
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [t])

  async function handleSubmit(e: FormEvent) {
    e.preventDefault()
    setSaving(true)
    setError('')
    try {
      const config: Record<string, unknown> = {
        app_key: form.app_key.trim(),
        app_secret: form.app_secret.trim(),
        robot_code: form.robot_code.trim(),
      }
      const updated = await integrationsApi.upsert('dingtalk', { config, enabled })
      const c = (updated.config ?? {}) as Partial<DingTalkForm>
      setForm({
        app_key: c.app_key ?? '',
        app_secret: c.app_secret ?? '',
        robot_code: c.robot_code ?? '',
      })
      setEnabled(updated.enabled)
      setSavedAt(Date.now())
      await refresh()
    } catch (err) {
      setError(err instanceof Error ? err.message : t.common.error)
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="max-w-3xl mx-auto p-6 space-y-6">
      <div>
        <h1 className="text-2xl font-bold text-gray-900 dark:text-white flex items-center gap-2">
          <Plug size={22} className="text-sky-500" />
          {t.integrations.title}
        </h1>
        <p className="text-sm text-gray-500 dark:text-gray-400 mt-1">{t.integrations.subtitle}</p>
      </div>

      <section className="bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-800 rounded-xl">
        <header className="flex items-center justify-between px-5 py-4 border-b border-gray-100 dark:border-gray-800">
          <div>
            <h2 className="text-base font-semibold text-gray-900 dark:text-white">
              {t.integrations.providers.dingtalk}
            </h2>
            <p className="text-xs text-gray-500 dark:text-gray-400 mt-0.5">
              {t.integrations.dingtalk.description}
            </p>
          </div>
          <label className="flex items-center gap-2 text-sm text-gray-700 dark:text-gray-300 cursor-pointer">
            <input
              type="checkbox"
              checked={enabled}
              onChange={e => setEnabled(e.target.checked)}
              className="w-4 h-4 accent-sky-600"
            />
            {t.integrations.enabled}
          </label>
        </header>

        <form onSubmit={handleSubmit} className="p-5 space-y-4">
          {loading ? (
            <div className="text-sm text-gray-500 dark:text-gray-400">{t.common.loading}</div>
          ) : (
            <>
              <Field label={t.integrations.dingtalk.appKey}>
                <input
                  type="text"
                  value={form.app_key}
                  onChange={e => setForm({ ...form, app_key: e.target.value })}
                  className="w-full px-3 py-2 rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 text-sm text-gray-900 dark:text-gray-100 focus:outline-none focus:border-sky-500"
                  autoComplete="off"
                />
              </Field>
              <Field label={t.integrations.dingtalk.appSecret}>
                <input
                  type="text"
                  value={form.app_secret}
                  onChange={e => setForm({ ...form, app_secret: e.target.value })}
                  className="w-full px-3 py-2 rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 text-sm text-gray-900 dark:text-gray-100 focus:outline-none focus:border-sky-500 font-mono"
                  autoComplete="off"
                />
              </Field>
              <Field label={t.integrations.dingtalk.robotCode}>
                <input
                  type="text"
                  value={form.robot_code}
                  onChange={e => setForm({ ...form, robot_code: e.target.value })}
                  className="w-full px-3 py-2 rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 text-sm text-gray-900 dark:text-gray-100 focus:outline-none focus:border-sky-500"
                  autoComplete="off"
                />
              </Field>

              {error && (
                <div className="text-sm text-red-600 dark:text-red-400">{error}</div>
              )}

              <div className="flex items-center justify-end gap-3 pt-2">
                {savedAt && !error && (
                  <span className="text-xs text-emerald-600 dark:text-emerald-400">
                    {t.integrations.saved}
                  </span>
                )}
                <button
                  type="submit"
                  disabled={saving}
                  className="inline-flex items-center gap-1.5 px-4 py-2 rounded-lg bg-sky-600 hover:bg-sky-700 text-white text-sm font-medium disabled:opacity-60"
                >
                  <Save size={14} />
                  {t.integrations.save}
                </button>
              </div>
            </>
          )}
        </form>
      </section>
    </div>
  )
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="block">
      <span className="block text-xs font-medium text-gray-600 dark:text-gray-400 mb-1">
        {label}
      </span>
      {children}
    </label>
  )
}
