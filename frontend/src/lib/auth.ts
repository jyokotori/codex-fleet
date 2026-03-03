export interface AuthUser {
  token: string
  user_id: string
  username: string
  display_name: string
}

export function saveAuth(user: AuthUser) {
  localStorage.setItem('token', user.token)
  localStorage.setItem('user', JSON.stringify(user))
}

export function getAuth(): AuthUser | null {
  const user = localStorage.getItem('user')
  const token = localStorage.getItem('token')
  if (!user || !token) return null
  try {
    return JSON.parse(user)
  } catch {
    return null
  }
}

export function clearAuth() {
  localStorage.removeItem('token')
  localStorage.removeItem('user')
}

export function isAuthenticated(): boolean {
  return !!getAuth()
}
