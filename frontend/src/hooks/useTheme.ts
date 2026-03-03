import { useState, useEffect } from 'react'
import { type Theme, getStoredTheme, setStoredTheme, applyTheme, getSystemTheme } from '../lib/theme'

export function useTheme() {
  const [theme, setThemeState] = useState<Theme>(getStoredTheme)

  function setTheme(next: Theme) {
    setThemeState(next)
    setStoredTheme(next)
    applyTheme(next)
  }

  // Re-apply when system preference changes (only relevant when theme === 'system')
  useEffect(() => {
    const mq = window.matchMedia('(prefers-color-scheme: dark)')
    const handler = () => { if (getStoredTheme() === 'system') applyTheme('system') }
    mq.addEventListener('change', handler)
    return () => mq.removeEventListener('change', handler)
  }, [])

  const resolved = theme === 'system' ? getSystemTheme() : theme
  return { theme, resolved, setTheme }
}
