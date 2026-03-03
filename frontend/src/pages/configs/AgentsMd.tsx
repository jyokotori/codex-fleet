import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Plus, Trash2, Edit2, Bot } from 'lucide-react'
import { configsApi, type CompanyConfig } from '../../lib/api'
import { useI18n } from '../../hooks/useI18n'

interface ConfigFormData {
  name: string
  content: string
}

const defaultForm: ConfigFormData = { name: '', content: '' }

export default function AgentsMd() {
  const qc = useQueryClient()
  const { t } = useI18n()
  const [showModal, setShowModal] = useState(false)
  const [editConfig, setEditConfig] = useState<CompanyConfig | null>(null)
  const [form, setForm] = useState<ConfigFormData>(defaultForm)
  const [loadingTemplate, setLoadingTemplate] = useState(false)

  const { data: configs = [], isLoading } = useQuery({
    queryKey: ['configs', 'agents_md'],
    queryFn: () => configsApi.list({ category: 'agents_md' }),
  })

  const createMutation = useMutation({
    mutationFn: (data: ConfigFormData) =>
      configsApi.create({
        name: data.name,
        category: 'agents_md',
        cli_type: 'codex',
        content: data.content,
      }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['configs'] })
      closeModal()
    },
  })

  const updateMutation = useMutation({
    mutationFn: ({ id, data }: { id: string; data: Partial<ConfigFormData> }) =>
      configsApi.update(id, data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['configs'] })
      closeModal()
    },
  })

  const deleteMutation = useMutation({
    mutationFn: configsApi.delete,
    onSuccess: () => qc.invalidateQueries({ queryKey: ['configs'] }),
  })

  function openCreate() {
    setEditConfig(null)
    setForm(defaultForm)
    setShowModal(true)
  }

  function openEdit(config: CompanyConfig) {
    setEditConfig(config)
    setForm({ name: config.name, content: config.content })
    setShowModal(true)
  }

  function closeModal() {
    setShowModal(false)
    setEditConfig(null)
    setForm(defaultForm)
  }

  async function handleUseTemplate(checked: boolean) {
    if (!checked) return
    setLoadingTemplate(true)
    try {
      const result = await configsApi.getTemplate('agents_md/AGENTS.md')
      setForm(f => ({ ...f, content: result.content }))
    } catch {
      // ignore
    } finally {
      setLoadingTemplate(false)
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
          <h1 className="text-2xl font-bold text-gray-900 dark:text-white">AGENTS.md</h1>
          <p className="text-gray-500 mt-1">{t.configs.subtitle}</p>
        </div>
        <button onClick={openCreate} className="btn-primary flex items-center gap-2">
          <Plus size={16} />{t.configs.newAgentsMd}
        </button>
      </div>

      {isLoading ? (
        <div className="text-center py-12 text-gray-500">{t.common.loading}</div>
      ) : configs.length === 0 ? (
        <div className="text-center py-12 card">
          <Bot size={40} className="mx-auto text-gray-400 dark:text-gray-600 mb-3" />
          <p className="text-gray-500 dark:text-gray-400">{t.configs.noConfigs}</p>
          <p className="text-gray-400 dark:text-gray-600 text-sm mt-1">{t.configs.noConfigsHint}</p>
        </div>
      ) : (
        <div className="grid gap-4">
          {configs.map(config => (
            <div key={config.id} className="card">
              <div className="flex items-start justify-between gap-4">
                <div className="flex items-center gap-3">
                  <div className="w-9 h-9 rounded-lg bg-gray-100 dark:bg-gray-700 flex items-center justify-center">
                    <Bot size={16} className="text-gray-500 dark:text-gray-400" />
                  </div>
                  <div>
                    <p className="font-medium text-gray-800 dark:text-gray-100">{config.name}</p>
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
              <pre className="mt-3 p-3 bg-gray-100 dark:bg-gray-800 rounded-lg text-xs text-gray-600 dark:text-gray-400 overflow-auto max-h-32 border border-gray-200 dark:border-gray-700">
                {config.content.slice(0, 500)}{config.content.length > 500 ? '...' : ''}
              </pre>
            </div>
          ))}
        </div>
      )}

      {showModal && (
        <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50 p-4">
          <div className="bg-white border border-gray-200 rounded-xl w-full max-w-2xl max-h-[90vh] flex flex-col dark:bg-gray-900 dark:border-gray-700">
            <div className="flex items-center justify-between px-6 py-4 border-b border-gray-200 dark:border-gray-700">
              <h3 className="font-semibold text-gray-800 dark:text-gray-100">
                {editConfig ? t.configs.editConfig : t.configs.newAgentsMd}
              </h3>
              <button onClick={closeModal} className="text-gray-400 hover:text-gray-700 dark:hover:text-gray-300">✕</button>
            </div>
            <form onSubmit={handleSubmit} className="flex flex-col flex-1 overflow-hidden">
              <div className="p-6 space-y-4 flex-1 overflow-y-auto">
                <div>
                  <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.common.name}</label>
                  <input
                    className="input"
                    value={form.name}
                    onChange={e => setForm(f => ({ ...f, name: e.target.value }))}
                    placeholder="My Project AGENTS.md"
                    required
                  />
                </div>
                <div className="flex items-center gap-2">
                  <input
                    id="use-template-agents"
                    type="checkbox"
                    className="rounded border-gray-300 dark:border-gray-600"
                    onChange={e => handleUseTemplate(e.target.checked)}
                    disabled={loadingTemplate}
                  />
                  <label htmlFor="use-template-agents" className="text-sm text-gray-600 dark:text-gray-400 cursor-pointer select-none">
                    {loadingTemplate ? t.common.loading : t.configs.useDefaultTemplate}
                  </label>
                </div>
                <div>
                  <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.configs.configContent}</label>
                  <textarea
                    className="input h-64 resize-none font-mono text-sm"
                    value={form.content}
                    onChange={e => setForm(f => ({ ...f, content: e.target.value }))}
                    placeholder="# AGENTS.md&#10;&#10;Guidelines for AI agents..."
                    required
                  />
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
