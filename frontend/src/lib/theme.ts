export type Theme = 'light' | 'dark' | 'system'

export function getSystemTheme(): 'light' | 'dark' {
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
}

export function getStoredTheme(): Theme {
  const v = localStorage.getItem('theme')
  if (v === 'light' || v === 'dark') return v
  return 'system'
}

export function setStoredTheme(theme: Theme) {
  if (theme === 'system') localStorage.removeItem('theme')
  else localStorage.setItem('theme', theme)
}

export function applyTheme(theme: Theme) {
  const resolved = theme === 'system' ? getSystemTheme() : theme
  document.documentElement.classList.toggle('dark', resolved === 'dark')
}
