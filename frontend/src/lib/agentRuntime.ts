import type { Agent } from './api'

export type AgentRuntimeAction = 'start' | 'pause' | 'restart'

export function getAgentRuntimeAction(agent: Agent): AgentRuntimeAction | null {
  if (agent.status === 'provisioning') return null

  if (agent.use_docker) {
    if (agent.runtime_action === 'start' || agent.runtime_action === 'pause' || agent.runtime_action === 'restart') {
      return agent.runtime_action
    }

    if (agent.status === 'running') return 'pause'
    if (agent.status === 'stopped') return 'start'
    return 'restart'
  }

  return null
}

export function canDispatchTask(agent: Agent): boolean {
  if (agent.status === 'provisioning' || agent.status === 'error') {
    return false
  }

  if (agent.use_docker) {
    return getAgentRuntimeAction(agent) === 'pause'
  }

  return agent.status === 'stopped' || agent.status === 'running'
}
