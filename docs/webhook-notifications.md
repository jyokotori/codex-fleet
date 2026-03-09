# Webhook Notifications

## Overview

Codex Fleet can send webhook notifications when task statuses change. You configure notification endpoints (webhooks), then associate them with tasks or work items. When a status transition occurs, Codex Fleet POSTs a JSON payload to each matching webhook.

## Configuration

1. Navigate to **Notifications** in the sidebar.
2. Click **Add Webhook**.
3. Enter a name and webhook URL.
4. (Optional) Click **+ Add Header** to attach custom HTTP headers (e.g. `Authorization: Bearer ...`).
5. Select which events should trigger the webhook.
6. Save. The config is now available for association with tasks and work items.

## Associating Notifications

### Manual Task Dispatch

When dispatching a task from the Agent Detail page, a checkbox list of enabled notification configs appears in the modal. Select the ones you want to receive notifications for that task.

### Work Items

When creating or editing a work item in the Requirements page, select notification configs from the checkbox list. When the scheduler dispatches a task for that work item, the notification config IDs are copied to the task. Notifications fire on task completion, failure, approval, or rejection.

## Events

Events correspond to unified status values used across the system:

| Event | Description |
|---|---|
| `waiting` | Task/work item is waiting to be picked up |
| `agent_in_progress` | Agent has started working on the task |
| `agent_completed` | Agent finished the task successfully |
| `agent_failed` | Agent failed to complete the task |
| `human_approved` | A human reviewer approved the completed task |
| `human_rejected` | A human reviewer rejected the completed task |
| `cancelled` | Task was cancelled |
| `closed` | Task was closed |

## Webhook Payload

The webhook receives a `POST` request with `Content-Type: application/json`.

### Schema

```json
{
  "event": "<status>",
  "task": {
    "id": "uuid",
    "agent_id": "uuid",
    "title": "string",
    "status": "<status>",
    "user_id": "uuid | null",
    "username": "string",
    "created_at": "timestamp",
    "completed_at": "timestamp | null"
  },
  "work_item": {
    "id": "uuid",
    "project_id": "uuid",
    "title": "string",
    "status": "<status>",
    "priority": "low | medium | high | urgent",
    "assigned_agent_id": "uuid | null"
  }
}
```

- `event`: The status that triggered the notification.
- `task`: Always present. Contains task metadata (excludes `task_log` for payload size).
- `work_item`: Present only if the task is linked to a work item.

### Example Payload

```json
{
  "event": "agent_completed",
  "task": {
    "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
    "agent_id": "b2c3d4e5-f6a7-8901-bcde-f12345678901",
    "title": "Implement login page",
    "status": "agent_completed",
    "user_id": "c3d4e5f6-a7b8-9012-cdef-123456789012",
    "username": "alice",
    "created_at": "2026-03-09 10:30:00 UTC",
    "completed_at": "2026-03-09 10:45:00 UTC"
  },
  "work_item": {
    "id": "d4e5f6a7-b8c9-0123-defa-234567890123",
    "project_id": "e5f6a7b8-c9d0-1234-efab-345678901234",
    "title": "Login feature",
    "status": "agent_completed",
    "priority": "high",
    "assigned_agent_id": "b2c3d4e5-f6a7-8901-bcde-f12345678901"
  }
}
```

## Custom Headers

Custom headers can be configured directly in the notification create/edit modal via the **Custom Headers** section. Click **+ Add Header** to add key-value pairs. These headers are sent with every webhook POST request.

Common use cases:
- `Authorization: Bearer <token>` — authenticate with the receiving service
- `X-Webhook-Secret: <secret>` — verify request origin on the receiver side

## Error Handling

- Webhook delivery is fire-and-forget. Failures are logged but do not affect task execution.
- There is no retry mechanism. If the webhook endpoint is unreachable, the notification is lost.
- Check your application logs for `Webhook notification failed` messages to diagnose delivery issues.
