import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from 'react'
import { integrationsApi, type IntegrationsStatus } from './api'

interface IntegrationsContextValue {
  status: IntegrationsStatus
  refresh: () => Promise<void>
}

const defaultStatus: IntegrationsStatus = { dingtalk: { enabled: false } }

const IntegrationsContext = createContext<IntegrationsContextValue>({
  status: defaultStatus,
  refresh: async () => {},
})

export function IntegrationsProvider({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<IntegrationsStatus>(defaultStatus)

  const refresh = useCallback(async () => {
    try {
      const s = await integrationsApi.status()
      setStatus(s)
    } catch {
      setStatus(defaultStatus)
    }
  }, [])

  useEffect(() => {
    refresh()
  }, [refresh])

  return (
    <IntegrationsContext.Provider value={{ status, refresh }}>
      {children}
    </IntegrationsContext.Provider>
  )
}

export function useIntegrations() {
  return useContext(IntegrationsContext)
}
