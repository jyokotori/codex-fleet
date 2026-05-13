import { useEffect, useState, type FormEvent } from 'react'
import { Plug, Save, Copy, RefreshCw, Check } from 'lucide-react'
import {
  apiTokenApi,
  integrationsApi,
  type IntegrationConfig,
} from '../../lib/api'
import { useI18n } from '../../hooks/useI18n'
import { useIntegrations } from '../../lib/integrations'

type Tab = 'dingtalk' | 'api_token'

interface DingTalkForm {
  app_key: string
  app_secret: string
  robot_code: string
}

const EMPTY_FORM: DingTalkForm = { app_key: '', app_secret: '', robot_code: '' }

export default function Integrations() {
  const { t } = useI18n()
  const [tab, setTab] = useState<Tab>('dingtalk')

  return (
    <div className="max-w-3xl mx-auto p-6 space-y-6">
      <div>
        <h1 className="text-2xl font-bold text-gray-900 dark:text-white flex items-center gap-2">
          <Plug size={22} className="text-sky-500" />
          {t.integrations.title}
        </h1>
        <p className="text-sm text-gray-500 dark:text-gray-400 mt-1">{t.integrations.subtitle}</p>
      </div>

      <div className="flex gap-1 border-b dark:border-gray-700">
        <TabButton active={tab === 'dingtalk'} onClick={() => setTab('dingtalk')}>
          {t.integrations.tabs.dingtalk}
        </TabButton>
        <TabButton active={tab === 'api_token'} onClick={() => setTab('api_token')}>
          {t.integrations.tabs.apiToken}
        </TabButton>
      </div>

      {tab === 'dingtalk' && <DingTalkPanel />}
      {tab === 'api_token' && <ApiTokenPanel />}
    </div>
  )
}

function TabButton({
  active,
  onClick,
  children,
}: {
  active: boolean
  onClick: () => void
  children: React.ReactNode
}) {
  return (
    <button
      onClick={onClick}
      className={`px-4 py-2 text-sm font-medium border-b-2 -mb-px transition-colors ${
        active
          ? 'border-sky-600 text-sky-600 dark:text-sky-400'
          : 'border-transparent text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200'
      }`}
    >
      {children}
    </button>
  )
}

function DingTalkPanel() {
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

            {error && <div className="text-sm text-red-600 dark:text-red-400">{error}</div>}

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
  )
}

function ApiTokenPanel() {
  const { t } = useI18n()
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [hasToken, setHasToken] = useState(false)
  const [preview, setPreview] = useState<string | null>(null)
  // Full raw token is only held in memory right after (re)generation.
  const [revealedToken, setRevealedToken] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)

  useEffect(() => {
    let cancelled = false
    apiTokenApi
      .get()
      .then(r => {
        if (cancelled) return
        setHasToken(r.has_token)
        setPreview(r.preview)
      })
      .catch(e => {
        if (!cancelled) setError(e instanceof Error ? e.message : t.common.error)
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [t])

  async function regenerate() {
    if (hasToken && !window.confirm(t.integrations.apiToken.regenerateConfirm)) return
    setBusy(true)
    setError('')
    try {
      const r = await apiTokenApi.regenerate()
      setRevealedToken(r.token)
      setPreview(r.preview)
      setHasToken(r.has_token)
    } catch (err) {
      setError(err instanceof Error ? err.message : t.common.error)
    } finally {
      setBusy(false)
    }
  }

  async function copy() {
    if (!revealedToken) return
    try {
      await navigator.clipboard.writeText(revealedToken)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1500)
    } catch {
      // ignore
    }
  }

  return (
    <section className="bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-800 rounded-xl">
      <header className="px-5 py-4 border-b border-gray-100 dark:border-gray-800">
        <h2 className="text-base font-semibold text-gray-900 dark:text-white">
          {t.integrations.apiToken.title}
        </h2>
        <p className="text-xs text-gray-500 dark:text-gray-400 mt-0.5">
          {t.integrations.apiToken.description}
        </p>
      </header>

      <div className="p-5 space-y-4">
        {loading ? (
          <div className="text-sm text-gray-500 dark:text-gray-400">{t.common.loading}</div>
        ) : revealedToken ? (
          <>
            <div>
              <span className="block text-xs font-medium text-gray-600 dark:text-gray-400 mb-1">
                {t.integrations.apiToken.tokenLabel}
              </span>
              <div className="flex gap-2">
                <input
                  type="text"
                  readOnly
                  value={revealedToken}
                  className="flex-1 px-3 py-2 rounded-lg border border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800 text-sm font-mono text-gray-900 dark:text-gray-100 focus:outline-none"
                />
                <button
                  type="button"
                  onClick={copy}
                  className="inline-flex items-center gap-1.5 px-3 py-2 rounded-lg border border-gray-200 dark:border-gray-700 text-sm text-gray-700 dark:text-gray-200 hover:bg-gray-50 dark:hover:bg-gray-800"
                  title={t.integrations.apiToken.copy}
                >
                  {copied ? <Check size={14} className="text-emerald-500" /> : <Copy size={14} />}
                  {copied ? t.integrations.apiToken.copied : t.integrations.apiToken.copy}
                </button>
              </div>
              <p className="text-xs text-amber-600 dark:text-amber-400 mt-2">
                {t.integrations.apiToken.oneTimeNotice}
              </p>
              <p className="text-xs text-gray-500 dark:text-gray-400 mt-1">
                {t.integrations.apiToken.usage}
              </p>
            </div>

            {error && <div className="text-sm text-red-600 dark:text-red-400">{error}</div>}
          </>
        ) : hasToken ? (
          <>
            <div>
              <span className="block text-xs font-medium text-gray-600 dark:text-gray-400 mb-1">
                {t.integrations.apiToken.previewLabel}
              </span>
              <input
                type="text"
                readOnly
                value={preview ?? ''}
                className="w-full px-3 py-2 rounded-lg border border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-800 text-sm font-mono text-gray-500 dark:text-gray-400 focus:outline-none"
              />
              <p className="text-xs text-gray-500 dark:text-gray-400 mt-2">
                {t.integrations.apiToken.hiddenNotice}
              </p>
              <p className="text-xs text-gray-500 dark:text-gray-400 mt-1">
                {t.integrations.apiToken.usage}
              </p>
            </div>

            {error && <div className="text-sm text-red-600 dark:text-red-400">{error}</div>}

            <div className="flex justify-end">
              <button
                type="button"
                onClick={regenerate}
                disabled={busy}
                className="inline-flex items-center gap-1.5 px-4 py-2 rounded-lg bg-sky-600 hover:bg-sky-700 text-white text-sm font-medium disabled:opacity-60"
              >
                <RefreshCw size={14} className={busy ? 'animate-spin' : undefined} />
                {t.integrations.apiToken.regenerate}
              </button>
            </div>
          </>
        ) : (
          <>
            <p className="text-sm text-gray-500 dark:text-gray-400">
              {t.integrations.apiToken.empty}
            </p>
            {error && <div className="text-sm text-red-600 dark:text-red-400">{error}</div>}
            <div className="flex justify-end">
              <button
                type="button"
                onClick={regenerate}
                disabled={busy}
                className="inline-flex items-center gap-1.5 px-4 py-2 rounded-lg bg-sky-600 hover:bg-sky-700 text-white text-sm font-medium disabled:opacity-60"
              >
                <RefreshCw size={14} className={busy ? 'animate-spin' : undefined} />
                {t.integrations.apiToken.generate}
              </button>
            </div>
          </>
        )}
      </div>
    </section>
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
