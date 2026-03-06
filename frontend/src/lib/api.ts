const BASE_URL = ''

function getToken(): string | null {
  return localStorage.getItem('token')
}

// Prevent multiple concurrent refresh attempts
let refreshPromise: Promise<boolean> | null = null

async function tryRefreshToken(): Promise<boolean> {
  const refreshToken = localStorage.getItem('refresh_token')
  if (!refreshToken) return false

  try {
    const res = await fetch(BASE_URL + '/api/auth/refresh', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ refresh_token: refreshToken }),
    })
    if (!res.ok) return false

    const data = await res.json()
    localStorage.setItem('token', data.access_token)
    localStorage.setItem('refresh_token', data.refresh_token)
    if (data.user) localStorage.setItem('user', JSON.stringify(data.user))
    return true
  } catch {
    return false
  }
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

  let res = await fetch(BASE_URL + path, { ...options, headers })

  // On 401, try refresh token before giving up
  if (res.status === 401 && !path.startsWith('/api/auth/')) {
    if (!refreshPromise) {
      refreshPromise = tryRefreshToken().finally(() => { refreshPromise = null })
    }
    const refreshed = await refreshPromise
    if (refreshed) {
      // Retry with new token
      const newHeaders = { ...headers, Authorization: `Bearer ${getToken()}` }
      res = await fetch(BASE_URL + path, { ...options, headers: newHeaders })
    }
  }

  if (!res.ok) {
    let errMsg = `HTTP ${res.status}`
    let requires_confirm = false
    try {
      const body = await res.json()
      errMsg = body.error || body.message || errMsg
      requires_confirm = body.requires_confirm === true
    } catch {}

    // Refresh failed or still 401 → redirect to login
    if (res.status === 401 && !path.startsWith('/api/auth/')) {
      localStorage.removeItem('token')
      localStorage.removeItem('refresh_token')
      localStorage.removeItem('user')
      window.location.href = '/login'
    }

    const err = new Error(errMsg) as Error & { status: number; requires_confirm: boolean }
    err.status = res.status
    err.requires_confirm = requires_confirm
    throw err
  }

  if (res.status === 204) return undefined as T
  return res.json()
}

// Auth
export interface AuthUser {
  id: string
  username: string
  display_name: string
  status: string
  roles: string[]
}

export interface LoginResponse {
  access_token: string
  refresh_token: string
  expires_in: number
  user: AuthUser
}

export const authApi = {
  login: (username: string, password: string) =>
    request<LoginResponse>(
      '/api/auth/login',
      { method: 'POST', body: JSON.stringify({ username, password }) },
    ),
  refresh: (refresh_token: string) =>
    request<LoginResponse>('/api/auth/refresh', {
      method: 'POST',
      body: JSON.stringify({ refresh_token }),
    }),
  me: () => request<AuthUser>('/api/me'),
  changeMyPassword: (old_password: string, new_password: string) =>
    request<{ message: string }>('/api/me/password', {
      method: 'PUT',
      body: JSON.stringify({ old_password, new_password }),
    }),
  logout: () => request<{ message: string }>('/api/auth/logout', { method: 'POST' }),
}

export interface AdminUser {
  id: string
  username: string
  display_name: string
  status: 'active' | 'disabled'
  failed_attempts: number
  locked_until?: string
  roles: string[]
  created_at: string
}

export const adminUsersApi = {
  list: () => request<AdminUser[]>('/api/admin/users'),
  create: (data: { username: string; display_name: string; password: string; roles?: string[] }) =>
    request<AdminUser>('/api/admin/users', { method: 'POST', body: JSON.stringify(data) }),
  resetPassword: (id: string, new_password: string) =>
    request<{ message: string }>(`/api/admin/users/${id}/reset-password`, {
      method: 'POST',
      body: JSON.stringify({ new_password }),
    }),
  updateStatus: (id: string, status: 'active' | 'disabled') =>
    request<{ message: string }>(`/api/admin/users/${id}/status`, {
      method: 'PATCH',
      body: JSON.stringify({ status }),
    }),
  unlock: (id: string) =>
    request<{ message: string }>(`/api/admin/users/${id}/unlock`, { method: 'POST' }),
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
  provision_steps: Record<string, string>
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
  task_log: string
  thread_id?: string
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

// Requirements
export interface Project {
  id: string
  name: string
  description: string
  status: string
  created_at: string
  updated_at: string
}

export interface WorkItem {
  id: string
  project_id: string
  parent_id?: string
  type: string
  title: string
  description: string
  status: string
  priority: string
  assigned_agent_id?: string
  assigned_user_id?: string
  execution_id?: string
  created_at: string
  updated_at: string
}

export interface SimpleUser {
  id: string
  username: string
  display_name: string
}

export const usersApi = {
  list: () => request<SimpleUser[]>('/api/users'),
}

export const projectsApi = {
  list: () => request<Project[]>('/api/projects'),
  create: (data: { name: string; description?: string }) =>
    request<Project>('/api/projects', { method: 'POST', body: JSON.stringify(data) }),
  get: (id: string) => request<Project>(`/api/projects/${id}`),
  update: (id: string, data: { name?: string; description?: string; status?: string }) =>
    request<Project>(`/api/projects/${id}`, { method: 'PUT', body: JSON.stringify(data) }),
  delete: (id: string) =>
    request<{ message: string }>(`/api/projects/${id}`, { method: 'DELETE' }),
  listWorkItems: (projectId: string, params?: { status?: string; type?: string }) => {
    const qs = new URLSearchParams()
    if (params?.status) qs.set('status', params.status)
    if (params?.type) qs.set('type', params.type)
    const query = qs.toString() ? `?${qs.toString()}` : ''
    return request<WorkItem[]>(`/api/projects/${projectId}/work-items${query}`)
  },
  createWorkItem: (projectId: string, data: {
    parent_id?: string; type: string; title: string; description?: string; priority?: string; assigned_user_id?: string
  }) =>
    request<WorkItem>(`/api/projects/${projectId}/work-items`, { method: 'POST', body: JSON.stringify(data) }),
}

export const workItemsApi = {
  get: (id: string) => request<WorkItem>(`/api/work-items/${id}`),
  update: (id: string, data: {
    title?: string; description?: string; priority?: string; status?: string;
    assigned_agent_id?: string; assigned_user_id?: string; execution_id?: string
  }) =>
    request<WorkItem>(`/api/work-items/${id}`, { method: 'PUT', body: JSON.stringify(data) }),
  delete: (id: string) =>
    request<{ message: string }>(`/api/work-items/${id}`, { method: 'DELETE' }),
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
