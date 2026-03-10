import type { Agent } from './api'

export type AgentRuntimeAction = 'start' | 'pause' | 'restart'

export function getAgentRuntimeAction(agent: Agent): AgentRuntimeAction | null {
  if (agent.status === 'provisioning' || agent.status === 'provision_failed') return null

  if (agent.use_docker) {
    if (agent.status === 'running') return 'pause'
    if (agent.status === 'stopped') return 'start'
    return 'restart'
  }

  return null
}

export function canDispatchTask(agent: Agent): boolean {
  if (agent.is_busy) return false
  if (agent.status === 'provisioning' || agent.status === 'provision_failed' || agent.status === 'error') {
    return false
  }

  if (agent.use_docker) {
    return agent.status === 'running'
  }

  return agent.status === 'stopped' || agent.status === 'running'
}
