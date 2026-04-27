# Plane 集成 —— 当前工作逻辑

本文用于梳理 `codex-fleet` 当前 Plane 集成的实际运行逻辑，作为后续迭代改造的基线。面向开发者视角，重点描述数据流、状态机与关键代码位置。用户侧的安装/配置说明见 `docs/plane-integration.md`。

> **配置模型**：Plane 的 base_url / workspace_slug / api_key / webhook_secret 全部存储在数据库的 `plane_workspaces` 表，由页面管理。**不再使用环境变量**。一个 codex-fleet 实例可同时对接多个 Plane 工作区。

---

## 1. 总览

```
Plane Issue (Todo)
        │ ① webhook 到 /api/webhooks/plane/{workspace_id}
        │    - 按 workspace_id 查 plane_workspaces 拿 webhook_secret
        │    - HMAC 校验 & state_id→Todo 过滤
        ▼
INSERT plane_tasks (workspace_id, status=pending)
        │
        ▼ ② scheduler 每 10s 轮询 (JOIN plane_workspaces WHERE enabled=true)
        │   - 按 (workspace_id, plane_project_id) 查 plane_bindings 找 agent_group
        │   - 按 assignee_email 在 group 内选空闲 agent（为空则任意空闲 agent）
        │   - 用该 workspace 的 base_url/slug/api_key 构造 PlaneClient
        │   - 调 Plane API 校验 issue 仍为 Todo；从 Plane 拉最新 title/description
        │   - dispatch_task_for_agent() 下发
        ▼
plane_tasks.status = dispatched
Plane Issue   ── PATCH state ──> In Progress
              ── POST comment ──> "Task dispatched to agent {name}"
        │
        ▼ ③ agent 执行（tasks 表）
        │
        ├─ 完成：tasks.rs 在 status='agent_completed' 时
        │         JOIN plane_workspaces 取凭证，回写
        │         Plane → Human Review + comment
        │         plane_tasks.status = completed
        │
        └─ 失败：tasks.rs 在 status='agent_failed' 时
                  Plane 当前为 In Progress 才 → Review Failed
                  否则仅加 comment，不改状态
                  plane_tasks.status = failed

（从 Human Review 推进到 Done / Review Failed 目前在 codex-fleet 端没有 UI 闭环，需在 Plane 中手动完成。）
```

Plane 侧每个绑定的项目必须存在以下状态名（大小写敏感，严格匹配）：
`Backlog` / `Todo` / `In Progress` / `Human Review` / `Review Failed` / `Done` / `Cancelled`。

---

## 2. 数据模型

迁移文件：`crates/backend/migrations/001_init.sql`（Plane Integration 段）

- `users.email` —— 与 Plane assignee 邮箱匹配
- `agent_groups(id, name, created_at)` —— agent 分组
- `agent_group_members(group_id, agent_id)` —— 组成员关系
- **`plane_workspaces(id, name, base_url, workspace_slug, api_key, webhook_secret, enabled, timestamps)`**
  - 唯一约束：`(base_url, workspace_slug)`
  - `enabled=false` 时：scheduler 不会拾取其 pending plane_task；webhook 返回 200 但忽略
  - API key / webhook secret 存明文；HTTP 返回只给掩码 `xxxx••••xxxx`
- `plane_bindings(id, workspace_id, plane_project_id, ..., agent_group_id, enabled, created_at)`
  - 唯一约束：`(workspace_id, plane_project_id)`
  - `ON DELETE CASCADE workspace_id` —— 删除工作区级联删 binding
- `plane_tasks(id, workspace_id, plane_issue_id, plane_project_id, title, description, priority, assignee_email, status, agent_id, task_id, timestamps)`
  - `status`: `pending` → `dispatched` → `completed` / `failed`；另有 `cancelled`
  - `agent_id`、`task_id` 在 dispatch 成功后回填
  - 索引：`(status, created_at)` 加速 scheduler，`(workspace_id, plane_project_id)` 加速绑定查询
  - **无 issue 级去重**：同一 issue 重新回到 Todo 会再插入一行

---

## 3. Webhook 入口

文件：`crates/runtime_agent/src/api/webhooks.rs`
路由：`POST /api/webhooks/plane/{workspace_id}`（公开，无鉴权）—— `crates/runtime_agent/src/lib.rs`

流程：

1. 按 `workspace_id` 查 `plane_workspaces`：
   - 不存在 → 404
   - `enabled=false` → 200（静默忽略）
   - 取出 `webhook_secret` 用于签名校验
2. `verify_signature`：`x-plane-signature` 头做 HMAC-SHA256(body, secret) 十六进制比对。
   - secret 为空时跳过校验（开发模式）
3. 仅处理：`event=="issue"` 且 `action=="updated"` 且 `activity.field=="state_id"` 且 `data.state.name=="Todo"`。
4. 抽取 `issue_id / project_id / name / description_stripped / priority / assignees[0].email`。
5. `INSERT plane_tasks (workspace_id=..., status='pending')`。
6. 返回 `200 OK`。

---

## 4. 调度器（分发）

文件：`crates/runtime_agent/src/scheduler.rs`
Tick 周期：10 秒；`plane_tick` 与 codex-fleet 自有 `tick` 并行。

`plane_tick` 流程：

1. 单次 JOIN 查询：`plane_tasks pt INNER JOIN plane_workspaces pw ON pw.id = pt.workspace_id AND pw.enabled=true` where `pt.status='pending'`，按 `created_at ASC`。
   - 一并取出该 workspace 的 `base_url/workspace_slug/api_key`，避免单独查询。
2. 每个 pending 任务派独立 `tokio::spawn` 执行 `plane_dispatch_one`，构造属于该 workspace 的 `PlaneClient`。

`plane_dispatch_one` 流程：

1. **找绑定**：按 `(workspace_id, plane_project_id)` + `enabled=true` 取首条；没有则 warn 跳过（plane_task 不清理，下一轮继续）。
2. **找空闲 agent**（二选一 SQL）：
   - `assignee_email` 为空：组内任意 `status='running'` 且无 `tasks.status='agent_in_progress'` 的 agent，`LIMIT 1`。
   - `assignee_email` 非空：同上 + `INNER JOIN users u ON u.id = a.user_id AND u.email = $email`。
   - 找不到：直接返回，下一 tick 重试。
3. **Plane 侧状态复核**：`PlaneClient::get_issue_state_name`，若当前已不是 `Todo` → 将 plane_task 置为 `cancelled` 返回。
4. **拉最新 title/description**：`PlaneClient::get_issue`（以 Plane 当前值为准）。
5. **校验 agent 在线**：`sync_agent_status_with_creds` 再次确认 `status='running'`。
6. **下发**：调用 `dispatch_task_for_agent` 创建 `tasks` 记录。
7. **成功后**：
   - `UPDATE plane_tasks SET status='dispatched', agent_id, task_id`
   - `PATCH Plane issue → "In Progress"`（无条件）
   - `POST comment: "Task dispatched to agent {agent_name}"`

并发保护：`plane_dispatch_one` 本身**没有** per-agent 互斥锁（与 `tick` 内部的 codex-fleet 路径不同）；多个 pending plane_task 命中同一 agent 时，依赖下游幂等性。

---

## 5. 回写（agent 执行结果 → Plane）

### 5.1 Agent 完成 / 失败

文件：`crates/runtime_agent/src/api/tasks.rs`
触发点：task 状态变为 `agent_completed` 或 `agent_failed` 时。

查询时 `JOIN plane_workspaces` 取凭证：

```sql
SELECT pt.id, pt.plane_issue_id, pt.plane_project_id,
       pw.base_url, pw.workspace_slug, pw.api_key
  FROM plane_tasks pt
  JOIN plane_workspaces pw ON pw.id = pt.workspace_id
 WHERE pt.task_id = $1 AND pt.status='dispatched'
```

行为：
- **完成**（`agent_completed`）：
  - `plane_tasks.status = 'completed'`
  - Plane → `Human Review`（**无条件**写）
  - comment 写回 agent 结果
- **失败**（`agent_failed`）：
  - `plane_tasks.status = 'failed'`
  - 先 `get_issue_state_name`，**仅当前为 `In Progress`** 才切 `Review Failed`；否则仅追加 comment。

### 5.2 人工审核

codex-fleet 当前没有人工审核回写逻辑——agent 完成后把 Plane 置为 `Human Review`，后续流转需要在 Plane 中人工完成。

---

## 6. PlaneClient（HTTP 适配层）

文件：`crates/runtime_agent/src/infrastructure/plane_client.rs`

- 每次构造都带独立的 `base_url / workspace_slug / api_key`；不同 workspace 各自一个 `PlaneClient` 实例。
- 鉴权：每次请求附 `x-api-key`。
- **State 缓存**：`states_cache: project_id → (name → state_id)`，进程内 `RwLock<HashMap>`，永不主动过期；`update_issue_state` 在名字找不到时失效一次重新拉取。
  - 注意：缓存以 `project_id` 为键，不分 workspace。Plane 的 project_id 是 UUID，全局不会冲突，这点安全，但**所有 workspace 共享同一个 PlaneClient 时**缓存才有效；目前每次 plane_tick 为每条任务都 new 一个 `PlaneClient`，所以缓存仅在单次 dispatch 内生效（fetch_states、get_issue_state_name 两次调用之间）。
- 暴露方法：
  - `get_states(project_id)` / `get_issue_state_name` / `get_issue`
  - `update_issue_state(project_id, issue_id, state_name) -> Ok(bool)` —— 名字不存在返回 `Ok(false)`
  - `add_comment(project_id, issue_id, comment_html)`

---

## 7. HTTP API（前端用）

路由注册：`crates/runtime_agent/src/lib.rs`
前端页面：`frontend/src/pages/PlaneIntegration.tsx`

工作区：

| Method | 路径 | 说明 |
|---|---|---|
| GET | `/api/plane/workspaces` | 列出工作区；api_key / webhook_secret 仅返回掩码 |
| POST | `/api/plane/workspaces` | 新建 |
| PUT | `/api/plane/workspaces/{id}` | 更新；字段留空不覆盖原值（api_key 空值=保留） |
| DELETE | `/api/plane/workspaces/{id}` | 删除（级联删 binding / plane_task） |
| POST | `/api/plane/workspaces/{id}/toggle` | 翻转 enabled |
| GET | `/api/plane/workspaces/{id}/projects` | 代理调用 Plane API 列 project，用于绑定下拉 |
| GET | `/api/plane/workspaces/{id}/bindings` | 该工作区下的 bindings |
| POST | `/api/plane/workspaces/{id}/bindings` | 新建 binding |

binding 单独操作（id 即可定位）：

| Method | 路径 | 说明 |
|---|---|---|
| PUT | `/api/plane/bindings/{id}` | 目前仅支持改 `agent_group_id` |
| DELETE | `/api/plane/bindings/{id}` | 删除 |
| POST | `/api/plane/bindings/{id}/toggle` | 翻转 enabled |

其他：

| Method | 路径 | 说明 |
|---|---|---|
| GET | `/api/plane/tasks` | 最近 200 条 plane_tasks（全工作区） |
| POST | `/api/webhooks/plane/{workspace_id}` | Plane → codex-fleet 入口（公开） |

---

## 8. 配置（运行时 · 非环境变量）

所有 Plane 凭证都通过 UI 写入 `plane_workspaces`：

- `name` —— 页面展示名
- `base_url` —— Plane 根地址（无 trailing slash，例如 `http://192.168.14.63`）
- `workspace_slug` —— Plane 工作区 slug（例如 `magician`）
- `api_key` —— Plane API token
- `webhook_secret` —— 与 Plane 侧配置相同的 HMAC secret

UI 上对单条工作区显示的 webhook URL 由前端拼接 `{window.location.origin}/api/webhooks/plane/{workspace_id}`，用户复制后粘贴到 Plane。

> 备注：`.env.example` 与 `.env` 中原有的 `PLANE_BASE_URL / PLANE_WORKSPACE_SLUG / PLANE_API_KEY / PLANE_WEBHOOK_SECRET` 已全部删除；`shared_kernel::AppConfig` 不再包含这些字段。

---

## 9. 当前设计上的注意点 / 已知不对称

1. **plane_dispatch_one 没有 agent 锁**：并发下多个 pending plane_task 可能同时命中同一 agent，依赖下游幂等。
3. **`agent_completed` 无条件**置 Human Review，而 `agent_failed` 要求当前 In Progress 才改状态——两个分支逻辑不对称。
4. **state 名称硬编码**（`Todo / In Progress / Human Review / Review Failed`）散落在 scheduler、tasks、webhooks；改名或多语言 Plane 项目不可用。
5. **plane_tasks 无 issue 级去重**：同一 issue 多次 Todo 会产生多条 pending。
6. **binding 取第一条 enabled**：同 workspace 内一个 Plane project 绑定多个 agent group 的场景未定义（取 LIMIT 1，其余忽略）。唯一约束 `(workspace_id, plane_project_id)` 会阻止插第二条，但跨工作区可能撞 project_id（实际 Plane project_id 是 UUID，不会撞）。
7. **states_cache 实际形同没有**：当前每次 dispatch 都 new 一个 `PlaneClient`，缓存只在单次调用链内复用。
8. **webhook 只认 Todo**：Backlog → 其他状态不会触发；手动拖回 Todo 会再次派发。
9. **assignee 多人仅取首个**，无策略可配。
10. **api_key / webhook_secret 明文入库**：当前未加密；生产环境应纳入 `master_key` 加密方案，与其他敏感凭证（例如 codex_configs）保持一致。

---

## 10. 关键代码路径索引

- 入队：`crates/runtime_agent/src/api/webhooks.rs` —— `plane_webhook`
- 调度：`crates/runtime_agent/src/scheduler.rs` —— `plane_tick` / `plane_dispatch_one`
- HTTP 适配：`crates/runtime_agent/src/infrastructure/plane_client.rs`
- 工作区 / binding CRUD：`crates/runtime_agent/src/api/plane.rs`
- agent 完成/失败回写：`crates/runtime_agent/src/api/tasks.rs` —— Plane write-back 段
- 路由注册：`crates/runtime_agent/src/lib.rs`
- 迁移：`crates/backend/migrations/001_init.sql` —— Plane Integration 段
- 前端页面：`frontend/src/pages/PlaneIntegration.tsx`
- 前端 API 封装：`frontend/src/lib/api.ts` —— `planeApi`
