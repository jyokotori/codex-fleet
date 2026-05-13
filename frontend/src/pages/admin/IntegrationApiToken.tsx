import { useEffect, useState } from 'react'
import { Copy, RefreshCw, Check } from 'lucide-react'
import { apiTokenApi } from '../../lib/api'
import { useI18n } from '../../hooks/useI18n'

export default function IntegrationApiToken() {
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
    <div className="max-w-3xl mx-auto p-6">
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
    </div>
  )
}
