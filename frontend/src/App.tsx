import { Routes, Route, Navigate } from 'react-router-dom'
import { isAdmin, isAuthenticated } from './lib/auth'
import Login from './pages/Login'
import Dashboard from './pages/Dashboard'
import Servers from './pages/Servers'
import Agents from './pages/Agents'
import AgentDetail from './pages/AgentDetail'
import ConfigsLayout from './pages/configs/ConfigsLayout'
import CodexConfigs from './pages/configs/CodexConfigs'
import AgentsMd from './pages/configs/AgentsMd'
import DockerConfigs from './pages/configs/DockerConfigs'
import WIPSection from './pages/configs/WIPSection'
import Notifications from './pages/Notifications'
import Requirements from './pages/Requirements'
import RequirementDetail from './pages/RequirementDetail'
import Users from './pages/admin/Users'
import Layout from './components/Layout'

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
  return (
    <Routes>
      <Route path="/login" element={<Login />} />
      <Route
        path="/"
        element={
          <ProtectedRoute>
            <Layout />
          </ProtectedRoute>
        }
      >
        <Route index element={<Dashboard />} />
        <Route path="servers" element={<Servers />} />
        <Route path="agents" element={<Agents />} />
        <Route path="agents/:id" element={<AgentDetail />} />
        <Route path="configs" element={<ConfigsLayout />}>
          <Route index element={<Navigate to="config-files/codex" replace />} />
          <Route path="config-files/codex" element={<CodexConfigs />} />
          <Route path="config-files/:type" element={<WIPSection />} />
          <Route path="agents-md" element={<AgentsMd />} />
          <Route path="docker" element={<DockerConfigs />} />
          <Route path="skills" element={<WIPSection />} />
          <Route path="mcp" element={<WIPSection />} />
        </Route>
        <Route path="notifications" element={<Notifications />} />
        <Route path="requirements" element={<Requirements />} />
        <Route path="requirements/:projectId" element={<RequirementDetail />} />
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
