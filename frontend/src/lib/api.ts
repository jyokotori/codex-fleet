import { clearAuth } from './auth'

const BASE_URL = ''

function getToken(): string | null {
  return localStorage.getItem('token')
}

// --- Proactive token refresh scheduler ---

let refreshTimerId: ReturnType<typeof setTimeout> | null = null
const authChannel = new BroadcastChannel('auth')

export function scheduleTokenRefresh(expiresInSeconds: number) {
  clearScheduledRefresh()
  const refreshAfterMs = expiresInSeconds * 0.8 * 1000 // refresh at 80% lifetime
  if (refreshAfterMs <= 0) return
  refreshTimerId = setTimeout(async () => {
    const ok = await doRefreshToken()
    if (!ok) console.warn('Proactive token refresh failed')
  }, refreshAfterMs)
}

export function clearScheduledRefresh() {
  if (refreshTimerId !== null) {
    clearTimeout(refreshTimerId)
    refreshTimerId = null
  }
}

// Listen for token updates from other tabs
authChannel.onmessage = (event) => {
  if (event.data.type === 'token_refreshed') {
    localStorage.setItem('token', event.data.access_token)
    localStorage.setItem('refresh_token', event.data.refresh_token)
    localStorage.setItem('token_expires_in', String(event.data.expires_in))
    localStorage.setItem('token_obtained_at', String(Math.floor(Date.now() / 1000)))
    if (event.data.user) localStorage.setItem('user', JSON.stringify(event.data.user))
    scheduleTokenRefresh(event.data.expires_in)
  } else if (event.data.type === 'logout') {
    clearScheduledRefresh()
    clearAuth()
    window.location.href = '/login'
  }
}

// --- Token refresh core logic ---

async function doRefreshToken(): Promise<boolean> {
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
    localStorage.setItem('token_expires_in', String(data.expires_in))
    localStorage.setItem('token_obtained_at', String(Math.floor(Date.now() / 1000)))
    if (data.user) localStorage.setItem('user', JSON.stringify(data.user))

    // Chain next refresh
    scheduleTokenRefresh(data.expires_in)
    // Notify other tabs
    authChannel.postMessage({ type: 'token_refreshed', ...data })
    return true
  } catch {
    return false
  }
}

// Prevent multiple concurrent refresh attempts
let refreshPromise: Promise<boolean> | null = null

async function tryRefreshToken(): Promise<boolean> {
  if (!refreshPromise) {
    refreshPromise = doRefreshToken().finally(() => { refreshPromise = null })
  }
  return refreshPromise
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
    const refreshed = await tryRefreshToken()
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
      clearScheduledRefresh()
      authChannel.postMessage({ type: 'logout' })
      clearAuth()
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
  create: (data: { name: string; ip: string; port: number; username: string; password?: string; save_password?: boolean; os_type?: string }) =>
    request<Server>('/api/servers', { method: 'POST', body: JSON.stringify(data) }),
  update: (id: string, data: { name?: string; ip?: string; port?: number; username?: string; os_type?: string; password?: string; save_password?: boolean }) =>
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
  user_id?: string
  user_display_name?: string
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
  workdir: string
  use_docker: boolean
  status: string
  provision_log: string
  provision_steps: Record<string, string>
  is_busy: boolean
  created_at: string
}

export interface TerminalCommandResponse {
  local_cmd: string
  ssh_cmd?: string
  terminal_input_cmd?: string
}

export const agentsApi = {
  list: () => request<Agent[]>('/api/agents'),
  get: (id: string) => request<Agent>(`/api/agents/${id}`),
  create: (data: {
    name: string
    server_id: string
    user_id?: string
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
    user_id?: string
    codex_config_id?: string
    agents_md_id?: string
  }) =>
    request<Agent>(`/api/agents/${id}`, { method: 'PUT', body: JSON.stringify(data) }),
  delete: (id: string, cleanup = false) =>
    request<{ message: string }>(`/api/agents/${id}?cleanup=${cleanup}`, { method: 'DELETE' }),
  start: (id: string) =>
    request<{ message: string; status: string }>(`/api/agents/${id}/start`, { method: 'POST' }),
  stop: (id: string) =>
    request<{ message: string; status: string }>(`/api/agents/${id}/stop`, { method: 'POST' }),
  restart: (id: string) =>
    request<{ message: string; status: string }>(`/api/agents/${id}/restart`, { method: 'POST' }),
  getTerminalCommand: (id: string) =>
    request<TerminalCommandResponse>(`/api/agents/${id}/terminal-command`),
  getResumeCommand: (id: string, threadId: string) =>
    request<TerminalCommandResponse>(`/api/agents/${id}/resume-command?thread_id=${encodeURIComponent(threadId)}`),
  checkResumeProcess: (id: string, threadId: string) =>
    request<{ running: boolean; count: number }>(`/api/agents/${id}/check-resume-process?thread_id=${encodeURIComponent(threadId)}`),
  clone: (id: string) =>
    request<Agent>(`/api/agents/${id}/clone`, { method: 'POST' }),
  syncStatus: (agent_ids: string[], signal?: AbortSignal) =>
    request<{ statuses: Record<string, string> }>('/api/agents/sync-status', {
      method: 'POST',
      body: JSON.stringify({ agent_ids }),
      signal,
    }),
}

// Tasks
export interface TaskSummary {
  id: string
  agent_id: string
  title: string
  status: string
  task_dir: string
  thread_id?: string
  notification_ids: string
  user_id?: string
  username: string
  created_at: string
  started_at?: string
  completed_at?: string
}

export interface Task extends TaskSummary {
  description: string
  task_log: string
  result_md: string
}

export interface PaginatedTasks {
  items: TaskSummary[]
  total: number
  page: number
  per_page: number
}

export const tasksApi = {
  list: (agentId: string, page = 1, perPage = 20) =>
    request<PaginatedTasks>(`/api/agents/${agentId}/tasks?page=${page}&per_page=${perPage}`),
  create: (agentId: string, title: string, description: string, notification_ids?: string[]) =>
    request<Task>(`/api/agents/${agentId}/tasks`, {
      method: 'POST',
      body: JSON.stringify({ title, description, notification_ids }),
    }),
  get: (taskId: string) => request<Task>(`/api/tasks/${taskId}`),
  abort: (taskId: string) =>
    request<{ message: string; task_id: string }>(`/api/tasks/${taskId}/abort`, { method: 'POST' }),
}

export interface SimpleUser {
  id: string
  username: string
  display_name: string
  email: string
}

export const usersApi = {
  list: () => request<SimpleUser[]>('/api/users'),
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

// Agent Groups
export interface AgentGroup {
  id: string
  name: string
  agent_ids: string[]
  created_at: string
}

export const agentGroupsApi = {
  list: () => request<AgentGroup[]>('/api/agent-groups'),
  create: (data: { name: string; agent_ids?: string[] }) =>
    request<AgentGroup>('/api/agent-groups', { method: 'POST', body: JSON.stringify(data) }),
  update: (id: string, data: { name?: string; agent_ids?: string[] }) =>
    request<AgentGroup>(`/api/agent-groups/${id}`, { method: 'PUT', body: JSON.stringify(data) }),
  delete: (id: string) =>
    request<void>(`/api/agent-groups/${id}`, { method: 'DELETE' }),
}

// Plane Integration
export interface PlaneWorkspace {
  id: string
  name: string
  workspace_url: string
  api_key_masked: string
  webhook_secret_masked: string
  enabled: boolean
  created_at: string
  updated_at: string
}

export interface PlaneProject {
  id: string
  name: string
  identifier: string
}

export interface PlaneBinding {
  id: string
  workspace_id: string
  plane_project_id: string
  plane_project_name: string
  plane_project_identifier: string
  agent_group_id: string
  agent_group_name: string
  enabled: boolean
  created_at: string
}

export interface PlaneTask {
  id: string
  workspace_id: string
  plane_issue_id: string
  plane_project_id: string
  title: string
  description: string
  priority: string
  assignee_email: string
  status: string
  agent_id: string | null
  task_id: string | null
  created_at: string
  updated_at: string
}

export const planeApi = {
  // Workspaces
  listWorkspaces: () => request<PlaneWorkspace[]>('/api/plane/workspaces'),
  createWorkspace: (data: {
    name: string
    workspace_url: string
    api_key: string
    webhook_secret?: string
  }) => request<{ id: string }>('/api/plane/workspaces', { method: 'POST', body: JSON.stringify(data) }),
  updateWorkspace: (id: string, data: {
    name?: string
    workspace_url?: string
    api_key?: string
    webhook_secret?: string
  }) => request<void>(`/api/plane/workspaces/${id}`, { method: 'PUT', body: JSON.stringify(data) }),
  deleteWorkspace: (id: string) =>
    request<void>(`/api/plane/workspaces/${id}`, { method: 'DELETE' }),
  toggleWorkspace: (id: string) =>
    request<void>(`/api/plane/workspaces/${id}/toggle`, { method: 'POST' }),

  // Projects (scoped to workspace)
  listWorkspaceProjects: (workspaceId: string) =>
    request<PlaneProject[]>(`/api/plane/workspaces/${workspaceId}/projects`),

  // Bindings (scoped to workspace)
  listWorkspaceBindings: (workspaceId: string) =>
    request<PlaneBinding[]>(`/api/plane/workspaces/${workspaceId}/bindings`),
  createBinding: (workspaceId: string, data: {
    plane_project_id: string
    plane_project_name: string
    plane_project_identifier: string
    agent_group_id: string
  }) => request<{ id: string }>(`/api/plane/workspaces/${workspaceId}/bindings`, { method: 'POST', body: JSON.stringify(data) }),
  updateBinding: (id: string, data: { agent_group_id?: string }) =>
    request<void>(`/api/plane/bindings/${id}`, { method: 'PUT', body: JSON.stringify(data) }),
  deleteBinding: (id: string) =>
    request<void>(`/api/plane/bindings/${id}`, { method: 'DELETE' }),
  toggleBinding: (id: string) =>
    request<void>(`/api/plane/bindings/${id}/toggle`, { method: 'POST' }),

  listTasks: () => request<PlaneTask[]>('/api/plane/tasks'),
}
