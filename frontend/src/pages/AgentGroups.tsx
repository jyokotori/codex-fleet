import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Plus, Trash2, Edit2, Group } from 'lucide-react'
import { agentGroupsApi, agentsApi, type AgentGroup, type Agent } from '../lib/api'
import { useI18n } from '../hooks/useI18n'

export default function AgentGroups() {
  const { t } = useI18n()
  const qc = useQueryClient()
  const [showModal, setShowModal] = useState(false)
  const [editing, setEditing] = useState<AgentGroup | null>(null)
  const [name, setName] = useState('')
  const [selectedAgentIds, setSelectedAgentIds] = useState<string[]>([])

  const { data: groups = [] } = useQuery({ queryKey: ['agent-groups'], queryFn: agentGroupsApi.list })
  const { data: agents = [] } = useQuery({ queryKey: ['agents-list'], queryFn: agentsApi.list })

  const createMut = useMutation({
    mutationFn: (data: { name: string; agent_ids: string[] }) => agentGroupsApi.create(data),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['agent-groups'] }); closeModal() },
  })
  const updateMut = useMutation({
    mutationFn: ({ id, data }: { id: string; data: { name?: string; agent_ids?: string[] } }) => agentGroupsApi.update(id, data),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['agent-groups'] }); closeModal() },
  })
  const deleteMut = useMutation({
    mutationFn: (id: string) => agentGroupsApi.delete(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['agent-groups'] }),
  })

  function openCreate() {
    setEditing(null); setName(''); setSelectedAgentIds([]); setShowModal(true)
  }
  function openEdit(g: AgentGroup) {
    setEditing(g); setName(g.name); setSelectedAgentIds(g.agent_ids); setShowModal(true)
  }
  function closeModal() { setShowModal(false); setEditing(null) }

  function handleSave() {
    if (editing) {
      updateMut.mutate({ id: editing.id, data: { name, agent_ids: selectedAgentIds } })
    } else {
      createMut.mutate({ name, agent_ids: selectedAgentIds })
    }
  }

  function toggleAgent(id: string) {
    setSelectedAgentIds(prev =>
      prev.includes(id) ? prev.filter(a => a !== id) : [...prev, id]
    )
  }

  return (
    <div className="p-6 max-w-4xl mx-auto">
      <div className="flex items-center justify-between mb-6">
        <h1 className="text-2xl font-bold flex items-center gap-2 dark:text-white">
          <Group size={24} /> {t.agentGroups.title}
        </h1>
        <button
          onClick={openCreate}
          className="flex items-center gap-2 px-4 py-2 bg-sky-600 text-white rounded-lg hover:bg-sky-700 text-sm"
        >
          <Plus size={16} /> {t.agentGroups.addGroup}
        </button>
      </div>

      {groups.length === 0 ? (
        <p className="text-gray-500 dark:text-gray-400 text-center py-12">{t.agentGroups.noGroups}</p>
      ) : (
        <div className="space-y-4">
          {groups.map(g => (
            <div key={g.id} className="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-4">
              <div className="flex items-center justify-between mb-2">
                <h3 className="font-semibold text-lg dark:text-white">{g.name}</h3>
                <div className="flex items-center gap-2">
                  <button onClick={() => openEdit(g)} className="p-1.5 text-gray-500 hover:text-sky-600 dark:text-gray-400"><Edit2 size={16} /></button>
                  <button onClick={() => { if (confirm('Delete this group?')) deleteMut.mutate(g.id) }} className="p-1.5 text-gray-500 hover:text-red-600 dark:text-gray-400"><Trash2 size={16} /></button>
                </div>
              </div>
              <div className="flex flex-wrap gap-2">
                {g.agent_ids.length === 0 ? (
                  <span className="text-sm text-gray-400">{t.agentGroups.members}: 0</span>
                ) : (
                  g.agent_ids.map(aid => {
                    const agent = agents.find(a => a.id === aid)
                    return (
                      <span key={aid} className="px-2 py-1 bg-sky-50 text-sky-700 rounded text-xs dark:bg-sky-900/30 dark:text-sky-300">
                        {agent?.name || aid.slice(0, 8)}
                      </span>
                    )
                  })
                )}
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Modal */}
      {showModal && (
        <div className="fixed inset-0 bg-black/40 flex items-center justify-center z-50" onClick={closeModal}>
          <div className="bg-white dark:bg-gray-800 rounded-xl shadow-xl p-6 w-full max-w-md" onClick={e => e.stopPropagation()}>
            <h2 className="text-lg font-bold mb-4 dark:text-white">
              {editing ? t.agentGroups.editGroup : t.agentGroups.addGroup}
            </h2>
            <div className="space-y-4">
              <div>
                <label className="block text-sm font-medium mb-1 dark:text-gray-300">{t.agentGroups.groupName}</label>
                <input
                  value={name}
                  onChange={e => setName(e.target.value)}
                  className="w-full px-3 py-2 border rounded-lg dark:bg-gray-700 dark:border-gray-600 dark:text-white"
                  placeholder={t.agentGroups.groupName}
                />
              </div>
              <div>
                <label className="block text-sm font-medium mb-1 dark:text-gray-300">{t.agentGroups.selectAgents}</label>
                <div className="max-h-48 overflow-y-auto border rounded-lg p-2 dark:border-gray-600 space-y-1">
                  {agents.map(a => (
                    <label key={a.id} className="flex items-center gap-2 px-2 py-1.5 rounded hover:bg-gray-50 dark:hover:bg-gray-700 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={selectedAgentIds.includes(a.id)}
                        onChange={() => toggleAgent(a.id)}
                        className="rounded"
                      />
                      <span className="text-sm dark:text-gray-300">{a.name}</span>
                      <span className={`ml-auto text-xs px-1.5 py-0.5 rounded ${a.status === 'running' ? 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400' : 'bg-gray-100 text-gray-500 dark:bg-gray-700 dark:text-gray-400'}`}>
                        {a.status}
                      </span>
                    </label>
                  ))}
                </div>
              </div>
            </div>
            <div className="flex justify-end gap-2 mt-6">
              <button onClick={closeModal} className="px-4 py-2 text-sm text-gray-600 dark:text-gray-400">{t.common.cancel}</button>
              <button
                onClick={handleSave}
                disabled={!name.trim()}
                className="px-4 py-2 text-sm bg-sky-600 text-white rounded-lg hover:bg-sky-700 disabled:opacity-50"
              >
                {editing ? t.common.update : t.common.create}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
