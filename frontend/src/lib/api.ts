const BASE_URL = ''

function getToken(): string | null {
  return localStorage.getItem('token')
}

async function request<T>(
  path: string,
  options: RequestInit = {},
): Promise<T> {
  const token = getToken()
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    ...(options.headers as Record<string, string>),
  }
  if (token) {
    headers['Authorization'] = `Bearer ${token}`
  }

  const res = await fetch(BASE_URL + path, { ...options, headers })

  if (!res.ok) {
    let errMsg = `HTTP ${res.status}`
    let requires_confirm = false
    try {
      const body = await res.json()
      errMsg = body.error || body.message || errMsg
      requires_confirm = body.requires_confirm === true
    } catch {}
    const err = new Error(errMsg) as Error & { status: number; requires_confirm: boolean }
    err.status = res.status
    err.requires_confirm = requires_confirm
    throw err
  }

  if (res.status === 204) return undefined as T
  return res.json()
}

// Auth
export const authApi = {
  login: (username: string, password: string) =>
    request<{ token: string; user_id: string; username: string; display_name: string }>(
      '/api/auth/login',
      { method: 'POST', body: JSON.stringify({ username, password }) },
    ),
  register: (data: { username: string; display_name: string; password: string }) =>
    request<{ message: string }>('/api/auth/register', {
      method: 'POST',
      body: JSON.stringify(data),
    }),
  logout: () => request<{ message: string }>('/api/auth/logout', { method: 'POST' }),
}

// Servers
export interface Server {
  id: string
  name: string
  ip: string
  port: number
  username: string
  auth_type: string
  os_type: string
  status: string
  created_at: string
}

export const serversApi = {
  list: () => request<Server[]>('/api/servers'),
  create: (data: { name: string; ip: string; port: number; username: string; password?: string; os_type?: string }) =>
    request<Server>('/api/servers', { method: 'POST', body: JSON.stringify(data) }),
  update: (id: string, data: { name?: string; ip?: string; port?: number; username?: string; os_type?: string }) =>
    request<Server>(`/api/servers/${id}`, { method: 'PUT', body: JSON.stringify(data) }),
  delete: (id: string) =>
    request<{ message: string }>(`/api/servers/${id}`, { method: 'DELETE' }),
  test: (id: string) =>
    request<{ status: string; message: string; output: string }>(
      `/api/servers/${id}/test`,
      { method: 'POST' },
    ),
}

// Company Configs
export interface CompanyConfig {
  id: string
  name: string
  category: string
  cli_type: string
  file_type: string | null
  content: string
  created_at: string
  updated_at: string
}

export const configsApi = {
  list: (params?: { category?: string; cli_type?: string }) => {
    const qs = new URLSearchParams()
    if (params?.category) qs.set('category', params.category)
    if (params?.cli_type) qs.set('cli_type', params.cli_type)
    const query = qs.toString() ? `?${qs.toString()}` : ''
    return request<CompanyConfig[]>(`/api/configs${query}`)
  },
  create: (data: { name: string; category?: string; cli_type: string; file_type?: string; content: string }) =>
    request<CompanyConfig>('/api/configs', { method: 'POST', body: JSON.stringify(data) }),
  update: (id: string, data: Partial<{ name: string; cli_type: string; file_type: string; content: string }>) =>
    request<CompanyConfig>(`/api/configs/${id}`, { method: 'PUT', body: JSON.stringify(data) }),
  delete: (id: string) =>
    request<{ message: string }>(`/api/configs/${id}`, { method: 'DELETE' }),
  getTemplate: async (path: string): Promise<{ content: string }> => {
    const token = getToken()
    const headers: Record<string, string> = {}
    if (token) headers['Authorization'] = `Bearer ${token}`
    const res = await fetch(`/api/config-templates/${path}`, { headers })
    if (!res.ok) throw new Error(`HTTP ${res.status}`)
    const content = await res.text()
    return { content }
  },
}

// Docker Configs
export interface PortMapping {
  host_port: string
  container_port: string
  protocol: 'tcp' | 'udp'
}

export interface EnvVar {
  key: string
  value: string
}

export interface VolumeMapping {
  host_path: string
  container_path: string
  mode: 'rw' | 'ro'
}

export interface DockerConfig {
  id: string
  name: string
  port_mappings: PortMapping[]
  env_vars: EnvVar[]
  volume_mappings: VolumeMapping[]
  init_script: string
  created_at: string
  updated_at: string
}

export const dockerConfigsApi = {
  list: () => request<DockerConfig[]>('/api/docker-configs'),
  create: (data: {
    name: string
    port_mappings?: PortMapping[]
    env_vars?: EnvVar[]
    volume_mappings?: VolumeMapping[]
    init_script?: string
  }) => request<DockerConfig>('/api/docker-configs', { method: 'POST', body: JSON.stringify(data) }),
  update: (id: string, data: {
    name?: string
    port_mappings?: PortMapping[]
    env_vars?: EnvVar[]
    volume_mappings?: VolumeMapping[]
    init_script?: string
  }) => request<DockerConfig>(`/api/docker-configs/${id}`, { method: 'PUT', body: JSON.stringify(data) }),
  delete: (id: string) =>
    request<{ message: string }>(`/api/docker-configs/${id}`, { method: 'DELETE' }),
}

// Codex Configs
export interface CodexConfig {
  id: string
  name: string
  config_toml: string
  auth_json: string
  created_at: string
  updated_at: string
}

export const codexConfigsApi = {
  list: () => request<CodexConfig[]>('/api/codex-configs'),
  create: (data: { name: string; config_toml?: string; auth_json?: string }) =>
    request<CodexConfig>('/api/codex-configs', { method: 'POST', body: JSON.stringify(data) }),
  update: (id: string, data: { name?: string; config_toml?: string; auth_json?: string }) =>
    request<CodexConfig>(`/api/codex-configs/${id}`, { method: 'PUT', body: JSON.stringify(data) }),
  delete: (id: string) =>
    request<{ message: string }>(`/api/codex-configs/${id}`, { method: 'DELETE' }),
}

// Agents
export interface Agent {
  id: string
  name: string
  server_id: string
  git_repo: string
  git_branch: string
  git_auth_type: string
  git_username?: string
  cli_type: string
  codex_config_id?: string
  agents_md_id?: string
  docker_config_id?: string
  docker_image: string
  docker_container_name?: string
  container_id?: string
  tmux_session: string
  workdir: string
  use_docker: boolean
  status: string
  provision_log: string
  created_at: string
}

export interface TerminalCommandResponse {
  local_cmd: string
  ssh_cmd?: string
}

export const agentsApi = {
  list: () => request<Agent[]>('/api/agents'),
  create: (data: {
    name: string
    server_id: string
    git_repo?: string
    git_branch?: string
    git_auth_type?: string
    git_username?: string
    git_password?: string
    cli_type: string
    codex_config_id?: string
    agents_md_id?: string
    docker_config_id?: string
    docker_image?: string
    use_docker?: boolean
  }) => request<Agent>('/api/agents', { method: 'POST', body: JSON.stringify(data) }),
  update: (id: string, data: {
    name?: string
    git_repo?: string
    git_branch?: string
    force_reclone?: boolean
    codex_config_id?: string
    agents_md_id?: string
    docker_config_id?: string
  }) =>
    request<Agent>(`/api/agents/${id}`, { method: 'PUT', body: JSON.stringify(data) }),
  delete: (id: string) =>
    request<{ message: string }>(`/api/agents/${id}`, { method: 'DELETE' }),
  start: (id: string) =>
    request<{ message: string; status: string }>(`/api/agents/${id}/start`, { method: 'POST' }),
  stop: (id: string) =>
    request<{ message: string; status: string }>(`/api/agents/${id}/stop`, { method: 'POST' }),
  resume: (id: string) =>
    request<{ message: string }>(`/api/agents/${id}/resume`, { method: 'POST' }),
  getTerminalCommand: (id: string) =>
    request<TerminalCommandResponse>(`/api/agents/${id}/terminal-command`),
}

// Tasks
export interface Task {
  id: string
  agent_id: string
  description: string
  status: string
  tmux_window?: string
  created_at: string
  started_at?: string
  completed_at?: string
}

export const tasksApi = {
  list: (agentId: string) => request<Task[]>(`/api/agents/${agentId}/tasks`),
  create: (agentId: string, description: string) =>
    request<Task>(`/api/agents/${agentId}/tasks`, {
      method: 'POST',
      body: JSON.stringify({ description }),
    }),
  get: (taskId: string) => request<Task>(`/api/tasks/${taskId}`),
}

// Notifications
export interface NotificationConfig {
  id: string
  name: string
  type: string
  config_json: string
  enabled: boolean
  events_json: string
  created_at: string
}

export const notificationsApi = {
  list: () => request<NotificationConfig[]>('/api/notifications'),
  create: (data: {
    name: string
    type: string
    config_json: string
    enabled?: boolean
    events_json?: string
  }) => request<NotificationConfig>('/api/notifications', { method: 'POST', body: JSON.stringify(data) }),
  update: (id: string, data: Partial<{ name: string; config_json: string; enabled: boolean; events_json: string }>) =>
    request<NotificationConfig>(`/api/notifications/${id}`, { method: 'PUT', body: JSON.stringify(data) }),
  delete: (id: string) =>
    request<{ message: string }>(`/api/notifications/${id}`, { method: 'DELETE' }),
}
