import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Plus, Trash2, Play, Square, RotateCcw, Bot, ExternalLink, RefreshCw, Send, Copy, Pencil } from 'lucide-react'
import { useNavigate } from 'react-router-dom'
import {
  agentsApi, serversApi, codexConfigsApi, configsApi, dockerConfigsApi, tasksApi, notificationsApi,
  type Agent, type Server,
} from '../lib/api'
import { useI18n } from '../hooks/useI18n'
import DeleteAgentDialog from '../components/DeleteAgentDialog'
import { canDispatchTask, getAgentRuntimeAction, type AgentRuntimeAction } from '../lib/agentRuntime'

interface AgentFormData {
  name: string
  server_id: string
  use_docker: boolean
  use_git: boolean
  git_repo: string
  git_branch: string
  git_auth_type: string
  git_username: string
  git_password: string
  cli_type: string
  codex_config_id: string
  agents_md_id: string
  docker_config_id: string
  docker_image: string
}

interface EditFormData {
  name: string
  git_repo: string
  git_branch: string
  codex_config_id: string
  agents_md_id: string
}

const defaultForm: AgentFormData = {
  name: '',
  server_id: '',
  use_docker: true,
  use_git: false,
  git_repo: '',
  git_branch: 'main',
  git_auth_type: 'passwordless',
  git_username: '',
  git_password: '',
  cli_type: 'codex',
  codex_config_id: '',
  agents_md_id: '',
  docker_config_id: '',
  docker_image: 'ubuntu:24.04',
}

const CLI_TYPES = [
  { value: 'codex', label: 'Codex', wip: false },
  { value: 'claude_code', label: 'Claude Code', wip: true },
  { value: 'gemini_cli', label: 'Gemini CLI', wip: true },
  { value: 'opencode', label: 'OpenCode', wip: true },
]

function SectionDivider({ label }: { label: string }) {
  return (
    <div className="flex items-center gap-3 my-1">
      <div className="flex-1 h-px bg-gray-200 dark:bg-gray-700" />
      <span className="text-[11px] font-medium text-gray-400 dark:text-gray-500 uppercase tracking-wider whitespace-nowrap">{label}</span>
      <div className="flex-1 h-px bg-gray-200 dark:bg-gray-700" />
    </div>
  )
}

export default function Agents() {
  const qc = useQueryClient()
  const { t } = useI18n()
  const navigate = useNavigate()
  const [showModal, setShowModal] = useState(false)
  const [form, setForm] = useState<AgentFormData>(defaultForm)
  const [editAgent, setEditAgent] = useState<Agent | null>(null)
  const [editForm, setEditForm] = useState<EditFormData>({ name: '', git_repo: '', git_branch: '', codex_config_id: '', agents_md_id: '' })
  const [gitRepoConfirm, setGitRepoConfirm] = useState(false)
  const [dispatchAgent, setDispatchAgent] = useState<Agent | null>(null)
  const [dispatchInput, setDispatchInput] = useState('')
  const [dispatchNotifIds, setDispatchNotifIds] = useState<string[]>([])
  const [deleteAgent, setDeleteAgent] = useState<Agent | null>(null)
  const [deleteError, setDeleteError] = useState<string | null>(null)
  const [copySourceName, setCopySourceName] = useState<string | null>(null)
  const [runtimeConfirm, setRuntimeConfirm] = useState<{ agent: Agent; action: Exclude<AgentRuntimeAction, 'start'> } | null>(null)

  const { data: agents = [], isLoading } = useQuery({ queryKey: ['agents'], queryFn: agentsApi.list })
  const { data: servers = [] } = useQuery({ queryKey: ['servers'], queryFn: serversApi.list })
  const { data: codexConfigs = [] } = useQuery({ queryKey: ['codex-configs'], queryFn: codexConfigsApi.list })
  const { data: agentsMdConfigs = [] } = useQuery({
    queryKey: ['configs', 'agents_md'],
    queryFn: () => configsApi.list({ category: 'agents_md' }),
  })
  const { data: dockerConfigs = [] } = useQuery({ queryKey: ['docker-configs'], queryFn: dockerConfigsApi.list })
  const { data: notifConfigs = [] } = useQuery({ queryKey: ['notifications'], queryFn: notificationsApi.list })

  const createMutation = useMutation({
    mutationFn: (data: AgentFormData) => agentsApi.create({
      name: data.name.trim(),
      server_id: data.server_id,
      git_repo: data.use_git ? data.git_repo.trim() : '',
      git_branch: data.git_branch.trim(),
      git_auth_type: data.use_git ? data.git_auth_type : 'none',
      git_username: data.git_username.trim() || undefined,
      git_password: data.git_password || undefined,
      cli_type: data.cli_type,
      codex_config_id: data.codex_config_id || undefined,
      agents_md_id: data.agents_md_id || undefined,
      docker_config_id: data.use_docker ? (data.docker_config_id || undefined) : undefined,
      docker_image: data.use_docker ? data.docker_image.trim() : undefined,
      use_docker: data.use_docker,
    }),
    onSuccess: (agent) => {
      qc.invalidateQueries({ queryKey: ['agents'] })
      resetCreateModal()
      navigate(`/agents/${agent.id}?tab=provision`)
    },
  })

  const updateMutation = useMutation({
    mutationFn: async ({ id, data, forceReclone }: { id: string; data: EditFormData; forceReclone?: boolean }) => {
      return agentsApi.update(id, {
        name: data.name || undefined,
        git_repo: data.git_repo || undefined,
        git_branch: data.git_branch || undefined,
        codex_config_id: data.codex_config_id || undefined,
        agents_md_id: data.agents_md_id || undefined,
        force_reclone: forceReclone,
      })
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['agents'] })
      setEditAgent(null)
      setGitRepoConfirm(false)
    },
    onError: (err: Error & { status?: number; requires_confirm?: boolean }) => {
      if (err.status === 409 || err.requires_confirm) {
        setGitRepoConfirm(true)
      }
    },
  })

  const deleteMutation = useMutation({
    mutationFn: ({ id, cleanup }: { id: string; cleanup: boolean }) => agentsApi.delete(id, cleanup),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['agents'] })
      setDeleteAgent(null)
      setDeleteError(null)
    },
    onError: (err: any) => {
      setDeleteError(err?.message || String(err))
    },
  })
  const runtimeMutation = useMutation({
    mutationFn: ({ id, action }: { id: string; action: AgentRuntimeAction }) => {
      if (action === 'start') return agentsApi.start(id)
      if (action === 'pause') return agentsApi.stop(id)
      return agentsApi.restart(id)
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['agents'] })
      setRuntimeConfirm(null)
    },
  })

  const [dispatchTitle, setDispatchTitle] = useState('')

  const dispatchMutation = useMutation({
    mutationFn: ({ agentId, title, desc, notifIds }: { agentId: string; title: string; desc: string; notifIds?: string[] }) =>
      tasksApi.create(agentId, title, desc, notifIds),
    onSuccess: (task) => {
      qc.invalidateQueries({ queryKey: ['agents'] })
      setDispatchAgent(null)
      setDispatchInput('')
      setDispatchNotifIds([])
      // Navigate to agent detail with the new task expanded
      navigate(`/agents/${task.agent_id}?tab=tasks&task=${task.id}`)
    },
  })

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    createMutation.mutate(form)
  }

  function resetCreateModal() {
    createMutation.reset()
    setShowModal(false)
    setForm(defaultForm)
    setCopySourceName(null)
  }

  function handleCreateOpen() {
    createMutation.reset()
    setForm(defaultForm)
    setCopySourceName(null)
    setShowModal(true)
  }

  function handleCopyOpen(agent: Agent) {
    const useGit = Boolean(agent.git_repo)
    const gitAuthType = useGit && ['passwordless', 'https_password', 'ssh_key'].includes(agent.git_auth_type)
      ? agent.git_auth_type
      : defaultForm.git_auth_type

    createMutation.reset()
    setForm({
      name: t.agents.copyName(agent.name),
      server_id: agent.server_id,
      use_docker: agent.use_docker,
      use_git: useGit,
      git_repo: agent.git_repo,
      git_branch: agent.git_branch || defaultForm.git_branch,
      git_auth_type: gitAuthType,
      git_username: agent.git_username ?? '',
      git_password: '',
      cli_type: agent.cli_type,
      codex_config_id: agent.codex_config_id ?? '',
      agents_md_id: agent.agents_md_id ?? '',
      docker_config_id: agent.docker_config_id ?? '',
      docker_image: agent.docker_image || defaultForm.docker_image,
    })
    setCopySourceName(agent.name)
    setShowModal(true)
  }

  function handleEditOpen(agent: Agent) {
    updateMutation.reset()
    setEditAgent(agent)
    setEditForm({
      name: agent.name,
      git_repo: agent.git_repo,
      git_branch: agent.git_branch,
      codex_config_id: agent.codex_config_id ?? '',
      agents_md_id: agent.agents_md_id ?? '',
    })
    setGitRepoConfirm(false)
  }

  function handleEditSubmit(e: React.FormEvent, forceReclone?: boolean) {
    e.preventDefault()
    if (!editAgent) return
    updateMutation.mutate({ id: editAgent.id, data: editForm, forceReclone })
  }

  function handleRuntimeAction(agent: Agent) {
    const action = getAgentRuntimeAction(agent)
    if (!action) return
    if (action === 'start') {
      runtimeMutation.mutate({ id: agent.id, action })
      return
    }
    setRuntimeConfirm({ agent, action })
  }

  return (
    <div className="p-8">
      <div className="flex items-center justify-between mb-8">
        <div>
          <h1 className="text-2xl font-bold text-gray-900 dark:text-white">{t.agents.title}</h1>
          <p className="text-gray-500 mt-1">{t.agents.subtitle}</p>
        </div>
        <button onClick={handleCreateOpen} className="btn-primary flex items-center gap-2">
          <Plus size={16} />{t.agents.newAgent}
        </button>
      </div>

      {isLoading ? (
        <div className="text-center py-12 text-gray-500">{t.common.loading}</div>
      ) : agents.length === 0 ? (
        <div className="text-center py-12 card">
          <Bot size={40} className="mx-auto text-gray-400 dark:text-gray-600 mb-3" />
          <p className="text-gray-500 dark:text-gray-400">{t.agents.noAgents}</p>
          <p className="text-gray-400 dark:text-gray-600 text-sm mt-1">{t.agents.noAgentsHint}</p>
        </div>
      ) : (
        <div className="grid gap-4">
          {agents.map(agent => (
            <AgentRow key={agent.id} agent={agent} servers={servers} t={t}
              onRuntimeAction={() => handleRuntimeAction(agent)}
              onEdit={() => handleEditOpen(agent)}
              onDispatch={() => { setDispatchAgent(agent); setDispatchTitle(''); setDispatchInput('') }}
              onCopy={() => handleCopyOpen(agent)}
              onDelete={() => setDeleteAgent(agent)}
              runtimePending={runtimeMutation.isPending}
            />
          ))}
        </div>
      )}

      {/* Create modal */}
      {showModal && (
        <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50 p-4">
          <div className="bg-white border border-gray-200 rounded-xl w-full max-w-3xl max-h-[90vh] overflow-y-auto dark:bg-gray-900 dark:border-gray-700">
            <div className="flex items-center justify-between px-6 py-4 border-b border-gray-200 dark:border-gray-700 sticky top-0 bg-white dark:bg-gray-900 z-10">
              <h3 className="font-semibold text-gray-800 dark:text-gray-100">
                {copySourceName ? t.agents.copyAgentTitle(copySourceName) : t.agents.newAgent}
              </h3>
              <button onClick={resetCreateModal} className="text-gray-400 hover:text-gray-700 dark:hover:text-gray-300">✕</button>
            </div>
            <form onSubmit={handleSubmit} className="p-6 space-y-4">
              {copySourceName && form.use_git && form.git_auth_type === 'https_password' && (
                <div className="rounded-lg border border-amber-300 bg-amber-50 px-4 py-3 text-sm text-amber-800 dark:border-amber-700 dark:bg-amber-950/30 dark:text-amber-300">
                  {t.agents.copyPasswordHint}
                </div>
              )}

              {/* Agent Name + Server */}
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.agents.agentName}</label>
                  <input className="input" value={form.name} onChange={e => setForm(f => ({ ...f, name: e.target.value }))} placeholder="my-codex-agent" required />
                </div>
                <div>
                  <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.agents.server}</label>
                  <select className="input" value={form.server_id} onChange={e => setForm(f => ({ ...f, server_id: e.target.value }))} required>
                    <option value="">{t.agents.selectServer}</option>
                    {servers.map(s => <option key={s.id} value={s.id}>{s.name} ({s.ip})</option>)}
                  </select>
                </div>
              </div>

              {/* CLI Type */}
              <div>
                <label className="block text-sm text-gray-600 dark:text-gray-400 mb-2">{t.agents.cliType}</label>
                <div className="flex gap-2 flex-wrap">
                  {CLI_TYPES.map(cli => (
                    <button
                      key={cli.value}
                      type="button"
                      disabled={cli.wip}
                      onClick={() => !cli.wip && setForm(f => ({ ...f, cli_type: cli.value }))}
                      className={`px-3 py-1.5 rounded-lg text-sm font-medium border transition-colors flex items-center gap-1.5 ${
                        form.cli_type === cli.value && !cli.wip
                          ? 'bg-sky-500 text-white border-sky-500 dark:bg-sky-600 dark:border-sky-600'
                          : cli.wip
                          ? 'opacity-40 cursor-not-allowed border-gray-200 dark:border-gray-700 text-gray-400 dark:text-gray-600'
                          : 'border-gray-200 dark:border-gray-700 text-gray-600 dark:text-gray-400 hover:border-sky-400 dark:hover:border-sky-600'
                      }`}
                    >
                      {cli.label}
                      {cli.wip && (
                        <span className="text-[10px] px-1 py-0.5 rounded bg-yellow-100 text-yellow-700 dark:bg-yellow-900/30 dark:text-yellow-400 font-medium">WIP</span>
                      )}
                    </button>
                  ))}
                </div>
              </div>

              {/* CLI Config section */}
              <SectionDivider label={t.agents.cliConfigSection} />

              <div>
                <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.agents.codexConfig}</label>
                <div className="flex gap-2">
                  <select className="input flex-1" value={form.codex_config_id} onChange={e => setForm(f => ({ ...f, codex_config_id: e.target.value }))}>
                    <option value="">{t.agents.noConfig}</option>
                    {codexConfigs.map(c => <option key={c.id} value={c.id}>{c.name}</option>)}
                  </select>
                  <button type="button" title={t.agents.openCodexConfigPage} onClick={() => window.open('/configs/config-files/codex', '_blank')} className="btn-secondary btn-sm px-2.5"><ExternalLink size={13} /></button>
                  <button type="button" title={t.agents.refreshList} onClick={() => qc.invalidateQueries({ queryKey: ['codex-configs'] })} className="btn-secondary btn-sm px-2.5"><RefreshCw size={13} /></button>
                </div>
              </div>

              <div>
                <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.agents.agentsMdConfig}</label>
                <div className="flex gap-2">
                  <select className="input flex-1" value={form.agents_md_id} onChange={e => setForm(f => ({ ...f, agents_md_id: e.target.value }))}>
                    <option value="">{t.agents.noConfig}</option>
                    {agentsMdConfigs.map(c => <option key={c.id} value={c.id}>{c.name}</option>)}
                  </select>
                  <button type="button" title={t.agents.openAgentsMdPage} onClick={() => window.open('/configs/agents-md', '_blank')} className="btn-secondary btn-sm px-2.5"><ExternalLink size={13} /></button>
                  <button type="button" title={t.agents.refreshList} onClick={() => qc.invalidateQueries({ queryKey: ['configs', 'agents_md'] })} className="btn-secondary btn-sm px-2.5"><RefreshCw size={13} /></button>
                </div>
              </div>

              {/* Skills (WIP) */}
              <div>
                <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.agents.skills}</label>
                <div className="flex items-center gap-2 input bg-gray-50 dark:bg-gray-800 opacity-60 cursor-not-allowed">
                  <span className="text-xs text-gray-500 dark:text-gray-400 flex-1">{t.agents.wipFeature}</span>
                  <span className="text-[10px] px-1.5 py-0.5 rounded bg-yellow-100 text-yellow-700 dark:bg-yellow-900/30 dark:text-yellow-400 font-medium">WIP</span>
                </div>
              </div>

              {/* MCP (WIP) */}
              <div>
                <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.agents.mcp}</label>
                <div className="flex items-center gap-2 input bg-gray-50 dark:bg-gray-800 opacity-60 cursor-not-allowed">
                  <span className="text-xs text-gray-500 dark:text-gray-400 flex-1">{t.agents.wipFeature}</span>
                  <span className="text-[10px] px-1.5 py-0.5 rounded bg-yellow-100 text-yellow-700 dark:bg-yellow-900/30 dark:text-yellow-400 font-medium">WIP</span>
                </div>
              </div>

              {/* Git section */}
              <SectionDivider label={t.agents.gitSection} />

              <div>
                <label className="flex items-center gap-2 cursor-pointer select-none">
                  <input
                    type="checkbox"
                    className="rounded border-gray-300 dark:border-gray-600"
                    checked={form.use_git}
                    onChange={e => setForm(f => ({ ...f, use_git: e.target.checked }))}
                  />
                  <span className="text-sm text-gray-700 dark:text-gray-300">{t.agents.enableGit}</span>
                </label>
              </div>

              {form.use_git && (
                <>
                  <div>
                    <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.agents.gitRepo}</label>
                    <input
                      className="input"
                      value={form.git_repo}
                      onChange={e => setForm(f => ({ ...f, git_repo: e.target.value }))}
                      placeholder="https://github.com/org/repo.git"
                      required={form.use_git}
                    />
                  </div>
                  <div className="grid grid-cols-2 gap-4">
                    <div>
                      <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.agents.branch}</label>
                      <input className="input" value={form.git_branch} onChange={e => setForm(f => ({ ...f, git_branch: e.target.value }))} />
                    </div>
                    <div>
                      <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.agents.gitAuth}</label>
                      <select className="input" value={form.git_auth_type} onChange={e => setForm(f => ({ ...f, git_auth_type: e.target.value }))}>
                        <option value="passwordless">{t.agents.gitAuthPasswordless}</option>
                        <option value="https_password">{t.agents.gitAuthHttps}</option>
                        <option value="ssh_key">{t.agents.gitAuthSsh}</option>
                      </select>
                    </div>
                  </div>
                  {form.git_auth_type === 'https_password' && (
                    <div className="grid grid-cols-2 gap-4">
                      <div>
                        <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.agents.gitUsername}</label>
                        <input className="input" value={form.git_username} onChange={e => setForm(f => ({ ...f, git_username: e.target.value }))} />
                      </div>
                      <div>
                        <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.agents.gitPasswordToken}</label>
                        <input type="password" className="input" value={form.git_password} onChange={e => setForm(f => ({ ...f, git_password: e.target.value }))} />
                      </div>
                    </div>
                  )}
                </>
              )}

              {/* Docker section */}
              <SectionDivider label={t.agents.dockerSection} />

              <div>
                <label className="flex items-center gap-2 cursor-pointer select-none">
                  <input
                    type="checkbox"
                    className="rounded border-gray-300 dark:border-gray-600"
                    checked={form.use_docker}
                    onChange={e => setForm(f => ({ ...f, use_docker: e.target.checked }))}
                  />
                  <span className="text-sm text-gray-700 dark:text-gray-300">{t.agents.useDocker}</span>
                </label>
                {!form.use_docker && (
                  <p className="text-xs text-gray-400 dark:text-gray-500 mt-1 ml-6">{t.agents.noDocker}</p>
                )}
              </div>

              {form.use_docker && (
                <>
                  <div>
                    <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.agents.dockerImage}</label>
                    <input className="input" value={form.docker_image} onChange={e => setForm(f => ({ ...f, docker_image: e.target.value }))} />
                  </div>

                  <div>
                    <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.agents.dockerConfig}</label>
                    <div className="flex gap-2">
                      <select className="input flex-1" value={form.docker_config_id} onChange={e => setForm(f => ({ ...f, docker_config_id: e.target.value }))}>
                        <option value="">{t.agents.noConfig}</option>
                        {dockerConfigs.map(c => <option key={c.id} value={c.id}>{c.name}</option>)}
                      </select>
                      <button type="button" title={t.agents.openDockerConfigPage} onClick={() => window.open('/configs/docker', '_blank')} className="btn-secondary btn-sm px-2.5"><ExternalLink size={13} /></button>
                      <button type="button" title={t.agents.refreshList} onClick={() => qc.invalidateQueries({ queryKey: ['docker-configs'] })} className="btn-secondary btn-sm px-2.5"><RefreshCw size={13} /></button>
                    </div>
                  </div>
                </>
              )}

              {createMutation.error && (
                <div className="text-red-500 dark:text-red-400 text-sm">{String(createMutation.error.message)}</div>
              )}
              <div className="flex gap-3 justify-end pt-2">
                <button type="button" onClick={resetCreateModal} className="btn-secondary">{t.common.cancel}</button>
                <button type="submit" className="btn-primary" disabled={createMutation.isPending}>
                  {createMutation.isPending ? t.agents.creating : t.agents.createAgent}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {/* Edit modal */}
      {editAgent && (
        <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50 p-4">
          <div className="bg-white border border-gray-200 rounded-xl w-full max-w-2xl max-h-[90vh] overflow-y-auto dark:bg-gray-900 dark:border-gray-700">
            <div className="flex items-center justify-between px-6 py-4 border-b border-gray-200 dark:border-gray-700 sticky top-0 bg-white dark:bg-gray-900 z-10">
              <h3 className="font-semibold text-gray-800 dark:text-gray-100">{t.agents.editAgentTitle(editAgent.name)}</h3>
              <button onClick={() => { setEditAgent(null); setGitRepoConfirm(false) }} className="text-gray-400 hover:text-gray-700 dark:hover:text-gray-300">✕</button>
            </div>
            <form onSubmit={e => handleEditSubmit(e)} className="p-6 space-y-4">

              {/* Read-only fields */}
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.agents.server}</label>
                  <input
                    className="input bg-gray-50 dark:bg-gray-800 cursor-not-allowed opacity-70"
                    value={servers.find(s => s.id === editAgent.server_id)?.name ?? editAgent.server_id}
                    disabled
                    title={t.agents.recreateToChangeHint}
                  />
                </div>
                <div>
                  <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.agents.dockerImage}</label>
                  <input
                    className="input bg-gray-50 dark:bg-gray-800 cursor-not-allowed opacity-70"
                    value={editAgent.docker_image}
                    disabled
                    title={t.agents.recreateToChangeHint}
                  />
                </div>
              </div>

              {/* Editable name */}
              <div>
                <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.agents.agentName}</label>
                <input className="input" value={editForm.name} onChange={e => setEditForm(f => ({ ...f, name: e.target.value }))} />
              </div>

              {/* Codex config + AGENTS.md */}
              <SectionDivider label={t.agents.cliConfigSection} />

              <div>
                <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.agents.codexConfig}</label>
                <div className="flex gap-2">
                  <select className="input flex-1" value={editForm.codex_config_id} onChange={e => setEditForm(f => ({ ...f, codex_config_id: e.target.value }))}>
                    <option value="">{t.agents.noConfig}</option>
                    {codexConfigs.map(c => <option key={c.id} value={c.id}>{c.name}</option>)}
                  </select>
                  <button type="button" onClick={() => qc.invalidateQueries({ queryKey: ['codex-configs'] })} className="btn-secondary btn-sm px-2.5"><RefreshCw size={13} /></button>
                </div>
              </div>

              <div>
                <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.agents.agentsMdConfig}</label>
                <div className="flex gap-2">
                  <select className="input flex-1" value={editForm.agents_md_id} onChange={e => setEditForm(f => ({ ...f, agents_md_id: e.target.value }))}>
                    <option value="">{t.agents.noConfig}</option>
                    {agentsMdConfigs.map(c => <option key={c.id} value={c.id}>{c.name}</option>)}
                  </select>
                  <button type="button" onClick={() => qc.invalidateQueries({ queryKey: ['configs', 'agents_md'] })} className="btn-secondary btn-sm px-2.5"><RefreshCw size={13} /></button>
                </div>
              </div>

              {/* Git section */}
              <SectionDivider label={t.agents.gitSection} />

              <div>
                <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.agents.gitRepo}</label>
                <input
                  className="input"
                  value={editForm.git_repo}
                  onChange={e => setEditForm(f => ({ ...f, git_repo: e.target.value }))}
                  placeholder="https://github.com/org/repo.git"
                />
              </div>

              <div>
                <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.agents.branch}</label>
                <input className="input" value={editForm.git_branch} onChange={e => setEditForm(f => ({ ...f, git_branch: e.target.value }))} />
              </div>

              {/* Git repo change confirmation */}
              {gitRepoConfirm && (
                <div className="rounded-lg border border-orange-400 bg-orange-50 dark:bg-orange-900/20 p-4">
                  <p className="text-sm text-orange-700 dark:text-orange-300 font-medium mb-3">
                    {t.agents.gitRepoChangeWarning}
                  </p>
                  <div className="flex gap-2">
                    <button
                      type="button"
                      onClick={e => handleEditSubmit(e as unknown as React.FormEvent, true)}
                      className="btn-primary btn-sm"
                    >
                      {t.agents.confirmReclone}
                    </button>
                    <button type="button" onClick={() => setGitRepoConfirm(false)} className="btn-secondary btn-sm">{t.common.cancel}</button>
                  </div>
                </div>
              )}

              {updateMutation.error && !gitRepoConfirm && (
                <div className="text-red-500 dark:text-red-400 text-sm">{String(updateMutation.error.message)}</div>
              )}
              <div className="flex gap-3 justify-end pt-2">
                <button type="button" onClick={() => { setEditAgent(null); setGitRepoConfirm(false) }} className="btn-secondary">{t.common.cancel}</button>
                <button type="submit" className="btn-primary" disabled={updateMutation.isPending}>
                  {updateMutation.isPending ? t.common.loading : t.common.save}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}

      {/* Dispatch task modal */}
      {dispatchAgent && (
        <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50 p-4">
          <div className="bg-white border border-gray-200 rounded-xl w-full max-w-lg dark:bg-gray-900 dark:border-gray-700">
            <div className="flex items-center justify-between px-6 py-4 border-b border-gray-200 dark:border-gray-700">
              <h3 className="font-semibold text-gray-800 dark:text-gray-100">{t.agents.dispatchTask} — {dispatchAgent.name}</h3>
              <button onClick={() => { setDispatchAgent(null); setDispatchNotifIds([]) }} className="text-gray-400 hover:text-gray-700 dark:hover:text-gray-300">✕</button>
            </div>
            <div className="p-6 space-y-4">
              <div>
                <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.agents.dispatchTaskTitle}</label>
                <input
                  className="input w-full"
                  value={dispatchTitle}
                  onChange={e => setDispatchTitle(e.target.value)}
                  placeholder={t.agents.dispatchTaskTitle}
                  autoFocus
                />
              </div>
              <div>
                <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.agents.dispatchTaskDesc}</label>
                <textarea
                  className="input w-full"
                  rows={5}
                  value={dispatchInput}
                  onChange={e => setDispatchInput(e.target.value)}
                  placeholder={t.agentDetail.taskPlaceholder(dispatchAgent.cli_type)}
                />
              </div>
              {notifConfigs.length > 0 && (
                <div>
                  <label className="block text-sm text-gray-600 dark:text-gray-400 mb-1.5">{t.notifications.selectNotifications}</label>
                  <div className="space-y-1.5">
                    {notifConfigs.map(n => (
                      <label key={n.id} className="flex items-center gap-2 cursor-pointer">
                        <input
                          type="checkbox"
                          checked={dispatchNotifIds.includes(n.id)}
                          onChange={e => {
                            if (e.target.checked) setDispatchNotifIds(prev => [...prev, n.id])
                            else setDispatchNotifIds(prev => prev.filter(x => x !== n.id))
                          }}
                          className="w-4 h-4 rounded"
                        />
                        <span className={`text-sm ${n.enabled ? 'text-gray-700 dark:text-gray-300' : 'text-gray-400 dark:text-gray-500'}`}>
                          {n.name}
                          {!n.enabled && <span className="ml-1 text-xs">({t.common.disabled})</span>}
                        </span>
                      </label>
                    ))}
                  </div>
                </div>
              )}
              {dispatchMutation.error && (
                <div className="text-red-500 text-sm mt-2">{String(dispatchMutation.error.message)}</div>
              )}
              <div className="flex gap-3 justify-end pt-2">
                <button onClick={() => { setDispatchAgent(null); setDispatchNotifIds([]) }} className="btn-secondary">{t.common.cancel}</button>
                <button
                  onClick={() => { if (dispatchInput.trim()) dispatchMutation.mutate({ agentId: dispatchAgent.id, title: dispatchTitle.trim(), desc: dispatchInput.trim(), notifIds: dispatchNotifIds.length > 0 ? dispatchNotifIds : undefined }) }}
                  className="btn-primary flex items-center gap-2"
                  disabled={dispatchMutation.isPending || !dispatchInput.trim()}
                >
                  <Send size={14} />{dispatchMutation.isPending ? t.agents.dispatching : t.agents.dispatchTask}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}

      <DeleteAgentDialog
        agent={deleteAgent}
        open={!!deleteAgent}
        pending={deleteMutation.isPending}
        error={deleteError}
        onClose={() => { setDeleteAgent(null); setDeleteError(null) }}
        onConfirm={(agent, cleanup) => {
          setDeleteError(null)
          deleteMutation.mutate({ id: agent.id, cleanup })
        }}
      />

      {runtimeConfirm && (
        <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50 p-4">
          <div className="bg-white border border-gray-200 rounded-xl w-full max-w-md dark:bg-gray-900 dark:border-gray-700">
            <div className="px-6 py-4 border-b border-gray-200 dark:border-gray-700">
              <h3 className="font-semibold text-gray-800 dark:text-gray-100">
                {runtimeConfirm.action === 'pause'
                  ? t.agents.stopAgentTitle(runtimeConfirm.agent.name)
                  : t.agents.restartAgentTitle(runtimeConfirm.agent.name)}
              </h3>
            </div>
            <div className="p-6 space-y-4">
              <p className="text-sm text-gray-600 dark:text-gray-400">
                {runtimeConfirm.action === 'pause'
                  ? t.agents.stopAgentConfirmMessage
                  : t.agents.restartAgentConfirmMessage}
              </p>
              <div className="flex gap-3 justify-end">
                <button onClick={() => setRuntimeConfirm(null)} className="btn-secondary">
                  {t.common.cancel}
                </button>
                <button
                  onClick={() => runtimeMutation.mutate({ id: runtimeConfirm.agent.id, action: runtimeConfirm.action })}
                  className={runtimeConfirm.action === 'pause' ? 'btn-danger' : 'btn-primary'}
                  disabled={runtimeMutation.isPending}
                >
                  {runtimeConfirm.action === 'pause' ? t.agents.confirmStop : t.agents.confirmRestart}
                </button>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

function AgentRow({ agent, servers, t, onRuntimeAction, onEdit, onDispatch, onCopy, onDelete, runtimePending }: {
  agent: Agent
  servers: Server[]
  t: ReturnType<typeof useI18n>['t']
  onRuntimeAction: () => void
  onEdit: () => void
  onDispatch: () => void
  onCopy: () => void
  onDelete: () => void
  runtimePending: boolean
}) {
  const navigate = useNavigate()
  const serverLabel = servers.find(s => s.id === agent.server_id)?.name ?? agent.server_id
  const runtimeAction = getAgentRuntimeAction(agent)
  const RuntimeIcon = runtimeAction === 'start' ? Play : runtimeAction === 'pause' ? Square : RotateCcw
  const runtimeLabel = runtimeAction === 'start'
    ? t.agents.start
    : runtimeAction === 'pause'
      ? t.agents.pause
      : t.agents.restart

  const statusMap: Record<string, string> = {
    running: 'badge-green',
    stopped: 'badge-gray',
    error: 'badge-red',
    provisioning: 'badge-yellow',
    provision_failed: 'badge-red',
  }

  return (
    <div
      className="card flex items-center gap-4 cursor-pointer hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors"
      onClick={() => navigate(`/agents/${agent.id}`)}
    >
      <div className="w-10 h-10 rounded-lg bg-purple-600/20 flex items-center justify-center">
        <Bot size={18} className="text-purple-400" />
      </div>
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <p className="font-medium text-gray-800 dark:text-gray-100">{agent.name}</p>
          <span className={statusMap[agent.status] ?? 'badge-gray'}>{t.status[agent.status as keyof typeof t.status] ?? agent.status}</span>
          <span className={agent.cli_type === 'codex' ? 'badge badge-indigo' : 'badge badge-blue'}>{agent.cli_type}</span>
          {agent.is_busy && (
            <span className="badge-yellow">{t.requirements.busy}</span>
          )}
          <span className={agent.use_docker ? 'badge badge-blue' : 'badge badge-gray'}>
            {agent.use_docker ? t.agents.dockerBadge : t.agents.noDockerBadge}
          </span>
        </div>
        <p className="text-sm text-gray-500">
          {serverLabel}
          {agent.git_repo && ` · ${agent.git_repo.split('/').slice(-2).join('/')} (${agent.git_branch})`}
        </p>
      </div>
      <div className="flex items-center gap-2" onClick={e => e.stopPropagation()}>
        {canDispatchTask(agent) && (
          <button onClick={onDispatch} className="btn-primary btn-sm flex items-center gap-1"><Send size={13} />{t.agents.dispatchTask}</button>
        )}
        {runtimeAction && (
          <button
            onClick={onRuntimeAction}
            className="btn-secondary btn-sm flex items-center gap-1"
            disabled={runtimePending}
          >
            <RuntimeIcon size={13} />
            {runtimeLabel}
          </button>
        )}
        <button onClick={onCopy} className="btn-secondary btn-sm flex items-center gap-1.5" title={t.common.copy}>
          <Copy size={13} />
          {t.common.copy}
        </button>
        <button onClick={onEdit} className="btn-secondary btn-sm flex items-center gap-1.5" title={t.common.edit}>
          <Pencil size={13} />
          {t.common.edit}
        </button>
        <button onClick={onDelete} className="btn-danger btn-sm flex items-center gap-1.5" title={t.common.delete}>
          <Trash2 size={13} />
          {t.common.delete}
        </button>
      </div>
    </div>
  )
}
