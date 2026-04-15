# Plane Integration

Codex Fleet can integrate with [Plane](https://plane.so) to automatically receive issues and dispatch them to agents. When an issue is moved to **Todo** in Plane, Codex Fleet picks it up, assigns it to an idle agent, and writes results back to Plane as state transitions and comments.

## Prerequisites

### 1. Plane Project States

Your Plane project **must** have the following states configured (Settings > States):

| Group | State |
|---|---|
| Backlog | Backlog |
| Unstarted | **Todo** |
| Started | **In Progress** |
| Started | **Human Review** |
| Started | **Review Failed** |
| Completed | **Done** |
| Cancelled | Cancelled |

> The state names must match exactly. Codex Fleet uses these names to transition issues through the workflow.

### 2. Environment Variables

Add the following to your `.env` file:

```env
PLANE_BASE_URL=http://your-plane-instance:8080
PLANE_WORKSPACE_SLUG=your-workspace
PLANE_API_KEY=plane_api_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
PLANE_WEBHOOK_SECRET=plane_wh_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
```

| Variable | Description |
|---|---|
| `PLANE_BASE_URL` | Base URL of your Plane instance (no trailing slash) |
| `PLANE_WORKSPACE_SLUG` | The workspace slug visible in Plane URLs |
| `PLANE_API_KEY` | API token generated from Plane workspace settings |
| `PLANE_WEBHOOK_SECRET` | Secret used to verify incoming webhook signatures |

## Setup

### 1. Create Agent Groups

Navigate to **Agent Groups** in the sidebar and create a group containing the agents you want to receive Plane tasks. An agent group is simply a named collection of agents that can be targeted by Plane bindings.

### 2. Create a Plane Binding

Navigate to **Plane** in the sidebar and click **Add Binding**:

1. Select a Plane project from the dropdown (fetched live from the Plane API).
2. Select an agent group to handle issues from that project.
3. Save. The binding is enabled by default and can be toggled on/off.

### 3. Configure the Webhook in Plane

In your Plane workspace settings, create a webhook:

- **URL**: `https://your-codex-fleet-host/api/webhooks/plane`
- **Secret**: Same value as `PLANE_WEBHOOK_SECRET` in your `.env`
- **Events**: Enable **Work Items** only

> The webhook endpoint is public (no auth required) — it verifies requests using HMAC-SHA256 signature validation.

### 4. User Email Matching (Optional)

When a Plane issue has an assignee, Codex Fleet matches the assignee's email against the `email` field on Codex Fleet users. If a match is found, the task is dispatched to an idle agent **owned by that user** within the bound agent group. If no match is found (or the issue has no assignee), any idle agent in the group can pick it up.

To enable this, make sure your Codex Fleet users have their email set (via admin user management or profile settings) and that these emails match their Plane account emails.

## Workflow

```
Plane                          Codex Fleet                      Plane
─────                          ───────────                      ─────
Issue moved to "Todo"
        ──webhook──>   plane_tasks queue (pending)
                       scheduler finds idle agent
                       dispatches task
                              ──state update──>        "In Progress"
                       agent works...
                       agent completes
                              ──state update──>        "Human Review"
                              ──comment──>             agent result
                       human approves in Codex Fleet
                              ──state update──>        "Done"
                       (or human rejects)
                              ──state update──>        "Review Failed"
```

### State Transitions

| Codex Fleet Event | Plane State Change |
|---|---|
| Task dispatched to agent | Todo → **In Progress** |
| Agent completed | In Progress → **Human Review** |
| Agent failed | In Progress → **Review Failed** |
| Human approved | Human Review → **Done** |
| Human rejected | Human Review → **Review Failed** |

### Re-entry

After a **Review Failed**, you can move the issue back to **Todo** in Plane to trigger a new dispatch cycle. Codex Fleet does not deduplicate — the same issue can be queued multiple times.

## Troubleshooting

- **Webhook not firing**: Make sure the webhook in Plane is configured for **Work Items** events and the URL is reachable from your Plane instance.
- **Signature mismatch**: Check that `PLANE_WEBHOOK_SECRET` matches the secret configured in Plane. Look for `Plane webhook signature mismatch` in backend logs.
- **State not updating**: Ensure the exact state names exist in your Plane project. Look for `Plane write-back` warnings in backend logs.
- **No agent picked up**: The scheduler runs every 10 seconds. Check that the binding is enabled, the agent group has running agents, and agents are idle (no in-progress tasks).
