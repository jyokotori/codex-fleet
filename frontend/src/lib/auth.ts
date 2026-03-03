export interface SessionUser {
  id: string
  username: string
  display_name: string
  status: string
  roles: string[]
}

export interface LoginPayload {
  access_token: string
  refresh_token: string
  expires_in: number
  user: SessionUser
}

export interface StoredAuth {
  token: string
  refresh_token: string
  user: SessionUser
}

export function saveAuth(payload: LoginPayload) {
  localStorage.setItem('token', payload.access_token)
  localStorage.setItem('refresh_token', payload.refresh_token)
  localStorage.setItem('user', JSON.stringify(payload.user))
}

export function getAuth(): StoredAuth | null {
  const userRaw = localStorage.getItem('user')
  const token = localStorage.getItem('token')
  const refreshToken = localStorage.getItem('refresh_token')
  if (!userRaw || !token || !refreshToken) return null
  try {
    const user = JSON.parse(userRaw) as SessionUser
    return { token, refresh_token: refreshToken, user }
  } catch {
    return null
  }
}

export function clearAuth() {
  localStorage.removeItem('token')
  localStorage.removeItem('refresh_token')
  localStorage.removeItem('user')
}

export function isAuthenticated(): boolean {
  return !!getAuth()
}

export function isAdmin(): boolean {
  const auth = getAuth()
  if (!auth) return false
  return auth.user.roles.includes('admin')
}
