# Webhook Notifications

## Overview

Codex Fleet can send webhook notifications when task statuses change. You configure notification endpoints (webhooks), then associate them with tasks. When a status transition occurs, Codex Fleet POSTs a JSON payload to each matching webhook.

## Configuration

1. Navigate to **Notifications** in the sidebar.
2. Click **Add Webhook**.
3. Enter a name and webhook URL.
4. (Optional) Click **+ Add Header** to attach custom HTTP headers (e.g. `Authorization: Bearer ...`).
5. Select which events should trigger the webhook.
6. Save. The config is now available for association with tasks.

## Associating Notifications

### Manual Task Dispatch

When dispatching a task from the Agent Detail page, a checkbox list of enabled notification configs appears in the modal. Select the ones you want to receive notifications for that task.

## Events

Events correspond to unified status values used across the system:

| Event | Description |
|---|---|
| `agent_in_progress` | Agent has started working on the task |
| `agent_completed` | Agent finished the task successfully |
| `agent_failed` | Agent failed to complete the task |

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
    "result_md": "string | null",
    "user_id": "uuid | null",
    "username": "string",
    "created_at": "timestamp",
    "completed_at": "timestamp | null"
  }
}
```

- `event`: The status that triggered the notification.
- `task`: Always present. Contains task metadata (excludes `task_log` for payload size).
- `task.result_md`: The agent's result summary in Markdown format. Present when the agent writes a `result.md` file in the task directory upon completion; `null` otherwise.

### Example Payload

```json
{
  "event": "agent_completed",
  "task": {
    "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
    "agent_id": "b2c3d4e5-f6a7-8901-bcde-f12345678901",
    "title": "Implement login page",
    "status": "agent_completed",
    "result_md": "## Summary\nImplemented login page with email/password form ...",
    "user_id": "c3d4e5f6-a7b8-9012-cdef-123456789012",
    "username": "alice",
    "created_at": "2026-03-09 10:30:00 UTC",
    "completed_at": "2026-03-09 10:45:00 UTC"
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
