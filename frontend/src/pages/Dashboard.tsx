import { useQuery } from '@tanstack/react-query'
import { Link } from 'react-router-dom'
import { Server, Bot, CheckCircle, AlertCircle, Clock, Activity } from 'lucide-react'
import { serversApi, agentsApi } from '../lib/api'
import { isAdmin as hasAdminRole } from '../lib/auth'
import { useI18n } from '../hooks/useI18n'

function StatCard({
  label, value, icon: Icon, color,
}: {
  label: string; value: number | string
  icon: React.ComponentType<{ size?: number; className?: string }>; color: string
}) {
  return (
    <div className="card flex items-center gap-4">
      <div className={`w-12 h-12 rounded-lg flex items-center justify-center ${color}`}>
        <Icon size={22} className="text-white" />
      </div>
      <div>
        <p className="text-2xl font-bold text-gray-900 dark:text-white">{value}</p>
        <p className="text-sm text-gray-500">{label}</p>
      </div>
    </div>
  )
}

export default function Dashboard() {
  const { t } = useI18n()
  const isAdmin = hasAdminRole()
  const { data: servers = [] } = useQuery({
    queryKey: ['servers'],
    queryFn: serversApi.list,
    enabled: isAdmin,
    retry: false,
  })
  const { data: agents = [] } = useQuery({ queryKey: ['agents'], queryFn: agentsApi.list })

  const runningAgents = agents.filter(a => a.status === 'running').length
  const stoppedAgents = agents.filter(a => a.status === 'stopped').length
  const errorAgents = agents.filter(a => a.status === 'error').length
  const onlineServers = servers.filter(s => s.status === 'online').length

  return (
    <div className="p-8">
      <div className="mb-8">
        <h1 className="text-2xl font-bold text-gray-900 dark:text-white">{t.dashboard.title}</h1>
        <p className="text-gray-500 mt-1">{t.dashboard.subtitle}</p>
      </div>

      <div className={`grid grid-cols-2 gap-4 mb-8 ${isAdmin ? 'lg:grid-cols-4' : 'lg:grid-cols-2'}`}>
        {isAdmin && <StatCard label={t.dashboard.totalServers} value={servers.length} icon={Server} color="bg-blue-600" />}
        {isAdmin && <StatCard label={t.dashboard.onlineServers} value={onlineServers} icon={CheckCircle} color="bg-green-600" />}
        <StatCard label={t.dashboard.runningAgents} value={runningAgents} icon={Activity} color="bg-sky-600" />
        <StatCard label={t.dashboard.totalAgents} value={agents.length} icon={Bot} color="bg-purple-600" />
      </div>

      <div className={`grid grid-cols-1 gap-6 ${isAdmin ? 'lg:grid-cols-2' : ''}`}>
        {/* Agents */}
        <div className="card">
          <div className="flex items-center justify-between mb-4">
            <h2 className="font-semibold text-gray-800 dark:text-gray-100">{t.dashboard.recentAgents}</h2>
            <Link to="/agents" className="text-sky-500 text-sm hover:underline">{t.dashboard.viewAll}</Link>
          </div>
          {agents.length === 0 ? (
            <p className="text-gray-500 text-sm py-4 text-center">{t.dashboard.noAgents}</p>
          ) : (
            <div className="space-y-2">
              {agents.slice(0, 5).map(agent => (
                <Link
                  key={agent.id}
                  to={`/agents/${agent.id}`}
                  className="flex items-center justify-between p-3 rounded-lg bg-gray-100 hover:bg-gray-200 dark:bg-gray-800 dark:hover:bg-gray-700 transition-colors"
                >
                  <div className="flex items-center gap-3">
                    <Bot size={16} className="text-gray-400" />
                    <div>
                      <p className="text-sm font-medium text-gray-700 dark:text-gray-200">{agent.name}</p>
                      <p className="text-xs text-gray-500">{agent.cli_inits.map(ci => ci.cli_type).join(', ') || '—'}</p>
                    </div>
                  </div>
                  <AgentStatusBadge status={agent.status} t={t} />
                </Link>
              ))}
            </div>
          )}
        </div>

        {isAdmin && (
          <div className="card">
            <div className="flex items-center justify-between mb-4">
              <h2 className="font-semibold text-gray-800 dark:text-gray-100">{t.dashboard.recentServers}</h2>
              <Link to="/servers" className="text-sky-500 text-sm hover:underline">{t.dashboard.viewAll}</Link>
            </div>
            {servers.length === 0 ? (
              <p className="text-gray-500 text-sm py-4 text-center">{t.dashboard.noServers}</p>
            ) : (
              <div className="space-y-2">
                {servers.slice(0, 5).map(server => (
                  <div key={server.id} className="flex items-center justify-between p-3 rounded-lg bg-gray-100 dark:bg-gray-800">
                    <div className="flex items-center gap-3">
                      <Server size={16} className="text-gray-400" />
                      <div>
                        <p className="text-sm font-medium text-gray-700 dark:text-gray-200">{server.name}</p>
                        <p className="text-xs text-gray-500">{server.ip}:{server.port}</p>
                      </div>
                    </div>
                    <ServerStatusBadge status={server.status} t={t} />
                  </div>
                ))}
              </div>
            )}
          </div>
        )}
      </div>

      {agents.length > 0 && (
        <div className="mt-6 card">
          <h2 className="font-semibold text-gray-800 dark:text-gray-100 mb-4">{t.dashboard.agentSummary}</h2>
          <div className="flex gap-6">
            <div className="flex items-center gap-2">
              <Activity size={14} className="text-green-500" />
              <span className="text-sm text-gray-600 dark:text-gray-400">{runningAgents} {t.dashboard.running}</span>
            </div>
            <div className="flex items-center gap-2">
              <Clock size={14} className="text-gray-400" />
              <span className="text-sm text-gray-600 dark:text-gray-400">{stoppedAgents} {t.dashboard.stopped}</span>
            </div>
            {errorAgents > 0 && (
              <div className="flex items-center gap-2">
                <AlertCircle size={14} className="text-red-400" />
                <span className="text-sm text-gray-600 dark:text-gray-400">{errorAgents} {t.status.error}</span>
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  )
}

function AgentStatusBadge({ status, t }: { status: string; t: ReturnType<typeof useI18n>['t'] }) {
  const map: Record<string, string> = { running: 'badge-green', stopped: 'badge-gray', error: 'badge-red', provisioning: 'badge-yellow' }
  return <span className={map[status] ?? 'badge-gray'}>{t.status[status as keyof typeof t.status] ?? status}</span>
}

function ServerStatusBadge({ status, t }: { status: string; t: ReturnType<typeof useI18n>['t'] }) {
  const map: Record<string, string> = { online: 'badge-green', offline: 'badge-red', unknown: 'badge-gray' }
  return <span className={map[status] ?? 'badge-gray'}>{t.status[status as keyof typeof t.status] ?? status}</span>
}
