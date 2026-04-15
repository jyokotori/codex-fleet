import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Plus, Trash2, ToggleLeft, ToggleRight, Plane, List } from 'lucide-react'
import { planeApi, agentGroupsApi, type PlaneBinding, type PlaneProject, type AgentGroup, type PlaneTask } from '../lib/api'
import { useI18n } from '../hooks/useI18n'

type Tab = 'bindings' | 'tasks'

export default function PlaneIntegration() {
  const { t } = useI18n()
  const qc = useQueryClient()
  const [tab, setTab] = useState<Tab>('bindings')
  const [showModal, setShowModal] = useState(false)
  const [selectedProjectId, setSelectedProjectId] = useState('')
  const [selectedGroupId, setSelectedGroupId] = useState('')

  const { data: bindings = [] } = useQuery({ queryKey: ['plane-bindings'], queryFn: planeApi.listBindings })
  const { data: planeProjects = [], isError: projectsError } = useQuery({ queryKey: ['plane-projects'], queryFn: planeApi.listProjects })
  const { data: groups = [] } = useQuery({ queryKey: ['agent-groups'], queryFn: agentGroupsApi.list })
  const { data: tasks = [] } = useQuery({
    queryKey: ['plane-tasks'],
    queryFn: planeApi.listTasks,
    refetchInterval: 5000,
  })

  const createMut = useMutation({
    mutationFn: () => {
      const project = planeProjects.find(p => p.id === selectedProjectId)
      if (!project) throw new Error('Select a project')
      return planeApi.createBinding({
        plane_project_id: project.id,
        plane_project_name: project.name,
        plane_project_identifier: project.identifier,
        agent_group_id: selectedGroupId,
      })
    },
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['plane-bindings'] }); setShowModal(false) },
  })

  const toggleMut = useMutation({
    mutationFn: (id: string) => planeApi.toggleBinding(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['plane-bindings'] }),
  })

  const deleteMut = useMutation({
    mutationFn: (id: string) => planeApi.deleteBinding(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['plane-bindings'] }),
  })

  const statusColors: Record<string, string> = {
    pending: 'bg-yellow-100 text-yellow-800 dark:bg-yellow-900/30 dark:text-yellow-300',
    dispatched: 'bg-blue-100 text-blue-800 dark:bg-blue-900/30 dark:text-blue-300',
    completed: 'bg-green-100 text-green-800 dark:bg-green-900/30 dark:text-green-300',
    failed: 'bg-red-100 text-red-800 dark:bg-red-900/30 dark:text-red-300',
    cancelled: 'bg-gray-100 text-gray-600 dark:bg-gray-700 dark:text-gray-400',
  }

  return (
    <div className="p-6 max-w-5xl mx-auto">
      <h1 className="text-2xl font-bold flex items-center gap-2 mb-6 dark:text-white">
        <Plane size={24} /> {t.plane.title}
      </h1>

      {/* Tabs */}
      <div className="flex gap-1 mb-6 border-b dark:border-gray-700">
        <button
          onClick={() => setTab('bindings')}
          className={`px-4 py-2 text-sm font-medium border-b-2 transition-colors ${
            tab === 'bindings'
              ? 'border-sky-600 text-sky-600 dark:text-sky-400'
              : 'border-transparent text-gray-500 hover:text-gray-700 dark:text-gray-400'
          }`}
        >
          {t.plane.bindings}
        </button>
        <button
          onClick={() => setTab('tasks')}
          className={`px-4 py-2 text-sm font-medium border-b-2 transition-colors ${
            tab === 'tasks'
              ? 'border-sky-600 text-sky-600 dark:text-sky-400'
              : 'border-transparent text-gray-500 hover:text-gray-700 dark:text-gray-400'
          }`}
        >
          {t.plane.taskQueue} ({tasks.length})
        </button>
      </div>

      {/* Bindings Tab */}
      {tab === 'bindings' && (
        <div>
          <div className="flex justify-end mb-4">
            <button
              onClick={() => { setSelectedProjectId(''); setSelectedGroupId(''); setShowModal(true) }}
              className="flex items-center gap-2 px-4 py-2 bg-sky-600 text-white rounded-lg hover:bg-sky-700 text-sm"
            >
              <Plus size={16} /> {t.plane.addBinding}
            </button>
          </div>

          {projectsError && (
            <p className="text-amber-600 dark:text-amber-400 text-sm mb-4">{t.plane.notConfigured}</p>
          )}

          {bindings.length === 0 ? (
            <p className="text-gray-500 dark:text-gray-400 text-center py-12">{t.plane.noBindings}</p>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b dark:border-gray-700">
                    <th className="text-left py-3 px-4 font-medium text-gray-500 dark:text-gray-400">{t.plane.planeProject}</th>
                    <th className="text-left py-3 px-4 font-medium text-gray-500 dark:text-gray-400">{t.plane.agentGroup}</th>
                    <th className="text-center py-3 px-4 font-medium text-gray-500 dark:text-gray-400">Status</th>
                    <th className="text-right py-3 px-4 font-medium text-gray-500 dark:text-gray-400"></th>
                  </tr>
                </thead>
                <tbody>
                  {bindings.map(b => (
                    <tr key={b.id} className="border-b dark:border-gray-700 hover:bg-gray-50 dark:hover:bg-gray-800/50">
                      <td className="py-3 px-4 dark:text-white">
                        <span className="font-medium">{b.plane_project_name}</span>
                        <span className="text-gray-400 ml-2 text-xs">{b.plane_project_identifier}</span>
                      </td>
                      <td className="py-3 px-4 dark:text-gray-300">{b.agent_group_name || b.agent_group_id.slice(0, 8)}</td>
                      <td className="py-3 px-4 text-center">
                        <button onClick={() => toggleMut.mutate(b.id)} className="inline-flex items-center gap-1">
                          {b.enabled ? (
                            <><ToggleRight size={20} className="text-green-500" /> <span className="text-xs text-green-600">{t.plane.enabled}</span></>
                          ) : (
                            <><ToggleLeft size={20} className="text-gray-400" /> <span className="text-xs text-gray-500">{t.plane.disabled}</span></>
                          )}
                        </button>
                      </td>
                      <td className="py-3 px-4 text-right">
                        <button
                          onClick={() => { if (confirm('Delete?')) deleteMut.mutate(b.id) }}
                          className="p-1.5 text-gray-500 hover:text-red-600"
                        >
                          <Trash2 size={16} />
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      )}

      {/* Tasks Tab */}
      {tab === 'tasks' && (
        <div>
          {tasks.length === 0 ? (
            <p className="text-gray-500 dark:text-gray-400 text-center py-12">{t.plane.noTasks}</p>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b dark:border-gray-700">
                    <th className="text-left py-3 px-4 font-medium text-gray-500 dark:text-gray-400">Title</th>
                    <th className="text-left py-3 px-4 font-medium text-gray-500 dark:text-gray-400">Assignee</th>
                    <th className="text-left py-3 px-4 font-medium text-gray-500 dark:text-gray-400">Status</th>
                    <th className="text-left py-3 px-4 font-medium text-gray-500 dark:text-gray-400">Time</th>
                  </tr>
                </thead>
                <tbody>
                  {tasks.map(task => (
                    <tr key={task.id} className="border-b dark:border-gray-700 hover:bg-gray-50 dark:hover:bg-gray-800/50">
                      <td className="py-3 px-4 dark:text-white font-medium">{task.title}</td>
                      <td className="py-3 px-4 dark:text-gray-300 text-xs">{task.assignee_email || '-'}</td>
                      <td className="py-3 px-4">
                        <span className={`px-2 py-1 rounded text-xs font-medium ${statusColors[task.status] || ''}`}>
                          {task.status}
                        </span>
                      </td>
                      <td className="py-3 px-4 text-xs text-gray-500 dark:text-gray-400">
                        {new Date(task.created_at).toLocaleString()}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      )}

      {/* Create Binding Modal */}
      {showModal && (
        <div className="fixed inset-0 bg-black/40 flex items-center justify-center z-50" onClick={() => setShowModal(false)}>
          <div className="bg-white dark:bg-gray-800 rounded-xl shadow-xl p-6 w-full max-w-md" onClick={e => e.stopPropagation()}>
            <h2 className="text-lg font-bold mb-4 dark:text-white">{t.plane.addBinding}</h2>
            <div className="space-y-4">
              <div>
                <label className="block text-sm font-medium mb-1 dark:text-gray-300">{t.plane.planeProject}</label>
                <select
                  value={selectedProjectId}
                  onChange={e => setSelectedProjectId(e.target.value)}
                  className="w-full px-3 py-2 border rounded-lg dark:bg-gray-700 dark:border-gray-600 dark:text-white"
                >
                  <option value="">{t.plane.selectProject}</option>
                  {planeProjects.map(p => (
                    <option key={p.id} value={p.id}>{p.name} ({p.identifier})</option>
                  ))}
                </select>
              </div>
              <div>
                <label className="block text-sm font-medium mb-1 dark:text-gray-300">{t.plane.agentGroup}</label>
                <select
                  value={selectedGroupId}
                  onChange={e => setSelectedGroupId(e.target.value)}
                  className="w-full px-3 py-2 border rounded-lg dark:bg-gray-700 dark:border-gray-600 dark:text-white"
                >
                  <option value="">{t.plane.selectGroup}</option>
                  {groups.map(g => (
                    <option key={g.id} value={g.id}>{g.name}</option>
                  ))}
                </select>
              </div>
            </div>
            <div className="flex justify-end gap-2 mt-6">
              <button onClick={() => setShowModal(false)} className="px-4 py-2 text-sm text-gray-600 dark:text-gray-400">{t.common.cancel}</button>
              <button
                onClick={() => createMut.mutate()}
                disabled={!selectedProjectId || !selectedGroupId}
                className="px-4 py-2 text-sm bg-sky-600 text-white rounded-lg hover:bg-sky-700 disabled:opacity-50"
              >
                {t.common.create}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
