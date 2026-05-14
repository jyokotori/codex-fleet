import { useEffect } from 'react'
import { Routes, Route, Navigate } from 'react-router-dom'
import { isAdmin, isAuthenticated, getTokenRemainingSeconds } from './lib/auth'
import { scheduleTokenRefresh } from './lib/api'
import Login from './pages/Login'
import Dashboard from './pages/Dashboard'
import Servers from './pages/Servers'
import Agents from './pages/Agents'
import AgentDetail from './pages/AgentDetail'
import AgentsLayout from './pages/agents/AgentsLayout'
import ConfigsLayout from './pages/configs/ConfigsLayout'
import CodexConfigs from './pages/configs/CodexConfigs'
import ClaudeConfigs from './pages/configs/ClaudeConfigs'
import AgentsMd from './pages/configs/AgentsMd'
import DockerConfigs from './pages/configs/DockerConfigs'
import WIPSection from './pages/configs/WIPSection'
import Notifications from './pages/Notifications'
import AgentGroups from './pages/AgentGroups'
import PlaneIntegration from './pages/PlaneIntegration'
import Users from './pages/admin/Users'
import IntegrationDingTalk from './pages/admin/IntegrationDingTalk'
import IntegrationApiToken from './pages/admin/IntegrationApiToken'
import Layout from './components/Layout'
import { IntegrationsProvider } from './lib/integrations'

function ProtectedRoute({ children }: { children: React.ReactNode }) {
  if (!isAuthenticated()) {
    return <Navigate to="/login" replace />
  }
  return <>{children}</>
}

function AdminRoute({ children }: { children: React.ReactNode }) {
  if (!isAuthenticated()) {
    return <Navigate to="/login" replace />
  }
  if (!isAdmin()) {
    return <Navigate to="/" replace />
  }
  return <>{children}</>
}

export default function App() {
  useEffect(() => {
    const remaining = getTokenRemainingSeconds()
    if (remaining !== null && remaining > 0) {
      scheduleTokenRefresh(remaining)
    }
  }, [])

  return (
    <Routes>
      <Route path="/login" element={<Login />} />
      <Route
        path="/"
        element={
          <ProtectedRoute>
            <IntegrationsProvider>
              <Layout />
            </IntegrationsProvider>
          </ProtectedRoute>
        }
      >
        <Route index element={<Dashboard />} />
        <Route path="agents" element={<AgentsLayout />}>
          <Route index element={<Agents />} />
          <Route path="groups" element={<AgentGroups />} />
          <Route path="servers" element={<AdminRoute><Servers /></AdminRoute>} />
        </Route>
        <Route path="agents/:id" element={<AgentDetail />} />
        <Route path="configs" element={<ConfigsLayout />}>
          <Route index element={<Navigate to="config-files/codex" replace />} />
          <Route path="config-files/codex" element={<CodexConfigs />} />
          <Route path="config-files/claude-code" element={<ClaudeConfigs />} />
          <Route path="config-files/:type" element={<WIPSection />} />
          <Route path="agents-md" element={<AgentsMd />} />
          <Route path="docker" element={<DockerConfigs />} />
          <Route path="skills" element={<WIPSection />} />
          <Route path="mcp" element={<WIPSection />} />
          <Route
            path="integrations/dingtalk"
            element={<AdminRoute><IntegrationDingTalk /></AdminRoute>}
          />
          <Route
            path="integrations/api-token"
            element={<AdminRoute><IntegrationApiToken /></AdminRoute>}
          />
        </Route>
        <Route path="notifications" element={<Notifications />} />
        <Route path="plane" element={<PlaneIntegration />} />
        <Route
          path="admin/users"
          element={(
            <AdminRoute>
              <Users />
            </AdminRoute>
          )}
        />
      </Route>
    </Routes>
  )
}
