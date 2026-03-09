# External API

The External API provides login-free HTTP endpoints authenticated via a custom header and secret. It is designed for automation scripts, CI/CD pipelines, and programmatic access.

## Configuration

Set the following environment variables in `.env`:

| Variable | Description | Default |
|---|---|---|
| `EXTERNAL_API_HEADER` | HTTP header name used for authentication | `X-Agent-Secret` |
| `EXTERNAL_API_SECRET` | Secret value for the header (leave empty to disable) | empty (disabled) |

Example:

```env
EXTERNAL_API_HEADER=X-Agent-Secret
EXTERNAL_API_SECRET=8225140e022a98e8f3b0adb9800d0b6944eb8ae5f4ff53ab21cee823b769cec1
```

> **Note:** When `EXTERNAL_API_SECRET` is empty, all External API requests will return 403.

## Endpoints

### Create User

**POST** `/api/external/users`

Creates a new user and assigns the `member` role by default.

**Headers:**

```
X-Agent-Secret: <your-secret>
Content-Type: application/json
```

**Request Body:**

```json
{
  "username": "alice",
  "display_name": "Alice Wang",
  "password": "at-least-8-chars"
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `username` | string | Yes | Must be non-empty and unique |
| `display_name` | string | Yes | Display name for the user |
| `password` | string | Yes | Minimum 8 characters |

**Success Response (200):**

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**Error Responses:**

| Status | Description |
|---|---|
| 400 | Username is empty or password is less than 8 characters |
| 401 | Secret does not match |
| 403 | External API is not enabled (`EXTERNAL_API_SECRET` is empty) |
| 409 | Username already exists |

**curl Example:**

```bash
curl -X POST http://localhost:3000/api/external/users \
  -H "Content-Type: application/json" \
  -H "X-Agent-Secret: 8225140e022a98e8f3b0adb9800d0b6944eb8ae5f4ff53ab21cee823b769cec1" \
  -d '{
    "username": "alice",
    "display_name": "Alice Wang",
    "password": "securepassword123"
  }'
```
