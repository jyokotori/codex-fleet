import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Plus, Trash2, Edit2, Settings, ChevronDown, ChevronRight } from 'lucide-react'
import { claudeConfigsApi, type ClaudeConfig, type ClaudeConfigInput } from '../../lib/api'
import { useI18n } from '../../hooks/useI18n'

interface FormState {
  name: string
  anthropic_base_url: string
  anthropic_auth_token: string
  anthropic_model: string
  default_opus_model: string
  default_sonnet_model: string
  default_haiku_model: string
  subagent_model: string
  effort_level: string
  moreOpen: boolean
}

const DEFAULT_BASE_URL = 'https://api.anthropic.com'
const DEFAULT_MODEL = 'claude-opus-4-7[1m]'
const DEFAULT_EFFORT = 'xhigh'

function emptyForm(): FormState {
  return {
    name: '',
    anthropic_base_url: DEFAULT_BASE_URL,
    anthropic_auth_token: '',
    anthropic_model: DEFAULT_MODEL,
    default_opus_model: '',
    default_sonnet_model: '',
    default_haiku_model: '',
    subagent_model: '',
    effort_level: DEFAULT_EFFORT,
    moreOpen: true,
  }
}

function formFromConfig(c: ClaudeConfig): FormState {
  return {
    name: c.name,
    anthropic_base_url: c.anthropic_base_url || DEFAULT_BASE_URL,
    anthropic_auth_token: c.anthropic_auth_token, // backend returns masked
    anthropic_model: c.anthropic_model || DEFAULT_MODEL,
    default_opus_model: c.default_opus_model,
    default_sonnet_model: c.default_sonnet_model,
    default_haiku_model: c.default_haiku_model,
    subagent_model: c.subagent_model,
    effort_level: c.effort_level || DEFAULT_EFFORT,
    moreOpen: true,
  }
}

function toInput(form: FormState): ClaudeConfigInput & { name: string } {
  return {
    name: form.name,
    anthropic_base_url: form.anthropic_base_url,
    anthropic_auth_token: form.anthropic_auth_token,
    anthropic_model: form.anthropic_model,
    default_opus_model: form.default_opus_model,
    default_sonnet_model: form.default_sonnet_model,
    default_haiku_model: form.default_haiku_model,
    subagent_model: form.subagent_model,
    effort_level: form.effort_level,
  }
}

export default function ClaudeConfigs() {
  const qc = useQueryClient()
  const { t } = useI18n()
  const [showModal, setShowModal] = useState(false)
  const [editConfig, setEditConfig] = useState<ClaudeConfig | null>(null)
  const [form, setForm] = useState<FormState>(emptyForm())

  const { data: configs = [], isLoading } = useQuery({
    queryKey: ['claude-configs'],
    queryFn: claudeConfigsApi.list,
  })

  const createMutation = useMutation({
    mutationFn: (data: FormState) => claudeConfigsApi.create(toInput(data)),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['claude-configs'] })
      closeModal()
    },
  })

  const updateMutation = useMutation({
    mutationFn: ({ id, data }: { id: string; data: FormState }) =>
      claudeConfigsApi.update(id, toInput(data)),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['claude-configs'] })
      closeModal()
    },
  })

  const deleteMutation = useMutation({
    mutationFn: claudeConfigsApi.delete,
    onSuccess: () => qc.invalidateQueries({ queryKey: ['claude-configs'] }),
  })

  function openCreate() {
    setEditConfig(null)
    setForm(emptyForm())
    setShowModal(true)
  }

  function openEdit(c: ClaudeConfig) {
    setEditConfig(c)
    setForm(formFromConfig(c))
    setShowModal(true)
  }

  function closeModal() {
    setShowModal(false)
    setEditConfig(null)
    setForm(emptyForm())
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
          <h1 className="text-2xl font-bold text-gray-900 dark:text-white">Claude</h1>
          <p className="text-gray-500 mt-1">{t.configs.subtitle}</p>
        </div>
        <button onClick={openCreate} className="btn-primary flex items-center gap-2">
          <Plus size={16} />{t.configs.newClaudeConfig}
        </button>
      </div>

      {isLoading ? (
        <div className="text-center py-12 text-gray-500">{t.common.loading}</div>
      ) : configs.length === 0 ? (
        <div className="text-center py-12 card">
          <Settings size={40} className="mx-auto text-gray-400 dark:text-gray-600 mb-3" />
          <p className="text-gray-500 dark:text-gray-400">{t.configs.noClaudeConfigs}</p>
          <p className="text-gray-400 dark:text-gray-600 text-sm mt-1">{t.configs.noClaudeConfigsHint}</p>
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
                      <span className="badge badge-blue">{config.anthropic_model}</span>
                    </div>
                    <p className="text-xs text-gray-500 mt-0.5 font-mono">
                      {config.anthropic_base_url}
                    </p>
                    <p className="text-xs text-gray-400 mt-0.5">
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
            </div>
          ))}
        </div>
      )}

      {showModal && (
        <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50 p-4">
          <div className="bg-white border border-gray-200 rounded-xl w-full max-w-2xl max-h-[90vh] flex flex-col dark:bg-gray-900 dark:border-gray-700">
            <div className="flex items-center justify-between px-6 py-4 border-b border-gray-200 dark:border-gray-700">
              <h3 className="font-semibold text-gray-800 dark:text-gray-100">
                {editConfig ? t.configs.editClaudeConfig : t.configs.newClaudeConfig}
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
                    placeholder="My Claude Config"
                    required
                  />
                </div>

                {/* Core */}
                <div className="border border-gray-200 dark:border-gray-700 rounded-lg overflow-hidden">
                  <div className="px-4 py-2 bg-gray-50 dark:bg-gray-800 text-sm font-medium text-gray-700 dark:text-gray-300">
                    {t.configs.claudeCoreSection}
                  </div>
                  <div className="p-4 space-y-3">
                    <div>
                      <label className="block text-xs text-gray-600 dark:text-gray-400 mb-1 font-mono">ANTHROPIC_BASE_URL</label>
                      <input
                        className="input"
                        value={form.anthropic_base_url}
                        onChange={e => setForm(f => ({ ...f, anthropic_base_url: e.target.value }))}
                        placeholder={DEFAULT_BASE_URL}
                      />
                    </div>
                    <div>
                      <label className="block text-xs text-gray-600 dark:text-gray-400 mb-1 font-mono">ANTHROPIC_AUTH_TOKEN</label>
                      <input
                        className="input"
                        type="password"
                        value={form.anthropic_auth_token}
                        onChange={e => setForm(f => ({ ...f, anthropic_auth_token: e.target.value }))}
                        placeholder="sk-..."
                        autoComplete="off"
                      />
                    </div>
                    <div>
                      <label className="block text-xs text-gray-600 dark:text-gray-400 mb-1 font-mono">ANTHROPIC_MODEL</label>
                      <input
                        className="input"
                        value={form.anthropic_model}
                        onChange={e => setForm(f => ({ ...f, anthropic_model: e.target.value }))}
                        placeholder={DEFAULT_MODEL}
                      />
                    </div>
                  </div>
                </div>

                {/* More */}
                <div className="border border-gray-200 dark:border-gray-700 rounded-lg overflow-hidden">
                  <button
                    type="button"
                    onClick={() => setForm(f => ({ ...f, moreOpen: !f.moreOpen }))}
                    className="w-full flex items-center justify-between px-4 py-3 bg-gray-50 dark:bg-gray-800 text-sm font-medium text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors"
                  >
                    <div className="flex items-center gap-2">
                      {form.moreOpen ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
                      <span>{t.configs.claudeMoreSection}</span>
                    </div>
                  </button>
                  {form.moreOpen && (
                    <div className="p-4 space-y-3">
                      <div>
                        <label className="block text-xs text-gray-600 dark:text-gray-400 mb-1 font-mono">ANTHROPIC_DEFAULT_OPUS_MODEL</label>
                        <input
                          className="input"
                          value={form.default_opus_model}
                          onChange={e => setForm(f => ({ ...f, default_opus_model: e.target.value }))}
                        />
                      </div>
                      <div>
                        <label className="block text-xs text-gray-600 dark:text-gray-400 mb-1 font-mono">ANTHROPIC_DEFAULT_SONNET_MODEL</label>
                        <input
                          className="input"
                          value={form.default_sonnet_model}
                          onChange={e => setForm(f => ({ ...f, default_sonnet_model: e.target.value }))}
                        />
                      </div>
                      <div>
                        <label className="block text-xs text-gray-600 dark:text-gray-400 mb-1 font-mono">ANTHROPIC_DEFAULT_HAIKU_MODEL</label>
                        <input
                          className="input"
                          value={form.default_haiku_model}
                          onChange={e => setForm(f => ({ ...f, default_haiku_model: e.target.value }))}
                        />
                      </div>
                      <div>
                        <label className="block text-xs text-gray-600 dark:text-gray-400 mb-1 font-mono">CLAUDE_CODE_SUBAGENT_MODEL</label>
                        <input
                          className="input"
                          value={form.subagent_model}
                          onChange={e => setForm(f => ({ ...f, subagent_model: e.target.value }))}
                        />
                      </div>
                      <div>
                        <label className="block text-xs text-gray-600 dark:text-gray-400 mb-1 font-mono">CLAUDE_CODE_EFFORT_LEVEL</label>
                        <input
                          className="input"
                          value={form.effort_level}
                          onChange={e => setForm(f => ({ ...f, effort_level: e.target.value }))}
                          placeholder={DEFAULT_EFFORT}
                        />
                      </div>
                    </div>
                  )}
                </div>

                {/* Sandbox notice */}
                <div className="border border-gray-200 dark:border-gray-700 rounded-lg overflow-hidden">
                  <div className="px-4 py-2 bg-gray-50 dark:bg-gray-800 text-sm font-medium text-gray-700 dark:text-gray-300">
                    {t.configs.claudeSandboxSection}
                  </div>
                  <div className="p-4 flex items-start gap-3">
                    <input type="checkbox" checked disabled className="mt-0.5" />
                    <div>
                      <div className="font-mono text-sm text-gray-700 dark:text-gray-300">IS_SANDBOX=1</div>
                      <p className="text-xs text-gray-500 dark:text-gray-400 mt-1">{t.configs.claudeSandboxNote}</p>
                    </div>
                  </div>
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
