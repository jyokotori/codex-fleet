# External API

External API 提供无需登录的 HTTP 接口，通过请求头 + Secret 进行身份验证，适用于自动化脚本、CI/CD 等场景。

## 配置

在 `.env` 中设置以下两个变量：

| 变量 | 说明 | 默认值 |
|---|---|---|
| `EXTERNAL_API_HEADER` | 用于认证的 HTTP 请求头名称 | `X-Agent-Secret` |
| `EXTERNAL_API_SECRET` | 请求头对应的密钥（留空则禁用 External API） | 空（禁用） |

示例：

```env
EXTERNAL_API_HEADER=X-Agent-Secret
EXTERNAL_API_SECRET=8225140e022a98e8f3b0adb9800d0b6944eb8ae5f4ff53ab21cee823b769cec1
```

> **注意：** `EXTERNAL_API_SECRET` 为空时，所有 External API 请求将返回 403。

## 接口列表

### 创建用户

**POST** `/api/external/users`

创建一个新用户，自动分配 `member` 角色。

**请求头：**

```
X-Agent-Secret: <your-secret>
Content-Type: application/json
```

**请求体：**

```json
{
  "username": "alice",
  "display_name": "Alice Wang",
  "password": "at-least-8-chars"
}
```

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `username` | string | 是 | 用户名，不能为空，不能重复 |
| `display_name` | string | 是 | 显示名称 |
| `password` | string | 是 | 密码，至少 8 个字符 |

**成功响应（200）：**

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**错误响应：**

| 状态码 | 说明 |
|---|---|
| 400 | 用户名为空或密码少于 8 位 |
| 401 | Secret 不匹配 |
| 403 | External API 未启用（`EXTERNAL_API_SECRET` 为空） |
| 409 | 用户名已存在 |

**curl 示例：**

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
