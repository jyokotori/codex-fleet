import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Plus, Trash2, Edit2, Settings, ChevronDown, ChevronRight } from 'lucide-react'
import { codexConfigsApi, configsApi, type CodexConfig } from '../../lib/api'
import { useI18n } from '../../hooks/useI18n'

interface CodexConfigFormData {
  name: string
  config_toml: string
  auth_json: string
  config_toml_open: boolean
  auth_json_open: boolean
}

const defaultForm: CodexConfigFormData = {
  name: '',
  config_toml: '',
  auth_json: '',
  config_toml_open: true,
  auth_json_open: false,
}

export default function CodexConfigs() {
  const qc = useQueryClient()
  const { t } = useI18n()
  const [showModal, setShowModal] = useState(false)
  const [editConfig, setEditConfig] = useState<CodexConfig | null>(null)
  const [form, setForm] = useState<CodexConfigFormData>(defaultForm)
  const [loadingTemplate, setLoadingTemplate] = useState<'config_toml' | 'auth_json' | null>(null)

  const { data: configs = [], isLoading } = useQuery({
    queryKey: ['codex-configs'],
    queryFn: codexConfigsApi.list,
  })

  const createMutation = useMutation({
    mutationFn: (data: CodexConfigFormData) =>
      codexConfigsApi.create({
        name: data.name,
        config_toml: data.config_toml,
        auth_json: data.auth_json,
      }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['codex-configs'] })
      closeModal()
    },
  })

  const updateMutation = useMutation({
    mutationFn: ({ id, data }: { id: string; data: Partial<CodexConfigFormData> }) =>
      codexConfigsApi.update(id, {
        name: data.name,
        config_toml: data.config_toml,
        auth_json: data.auth_json,
      }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['codex-configs'] })
      closeModal()
    },
  })

  const deleteMutation = useMutation({
    mutationFn: codexConfigsApi.delete,
    onSuccess: () => qc.invalidateQueries({ queryKey: ['codex-configs'] }),
  })

  function openCreate() {
    setEditConfig(null)
    setForm(defaultForm)
    setShowModal(true)
  }

  function openEdit(config: CodexConfig) {
    setEditConfig(config)
    setForm({
      name: config.name,
      config_toml: config.config_toml,
      auth_json: config.auth_json,
      config_toml_open: !!config.config_toml,
      auth_json_open: !!config.auth_json,
    })
    setShowModal(true)
  }

  function closeModal() {
    setShowModal(false)
    setEditConfig(null)
    setForm(defaultForm)
  }

  async function handleLoadTemplate(section: 'config_toml' | 'auth_json') {
    const fileName = section === 'config_toml' ? 'config.toml' : 'auth.json'
    setLoadingTemplate(section)
    try {
      const result = await configsApi.getTemplate(`config_files/codex/${fileName}`)
      setForm(f => ({ ...f, [section]: result.content }))
    } catch {
      // ignore
    } finally {
      setLoadingTemplate(null)
    }
  }

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    if (editConfig) updateMutation.mutate({ id: editConfig.id, data: form })
    else createMutation.mutate(form)
  }

  const isPending = createMutation.isPending || updateMutation.isPending

  return (
    <div className="p-8">
      <div className="flex items-center justify-between mb-8">
        <div>
          <h1 className="text-2xl font-bold text-gray-900 dark:text-white">Codex</h1>
          <p className="text-gray-500 mt-1">{t.configs.subtitle}</p>
        </div>
        <button onClick={openCreate} className="btn-primary flex items-center gap-2">
          <Plus size={16} />{t.configs.newCodexConfig}
        </button>
      </div>

      {isLoading ? (
        <div className="text-center py-12 text-gray-500">{t.common.loading}</div>
      ) : configs.length === 0 ? (
        <div className="text-center py-12 card">
          <Settings size={40} className="mx-auto text-gray-400 dark:text-gray-600 mb-3" />
          <p className="text-gray-500 dark:text-gray-400">{t.configs.noCodexConfigs}</p>
          <p className="text-gray-400 dark:text-gray-600 text-sm mt-1">{t.configs.noCodexConfigsHint}</p>
        </div>
      ) : (
        <div className="grid gap-4">
          {configs.map(config => (
            <div key={config.id} className="card">
              <div className="flex items-start justify-between gap-4">
                <div className="flex items-center gap-3">
                  <div className="w-9 h-9 rounded-lg bg-gray-100 dark:bg-gray-700 flex items-center justify-center">
                    <Settings size={16} className="text-gray-500 dark:text-gray-400" />
                  </div>
                  <div>
                    <div className="flex items-center gap-2 flex-wrap">
                      <p className="font-medium text-gray-800 dark:text-gray-100">{config.name}</p>
                      {config.config_toml && (
                        <span className="badge badge-blue">config.toml ✓</span>
                      )}
                      {config.auth_json && (
                        <span className="badge badge-green">auth.json ✓</span>
                      )}
                    </div>
                    <p className="text-xs text-gray-500 mt-0.5">
                      {t.configs.updated} {new Date(config.updated_at).toLocaleDateString()}
                    </p>
                  </div>
                </div>
                <div className="flex gap-2">
                  <button onClick={() => openEdit(config)} className="btn-secondary btn-sm"><Edit2 size={13} /></button>
                  <button
                    onClick={() => { if (confirm(`${t.common.delete} "${config.name}"?`)) deleteMutation.mutate(config.id) }}
                    className="btn-danger btn-sm"
                  >
                    <Trash2 size={13} />
                  </button>
                </div>
              </div>
              {(config.config_toml || config.auth_json) && (
                <div className="mt-3 grid gap-2">
                  {config.config_toml && (
                    <div>
                      <p className="text-xs text-gray-400 mb-1">config.toml</p>
                      <pre className="p-3 bg-gray-100 dark:bg-gray-800 rounded-lg text-xs text-gray-600 dark:text-gray-400 overflow-auto max-h-24 border border-gray-200 dark:border-gray-700">
                        {config.config_toml.slice(0, 300)}{config.config_toml.length > 300 ? '...' : ''}
                      </pre>
                    </div>
                  )}
                  {config.auth_json && (
                    <div>
                      <p className="text-xs text-gray-400 mb-1">auth.json</p>
                      <pre className="p-3 bg-gray-100 dark:bg-gray-800 rounded-lg text-xs text-gray-600 dark:text-gray-400 overflow-auto max-h-24 border border-gray-200 dark:border-gray-700">
                        {config.auth_json.slice(0, 300)}{config.auth_json.length > 300 ? '...' : ''}
                      </pre>
                    </div>
                  )}
                </div>
              )}
            </div>
          ))}
        </div>
      )}

      {showModal && (
        <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50 p-4">
          <div className="bg-white border border-gray-200 rounded-xl w-full max-w-2xl max-h-[90vh] flex flex-col dark:bg-gray-900 dark:border-gray-700">
            <div className="flex items-center justify-between px-6 py-4 border-b border-gray-200 dark:border-gray-700">
              <h3 className="font-semibold text-gray-800 dark:text-gray-100">
                {editConfig ? t.configs.editCodexConfig : t.configs.newCodexConfig}
              </h3>
              <button onClick={closeModal} className="text-gray-400 hover:text-gray-700 dark:hover:text-gray-300">✕</button>
            </div>
            <form onSubmit={handleSubmit} className="flex flex-col flex-1 overflow-hidden">
              <div className="p-6 space-y-4 flex-1 overflow-y-auto">
                {/* Name */}
                <div>
                  <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.common.name}</label>
                  <input
                    className="input"
                    value={form.name}
                    onChange={e => setForm(f => ({ ...f, name: e.target.value }))}
                    placeholder="My Codex Config"
                    required
                  />
                </div>

                {/* config.toml section */}
                <div className="border border-gray-200 dark:border-gray-700 rounded-lg overflow-hidden">
                  <button
                    type="button"
                    onClick={() => setForm(f => ({ ...f, config_toml_open: !f.config_toml_open }))}
                    className="w-full flex items-center justify-between px-4 py-3 bg-gray-50 dark:bg-gray-800 text-sm font-medium text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors"
                  >
                    <div className="flex items-center gap-2">
                      {form.config_toml_open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
                      <span>{t.configs.configTomlSection}</span>
                      {form.config_toml && (
                        <span className="text-[10px] px-1.5 py-0.5 rounded bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400 font-medium">filled</span>
                      )}
                    </div>
                  </button>
                  {form.config_toml_open && (
                    <div className="p-4 space-y-3">
                      <div className="flex items-center gap-2">
                        <button
                          type="button"
                          onClick={() => handleLoadTemplate('config_toml')}
                          disabled={loadingTemplate === 'config_toml'}
                          className="text-xs text-sky-600 dark:text-sky-400 hover:underline disabled:opacity-50"
                        >
                          {loadingTemplate === 'config_toml' ? t.common.loading : t.configs.useDefaultTemplate}
                        </button>
                      </div>
                      <textarea
                        className="input h-48 resize-none font-mono text-sm"
                        value={form.config_toml}
                        onChange={e => setForm(f => ({ ...f, config_toml: e.target.value }))}
                        placeholder="# config.toml content"
                      />
                    </div>
                  )}
                </div>

                {/* auth.json section */}
                <div className="border border-gray-200 dark:border-gray-700 rounded-lg overflow-hidden">
                  <button
                    type="button"
                    onClick={() => setForm(f => ({ ...f, auth_json_open: !f.auth_json_open }))}
                    className="w-full flex items-center justify-between px-4 py-3 bg-gray-50 dark:bg-gray-800 text-sm font-medium text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors"
                  >
                    <div className="flex items-center gap-2">
                      {form.auth_json_open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
                      <span>{t.configs.authJsonSection}</span>
                      {form.auth_json && (
                        <span className="text-[10px] px-1.5 py-0.5 rounded bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400 font-medium">filled</span>
                      )}
                    </div>
                  </button>
                  {form.auth_json_open && (
                    <div className="p-4 space-y-3">
                      <div className="flex items-center gap-2">
                        <button
                          type="button"
                          onClick={() => handleLoadTemplate('auth_json')}
                          disabled={loadingTemplate === 'auth_json'}
                          className="text-xs text-sky-600 dark:text-sky-400 hover:underline disabled:opacity-50"
                        >
                          {loadingTemplate === 'auth_json' ? t.common.loading : t.configs.useDefaultTemplate}
                        </button>
                      </div>
                      <textarea
                        className="input h-48 resize-none font-mono text-sm"
                        value={form.auth_json}
                        onChange={e => setForm(f => ({ ...f, auth_json: e.target.value }))}
                        placeholder='{ "token": "..." }'
                      />
                    </div>
                  )}
                </div>

                {(createMutation.error || updateMutation.error) && (
                  <div className="text-red-500 dark:text-red-400 text-sm">
                    {String((createMutation.error || updateMutation.error)?.message)}
                  </div>
                )}
              </div>
              <div className="flex gap-3 justify-end p-6 border-t border-gray-200 dark:border-gray-700">
                <button type="button" onClick={closeModal} className="btn-secondary">{t.common.cancel}</button>
                <button type="submit" className="btn-primary" disabled={isPending}>
                  {isPending ? t.common.loading : editConfig ? t.common.update : t.common.create}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  )
}
