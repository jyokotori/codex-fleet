import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { Plus, Trash2, Edit2, ClipboardList, Archive, ArchiveRestore } from 'lucide-react'
import { projectsApi, type Project } from '../lib/api'
import { useI18n } from '../hooks/useI18n'

interface ProjectFormData {
  name: string
  description: string
}

const defaultForm: ProjectFormData = { name: '', description: '' }

export default function Requirements() {
  const qc = useQueryClient()
  const navigate = useNavigate()
  const { t } = useI18n()
  const tr = t.requirements

  const [showModal, setShowModal] = useState(false)
  const [editProject, setEditProject] = useState<Project | null>(null)
  const [form, setForm] = useState<ProjectFormData>(defaultForm)

  const { data: projects = [], isLoading } = useQuery({
    queryKey: ['projects'],
    queryFn: projectsApi.list,
  })

  const createMutation = useMutation({
    mutationFn: (data: ProjectFormData) => projectsApi.create(data),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['projects'] }); closeModal() },
  })

  const updateMutation = useMutation({
    mutationFn: ({ id, data }: { id: string; data: Partial<ProjectFormData & { status: string }> }) =>
      projectsApi.update(id, data),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ['projects'] }); closeModal() },
  })

  const deleteMutation = useMutation({
    mutationFn: projectsApi.delete,
    onSuccess: () => qc.invalidateQueries({ queryKey: ['projects'] }),
  })

  function openCreate() { setEditProject(null); setForm(defaultForm); setShowModal(true) }
  function openEdit(p: Project) {
    setEditProject(p)
    setForm({ name: p.name, description: p.description })
    setShowModal(true)
  }
  function closeModal() { setShowModal(false); setEditProject(null); setForm(defaultForm) }

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    if (editProject) updateMutation.mutate({ id: editProject.id, data: form })
    else createMutation.mutate(form)
  }

  function toggleArchive(p: Project) {
    updateMutation.mutate({
      id: p.id,
      data: { status: p.status === 'active' ? 'archived' : 'active' },
    })
  }

  const isPending = createMutation.isPending || updateMutation.isPending

  return (
    <div className="p-8">
      <div className="flex items-center justify-between mb-8">
        <div>
          <h1 className="text-2xl font-bold text-gray-900 dark:text-white">{tr.title}</h1>
          <p className="text-gray-500 mt-1">{tr.subtitle}</p>
        </div>
        <button
          onClick={openCreate}
          className="flex items-center gap-2 px-4 py-2 bg-sky-600 hover:bg-sky-700 text-white rounded-lg text-sm font-medium transition-colors"
        >
          <Plus size={16} />
          {tr.addProject}
        </button>
      </div>

      {isLoading ? (
        <div className="text-gray-500">{t.common.loading}</div>
      ) : projects.length === 0 ? (
        <div className="text-center py-16">
          <ClipboardList size={40} className="mx-auto text-gray-300 dark:text-gray-600 mb-3" />
          <p className="text-gray-500">{tr.noProjects}</p>
          <p className="text-sm text-gray-400 mt-1">{tr.noProjectsHint}</p>
        </div>
      ) : (
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
          {projects.map((p) => (
            <div
              key={p.id}
              className="bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-800 rounded-xl p-5 hover:border-sky-300 dark:hover:border-sky-700 transition-colors cursor-pointer group"
              onClick={() => navigate(`/requirements/${p.id}`)}
            >
              <div className="flex items-start justify-between gap-2 mb-3">
                <div className="flex items-center gap-2 min-w-0">
                  <ClipboardList size={16} className="text-sky-500 shrink-0" />
                  <h3 className="font-semibold text-gray-900 dark:text-white truncate">{p.name}</h3>
                </div>
                <span className={`shrink-0 text-xs px-2 py-0.5 rounded-full font-medium ${
                  p.status === 'active'
                    ? 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400'
                    : 'bg-gray-100 text-gray-500 dark:bg-gray-800 dark:text-gray-400'
                }`}>
                  {p.status === 'active' ? tr.active : tr.archived}
                </span>
              </div>

              {p.description && (
                <p className="text-sm text-gray-500 dark:text-gray-400 line-clamp-2 mb-4">{p.description}</p>
              )}

              <div
                className="flex items-center gap-1 mt-2 opacity-0 group-hover:opacity-100 transition-opacity"
                onClick={(e) => e.stopPropagation()}
              >
                <button
                  onClick={() => openEdit(p)}
                  className="p-1.5 text-gray-400 hover:text-sky-600 hover:bg-sky-50 dark:hover:bg-sky-900/20 rounded-lg transition-colors"
                  title={t.common.edit}
                >
                  <Edit2 size={14} />
                </button>
                <button
                  onClick={() => toggleArchive(p)}
                  className="p-1.5 text-gray-400 hover:text-amber-600 hover:bg-amber-50 dark:hover:bg-amber-900/20 rounded-lg transition-colors"
                  title={p.status === 'active' ? tr.archive : tr.unarchive}
                >
                  {p.status === 'active' ? <Archive size={14} /> : <ArchiveRestore size={14} />}
                </button>
                <button
                  onClick={() => {
                    if (confirm(`${t.common.delete} "${p.name}"?`)) deleteMutation.mutate(p.id)
                  }}
                  className="p-1.5 text-gray-400 hover:text-red-600 hover:bg-red-50 dark:hover:bg-red-900/20 rounded-lg transition-colors"
                  title={t.common.delete}
                >
                  <Trash2 size={14} />
                </button>
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Modal */}
      {showModal && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4">
          <div className="bg-white dark:bg-gray-900 rounded-xl shadow-xl w-full max-w-md">
            <div className="flex items-center justify-between p-6 border-b border-gray-100 dark:border-gray-800">
              <h2 className="text-lg font-semibold text-gray-900 dark:text-white">
                {editProject ? tr.editProject : tr.addProject}
              </h2>
              <button onClick={closeModal} className="text-gray-400 hover:text-gray-600 dark:hover:text-gray-200">✕</button>
            </div>
            <form onSubmit={handleSubmit} className="p-6 space-y-4">
              <div>
                <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">{tr.projectName}</label>
                <input
                  type="text"
                  value={form.name}
                  onChange={(e) => setForm({ ...form, name: e.target.value })}
                  required
                  className="w-full px-3 py-2 border border-gray-300 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-white text-sm focus:ring-2 focus:ring-sky-500 focus:border-transparent"
                />
              </div>
              <div>
                <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">{tr.projectDescription}</label>
                <textarea
                  value={form.description}
                  onChange={(e) => setForm({ ...form, description: e.target.value })}
                  rows={3}
                  className="w-full px-3 py-2 border border-gray-300 dark:border-gray-700 rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-white text-sm focus:ring-2 focus:ring-sky-500 focus:border-transparent resize-none"
                />
              </div>
              <div className="flex gap-3 pt-2">
                <button type="button" onClick={closeModal} className="flex-1 px-4 py-2 border border-gray-300 dark:border-gray-700 rounded-lg text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors">
                  {t.common.cancel}
                </button>
                <button type="submit" disabled={isPending} className="flex-1 px-4 py-2 bg-sky-600 hover:bg-sky-700 text-white rounded-lg text-sm font-medium transition-colors disabled:opacity-60">
                  {isPending ? t.common.loading : (editProject ? t.common.save : t.common.create)}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  )
}
