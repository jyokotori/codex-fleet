import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Plus, Trash2, Edit2, Copy, Container, X } from 'lucide-react'
import { dockerConfigsApi, type DockerConfig, type PortMapping, type EnvVar } from '../../lib/api'
import { useI18n } from '../../hooks/useI18n'

// ─── Form state ─────────────────────────────────────────────────────────────

interface FormState {
  name: string
  port_mappings: PortMapping[]
  env_vars: EnvVar[]
  init_script: string
}

const emptyForm = (): FormState => ({
  name: '',
  port_mappings: [],
  env_vars: [],
  init_script: '',
})

const fromConfig = (c: DockerConfig): FormState => ({
  name: c.name,
  port_mappings: c.port_mappings,
  env_vars: c.env_vars,
  init_script: c.init_script,
})

// ─── Sub-editors ─────────────────────────────────────────────────────────────

function PortMappingsEditor({
  rows,
  onChange,
  t,
}: {
  rows: PortMapping[]
  onChange: (v: PortMapping[]) => void
  t: ReturnType<typeof useI18n>['t']
}) {
  function add() {
    onChange([...rows, { host_port: '', container_port: '', protocol: 'tcp' }])
  }
  function remove(i: number) {
    onChange(rows.filter((_, idx) => idx !== i))
  }
  function update(i: number, field: keyof PortMapping, val: string) {
    onChange(rows.map((r, idx) => (idx === i ? { ...r, [field]: val } : r)))
  }

  return (
    <div>
      <div className="flex items-center justify-between mb-2">
        <span className="text-sm font-medium text-gray-700 dark:text-gray-300">{t.configs.portMappings}</span>
        <button type="button" onClick={add} className="text-xs text-sky-600 dark:text-sky-400 hover:underline">
          {t.configs.addPort}
        </button>
      </div>
      {rows.length > 0 && (
        <div className="rounded-lg border border-gray-200 dark:border-gray-700 overflow-hidden">
          <table className="w-full text-sm">
            <thead className="bg-gray-50 dark:bg-gray-800">
              <tr>
                <th className="px-3 py-2 text-left text-xs font-medium text-gray-500 dark:text-gray-400 w-[34%]">{t.configs.hostPort}</th>
                <th className="px-3 py-2 text-left text-xs font-medium text-gray-500 dark:text-gray-400 w-[34%]">{t.configs.containerPort}</th>
                <th className="px-3 py-2 text-left text-xs font-medium text-gray-500 dark:text-gray-400 w-[22%]">{t.configs.protocol}</th>
                <th className="w-8" />
              </tr>
            </thead>
            <tbody className="divide-y divide-gray-100 dark:divide-gray-700">
              {rows.map((row, i) => (
                <tr key={i} className="bg-white dark:bg-gray-900">
                  <td className="px-2 py-1.5">
                    <input
                      className="input py-1 text-sm w-full"
                      placeholder="8080"
                      value={row.host_port}
                      onChange={e => update(i, 'host_port', e.target.value)}
                    />
                  </td>
                  <td className="px-2 py-1.5">
                    <input
                      className="input py-1 text-sm w-full"
                      placeholder="80"
                      value={row.container_port}
                      onChange={e => update(i, 'container_port', e.target.value)}
                    />
                  </td>
                  <td className="px-2 py-1.5">
                    <select
                      className="input py-1 text-sm w-full"
                      value={row.protocol}
                      onChange={e => update(i, 'protocol', e.target.value as 'tcp' | 'udp')}
                    >
                      <option value="tcp">TCP</option>
                      <option value="udp">UDP</option>
                    </select>
                  </td>
                  <td className="px-2 py-1.5 text-center">
                    <button type="button" onClick={() => remove(i)} className="text-gray-400 hover:text-red-500 dark:hover:text-red-400">
                      <X size={14} />
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}

function EnvVarsEditor({
  rows,
  onChange,
  t,
}: {
  rows: EnvVar[]
  onChange: (v: EnvVar[]) => void
  t: ReturnType<typeof useI18n>['t']
}) {
  function add() {
    onChange([...rows, { key: '', value: '' }])
  }
  function remove(i: number) {
    onChange(rows.filter((_, idx) => idx !== i))
  }
  function update(i: number, field: keyof EnvVar, val: string) {
    onChange(rows.map((r, idx) => (idx === i ? { ...r, [field]: val } : r)))
  }

  return (
    <div>
      <div className="flex items-center justify-between mb-2">
        <span className="text-sm font-medium text-gray-700 dark:text-gray-300">{t.configs.envVars}</span>
        <button type="button" onClick={add} className="text-xs text-sky-600 dark:text-sky-400 hover:underline">
          {t.configs.addEnvVar}
        </button>
      </div>
      {rows.length > 0 && (
        <div className="rounded-lg border border-gray-200 dark:border-gray-700 overflow-hidden">
          <table className="w-full text-sm">
            <thead className="bg-gray-50 dark:bg-gray-800">
              <tr>
                <th className="px-3 py-2 text-left text-xs font-medium text-gray-500 dark:text-gray-400 w-[42%]">Key</th>
                <th className="px-3 py-2 text-left text-xs font-medium text-gray-500 dark:text-gray-400">Value</th>
                <th className="w-8" />
              </tr>
            </thead>
            <tbody className="divide-y divide-gray-100 dark:divide-gray-700">
              {rows.map((row, i) => (
                <tr key={i} className="bg-white dark:bg-gray-900">
                  <td className="px-2 py-1.5">
                    <input
                      className="input py-1 text-sm w-full font-mono"
                      placeholder="NODE_ENV"
                      value={row.key}
                      onChange={e => update(i, 'key', e.target.value)}
                    />
                  </td>
                  <td className="px-2 py-1.5">
                    <input
                      className="input py-1 text-sm w-full font-mono"
                      placeholder="production"
                      value={row.value}
                      onChange={e => update(i, 'value', e.target.value)}
                    />
                  </td>
                  <td className="px-2 py-1.5 text-center">
                    <button type="button" onClick={() => remove(i)} className="text-gray-400 hover:text-red-500 dark:hover:text-red-400">
                      <X size={14} />
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}

// ─── Summary helpers ─────────────────────────────────────────────────────────

function ConfigSummary({ config }: { config: DockerConfig }) {
  const ports = config.port_mappings
  const envs = config.env_vars

  return (
    <div className="mt-3 flex flex-wrap gap-x-5 gap-y-1 text-xs text-gray-500 dark:text-gray-400">
      {ports.length > 0 && (
        <span>
          <span className="text-gray-400 dark:text-gray-500">Ports: </span>
          {ports.slice(0, 3).map(p => `${p.host_port}:${p.container_port}`).join(', ')}
          {ports.length > 3 && ` +${ports.length - 3}`}
        </span>
      )}
      {envs.length > 0 && (
        <span>
          <span className="text-gray-400 dark:text-gray-500">Env: </span>
          {envs.length} var{envs.length !== 1 ? 's' : ''}
        </span>
      )}
      {config.init_script.trim() && (
        <span>
          <span className="text-gray-400 dark:text-gray-500">Init script: </span>
          {config.init_script.split('\n').filter(Boolean).length} line{config.init_script.split('\n').filter(Boolean).length !== 1 ? 's' : ''}
        </span>
      )}
      {ports.length === 0 && envs.length === 0 && !config.init_script.trim() && (
        <span className="italic">empty config</span>
      )}
    </div>
  )
}

// ─── Main page ───────────────────────────────────────────────────────────────

export default function DockerConfigs() {
  const qc = useQueryClient()
  const { t } = useI18n()
  const [showModal, setShowModal] = useState(false)
  const [editConfig, setEditConfig] = useState<DockerConfig | null>(null)
  const [form, setForm] = useState<FormState>(emptyForm())

  const { data: configs = [], isLoading } = useQuery({
    queryKey: ['docker-configs'],
    queryFn: dockerConfigsApi.list,
  })

  const createMutation = useMutation({
    mutationFn: (data: FormState) => dockerConfigsApi.create(data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['docker-configs'] })
      closeModal()
    },
  })

  const updateMutation = useMutation({
    mutationFn: ({ id, data }: { id: string; data: FormState }) => dockerConfigsApi.update(id, data),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['docker-configs'] })
      closeModal()
    },
  })

  const deleteMutation = useMutation({
    mutationFn: dockerConfigsApi.delete,
    onSuccess: () => qc.invalidateQueries({ queryKey: ['docker-configs'] }),
  })

  function openCreate() {
    setEditConfig(null)
    setForm(emptyForm())
    setShowModal(true)
  }

  function openEdit(config: DockerConfig) {
    setEditConfig(config)
    setForm(fromConfig(config))
    setShowModal(true)
  }

  function openCopy(config: DockerConfig) {
    setEditConfig(null)
    setForm({ ...fromConfig(config), name: `${config.name} (copy)` })
    setShowModal(true)
  }

  function closeModal() {
    setShowModal(false)
    setEditConfig(null)
    setForm(emptyForm())
  }

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    if (editConfig) {
      updateMutation.mutate({ id: editConfig.id, data: form })
    } else {
      createMutation.mutate(form)
    }
  }

  const isPending = createMutation.isPending || updateMutation.isPending

  return (
    <div className="p-8">
      <div className="flex items-center justify-between mb-8">
        <div>
          <h1 className="text-2xl font-bold text-gray-900 dark:text-white">Docker</h1>
          <p className="text-gray-500 mt-1">{t.configs.dockerSubtitle}</p>
        </div>
        <button onClick={openCreate} className="btn-primary flex items-center gap-2">
          <Plus size={16} />{t.configs.newDockerConfig}
        </button>
      </div>

      {isLoading ? (
        <div className="text-center py-12 text-gray-500">{t.common.loading}</div>
      ) : configs.length === 0 ? (
        <div className="text-center py-12 card">
          <Container size={40} className="mx-auto text-gray-400 dark:text-gray-600 mb-3" />
          <p className="text-gray-500 dark:text-gray-400">{t.configs.noDockerConfigs}</p>
          <p className="text-gray-400 dark:text-gray-600 text-sm mt-1">{t.configs.noDockerConfigsHint}</p>
        </div>
      ) : (
        <div className="grid gap-4">
          {configs.map(config => (
            <div key={config.id} className="card">
              <div className="flex items-start justify-between gap-4">
                <div className="flex items-center gap-3">
                  <div className="w-9 h-9 rounded-lg bg-blue-50 dark:bg-blue-900/20 flex items-center justify-center shrink-0">
                    <Container size={16} className="text-blue-500 dark:text-blue-400" />
                  </div>
                  <div>
                    <p className="font-medium text-gray-800 dark:text-gray-100">{config.name}</p>
                    <p className="text-xs text-gray-500 mt-0.5">
                      {t.configs.updated} {new Date(config.updated_at).toLocaleDateString()}
                    </p>
                  </div>
                </div>
                <div className="flex gap-2 shrink-0">
                  <button
                    title={t.configs.copyConfig}
                    onClick={() => openCopy(config)}
                    className="btn-secondary btn-sm"
                  >
                    <Copy size={13} />
                  </button>
                  <button onClick={() => openEdit(config)} className="btn-secondary btn-sm">
                    <Edit2 size={13} />
                  </button>
                  <button
                    onClick={() => {
                      if (confirm(`${t.common.delete} "${config.name}"?`)) deleteMutation.mutate(config.id)
                    }}
                    className="btn-danger btn-sm"
                  >
                    <Trash2 size={13} />
                  </button>
                </div>
              </div>
              <ConfigSummary config={config} />
            </div>
          ))}
        </div>
      )}

      {showModal && (
        <div className="fixed inset-0 bg-black/60 flex items-start justify-center z-50 p-4 overflow-y-auto">
          <div className="bg-white border border-gray-200 rounded-xl w-full max-w-3xl my-8 flex flex-col dark:bg-gray-900 dark:border-gray-700">
            {/* Header */}
            <div className="flex items-center justify-between px-6 py-4 border-b border-gray-200 dark:border-gray-700">
              <h3 className="font-semibold text-gray-800 dark:text-gray-100">
                {editConfig ? t.configs.editDockerConfig : t.configs.newDockerConfig}
              </h3>
              <button onClick={closeModal} className="text-gray-400 hover:text-gray-700 dark:hover:text-gray-300">
                <X size={18} />
              </button>
            </div>

            <form onSubmit={handleSubmit}>
              <div className="p-6 space-y-6">
                {/* Name */}
                <div>
                  <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.common.name}</label>
                  <input
                    className="input"
                    value={form.name}
                    onChange={e => setForm(f => ({ ...f, name: e.target.value }))}
                    placeholder="My Docker Config"
                    required
                  />
                </div>

                {/* Port Mappings */}
                <PortMappingsEditor
                  rows={form.port_mappings}
                  onChange={v => setForm(f => ({ ...f, port_mappings: v }))}
                  t={t}
                />

                {/* Env Vars */}
                <EnvVarsEditor
                  rows={form.env_vars}
                  onChange={v => setForm(f => ({ ...f, env_vars: v }))}
                  t={t}
                />

                {/* Init Script */}
                <div>
                  <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">{t.configs.initScript}</label>
                  <p className="text-xs text-gray-500 dark:text-gray-400 mb-2">{t.configs.initScriptHint}</p>
                  <textarea
                    className="input h-36 resize-y font-mono text-sm"
                    value={form.init_script}
                    onChange={e => setForm(f => ({ ...f, init_script: e.target.value }))}
                    placeholder={'#!/bin/bash\n# e.g. install dependencies, configure env...'}
                  />
                </div>

                {(createMutation.error || updateMutation.error) && (
                  <div className="text-red-500 dark:text-red-400 text-sm">
                    {String((createMutation.error || updateMutation.error)?.message)}
                  </div>
                )}
              </div>

              {/* Footer */}
              <div className="flex gap-3 justify-end px-6 py-4 border-t border-gray-200 dark:border-gray-700">
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
