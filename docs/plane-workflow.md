# Plane 集成 —— 当前工作逻辑

本文梳理 `codex-fleet` 当前 Plane 集成的实际运行逻辑,作为后续迭代的基线。面向开发者视角,重点描述数据流、状态机与关键代码位置。

> **配置模型**:Plane 的 base_url / workspace_slug / api_key / webhook_secret 全部存储在数据库的 `plane_workspaces` 表,由页面管理。**不再使用环境变量**。一个 codex-fleet 实例可同时对接多个 Plane 工作区。每个 binding 自己声明三个项目状态(accept / in-progress / completion)和一组 label → CLI 映射,**不再硬编码状态名**(Todo / In Progress / Human Review / Review Failed)。

---

## 1. 总览

```
Plane Issue (state moved into binding.accept_state)
        │ ① webhook → /api/webhooks/plane/{workspace_id}
        │    - 按 workspace_id 查 plane_workspaces 拿 webhook_secret
        │    - HMAC 校验
        │    - state_id 必须 == binding.accept_state_id
        │    - issue 的 label_ids 与 binding 配的 label 集合必须有交集
        │    - 必须有 assignee(user_id → email)
        ▼
INSERT plane_tasks (workspace_id, status='pending')
   ON CONFLICT (workspace_id, plane_issue_id) WHERE status IN ('pending','dispatched') DO NOTHING
        │  rows_affected==1 → 新入队;==0 → 已在队列,跳过
        ▼ ② scheduler 每 10s 跑一条 LATERAL SQL
        │   - JOIN plane_workspaces (enabled), plane_bindings (enabled)
        │   - LATERAL 子查询同时拿 matching_count 和 idle_agent_id
        │   ┌──────────────────────────────────────────────────────────┐
        │   │ matching_count == 0 → 评论 + 翻 completion → 'rejected' │
        │   │ idle_agent_id IS NULL → 等下一 tick(busy)                │
        │   │ 拿到 idle_agent_id → 进入派发分支                         │
        │   └──────────────────────────────────────────────────────────┘
        │ ③ 派发分支(per-agent agent_lock 保护):
        │   - PlaneClient.get_issue_full() 复核
        │   - state_id / labels / assignee 任一漂移 → 'cancelled'
        │   - 按 binding_label.priority 升序,首个命中 agent.cli_inits 的 cli_type 即用
        │   - 都不命中 → 评论"无可用 CLI" → completion → 'rejected'
        │   - dispatch_task_for_agent(picked_cli, ...) 创建 tasks 行
        ▼
plane_tasks.status = 'dispatched'
Plane Issue   ── PATCH state ──> in_progress_state_id
              ── POST comment ──> "Dispatched to {agent_name} via {cli_type}"
        │
        ▼ ④ agent 执行(tasks 表)
        │
        ├─ 完成(agent_completed):
        │   plane_tasks.status = 'completed'
        │   Plane → completion_state_id + comment("Completed" + 结果)
        │
        └─ 失败 / 手动 Stop(agent_failed):
            plane_tasks.status = 'failed'
            Plane → completion_state_id + comment("Failed" + 原因)
```

完成 / 失败两条分支统一翻 binding 自己配的 `completion_state_id`,不再做"当前是 In Progress 才翻"的兜底判断。

---

## 2. 数据模型

迁移文件:`crates/backend/migrations/001_init.sql`(Plane Integration 段)

### 2.1 工作区 / 用户 / 分组

- `users.email` —— 与 Plane 成员的 email 匹配
- `agent_groups(id, name, created_at)` —— agent 分组
- `agent_group_members(group_id, agent_id)` —— 组成员关系
- **`plane_workspaces(id, name, base_url, workspace_slug, api_key, webhook_secret, enabled, timestamps)`**
  - 唯一约束:`(base_url, workspace_slug)`
  - `enabled=false` 时 scheduler 不会拾取其 plane_task;webhook 直接 200 + ignore
  - api_key / webhook_secret 存明文,API 仅返回掩码 `xxxx••••xxxx`

### 2.2 Binding(扩展)

```sql
plane_bindings (
    id, workspace_id, plane_project_id, plane_project_name, plane_project_identifier,
    agent_group_id,
    accept_state_id      TEXT NOT NULL,  accept_state_name      TEXT NOT NULL,
    in_progress_state_id TEXT NOT NULL,  in_progress_state_name TEXT NOT NULL,
    completion_state_id  TEXT NOT NULL,  completion_state_name  TEXT NOT NULL,
    enabled, created_at,
    UNIQUE (workspace_id, plane_project_id),
    FOREIGN KEY (workspace_id) REFERENCES plane_workspaces ON DELETE CASCADE
)

plane_binding_labels (
    id, binding_id, label_id, label_name, cli_type, priority,
    UNIQUE (binding_id, label_id),
    FOREIGN KEY (binding_id) REFERENCES plane_bindings ON DELETE CASCADE
)
```

- 三个 `*_state_id` 必须存在于该 Plane project 的 states 列表(创建/更新时调 Plane API 校验)。
- `*_state_name` 仅作展示(rename-safe);判定一律用 `state_id`。
- 至少一个 label,且至少一个 label 的 `cli_type` 是非 WIP(目前实际只有 `codex` 是非 WIP)。
- 同一 binding 内 `label_id` 和 `priority` 都不能重复。`priority` 升序决定一条 issue 同时打多 label 时按哪条选 CLI。

### 2.3 Plane 任务队列

```sql
plane_tasks (
    id, workspace_id, plane_issue_id, plane_project_id,
    title, description, priority, assignee_email,
    status,           -- 'pending' | 'dispatched' | 'completed' | 'failed' | 'rejected' | 'cancelled'
    agent_id, task_id,
    created_at, updated_at
)

CREATE UNIQUE INDEX plane_tasks_active_uq
ON plane_tasks (workspace_id, plane_issue_id)
WHERE status IN ('pending', 'dispatched');
```

- 状态语义:
  - `pending` —— webhook 入队,等待调度
  - `dispatched` —— 已派给某个 agent 执行
  - `completed` —— agent 跑完成功
  - `failed` —— agent 跑失败 / 手动 Stop
  - `rejected` —— 调度时被拒绝(无可用 agent / 无可用 CLI / binding 失效)
  - `cancelled` —— Plane 侧手动改回非 accept 状态、或 label/assignee 漂移
- 偏函数 UNIQUE 索引保证同一 issue 同时只能有一个活跃(`pending` 或 `dispatched`)记录;历史的 `completed`/`failed`/`rejected`/`cancelled` 行不影响后续重新入队。
- webhook 用 `INSERT ... ON CONFLICT (workspace_id, plane_issue_id) WHERE status IN ('pending','dispatched') DO NOTHING`,根据 `rows_affected` 区分"新入队 / 已在队"。

### 2.4 Agent 多 CLI 配置

```sql
agents (
    id, name, server_id, user_id,
    git_*, docker_*, workdir, status, ...
    -- 不再有 cli_type / codex_config_id / agents_md_id 列
)

agent_cli_inits (
    id, agent_id, cli_type, codex_config_id, agents_md_id,
    UNIQUE (agent_id, cli_type),
    FOREIGN KEY (agent_id)        REFERENCES agents         ON DELETE CASCADE,
    FOREIGN KEY (codex_config_id) REFERENCES codex_configs,
    FOREIGN KEY (agents_md_id)    REFERENCES company_configs
)
```

- 一个 agent 可同时安装多个 CLI 的配置;`cli_type='codex'` 行用 `codex_config_id` + `agents_md_id`,其它 CLI 当前留空(WIP 占位)。
- provision Step 1 会按 `agent_cli_inits` 行逐个写盘;Step 2 docker run 不再读 `volume_mappings`,固定 `-v {agent_workspace_volume}:/workspace`(命名卷)+ `-v {base}/agent:/agent`(配置目录通过 SSH 写入到 host scratch + bind);Step 3 仅在 `cli_type='codex'` 行存在时 `ln -sfn /agent /root/.codex`。

### 2.5 Docker 配置(去 volume_mappings)

`docker_configs` 不再有 `volume_mappings` 列。Agent 容器始终挂载受管的 `/workspace` 命名卷,宿主机目录映射已彻底放弃。

---

## 3. Webhook 入口

文件:`crates/runtime_agent/src/api/webhooks.rs`
路由:`POST /api/webhooks/plane/{workspace_id}`(公开,无鉴权)

流程:

1. 按 `workspace_id` 查 `plane_workspaces`:
   - 不存在 → 404
   - `enabled=false` → 200 + ignore
   - 取出 `webhook_secret` 用于签名校验
2. `verify_signature`:`x-plane-signature` 头做 HMAC-SHA256(body, secret) 十六进制比对(secret 为空时跳过校验,开发模式)。**签名错返 401**,其余异常路径全部返 200 避免 Plane 死循环重试。
3. 解析 `payload["data"]`:`id`(issue_id)、`project`(project_id)、`state` 或 `state_id`、`labels[]`、`assignees[]`(user_id 数组)、`name`、`description_stripped`、`priority`。
4. 找 `plane_bindings WHERE workspace_id=$1 AND plane_project_id=$2 AND enabled=true`,无则 200 + ignore。
5. **State 过滤**:`state_id != binding.accept_state_id` → 200 + ignore。
6. **Label 过滤**:issue 的 `label_ids` 与 `plane_binding_labels.label_id` 取交集,空则 200 + ignore。
7. **Assignee 过滤**:`assignees` 必须非空。第一个 user_id 通过 `PlaneClient.member_email()` 解析为 email;解析失败 → 200 + ignore。
8. `INSERT INTO plane_tasks ... ON CONFLICT (workspace_id, plane_issue_id) WHERE status IN ('pending','dispatched') DO NOTHING`,记录 `rows_affected`。日志区分"新入队"和"已在队列跳过"。
9. 返回 `200 OK`。

> **assignees 字段实测注意**:当前实现假设 Plane webhook payload 的 `assignees` 是 user_id 数组。若你的 Plane 实例发的是 email 数组,需要改 webhook 解析路径(目前是 `as_str` + `member_email()` 调用)。**联调前用 curl 抓一发真实 webhook 验证**。

---

## 4. 调度器(分发)

文件:`crates/runtime_agent/src/scheduler.rs`
Tick 周期:10 秒;`plane_tick` 与 codex-fleet 自有 `tick` 并行。

### 4.1 单条批量 SQL

```sql
SELECT pt.id AS plane_task_id,
       pt.workspace_id, pt.plane_issue_id, pt.plane_project_id, pt.assignee_email,
       pw.base_url, pw.workspace_slug, pw.api_key,
       pb.id AS binding_id, pb.agent_group_id,
       pb.accept_state_id, pb.in_progress_state_id, pb.completion_state_id,
       m.matching_count,
       i.idle_agent_id
  FROM plane_tasks pt
  JOIN plane_workspaces pw  ON pw.id = pt.workspace_id AND pw.enabled = TRUE
  JOIN plane_bindings    pb ON pb.workspace_id = pt.workspace_id
                           AND pb.plane_project_id = pt.plane_project_id
                           AND pb.enabled = TRUE
  LEFT JOIN LATERAL (
    SELECT COUNT(*)::bigint AS matching_count
    FROM agents a
    JOIN agent_group_members agm ON agm.agent_id = a.id AND agm.group_id = pb.agent_group_id
    JOIN users u                 ON u.id = a.user_id   AND u.email     = pt.assignee_email
  ) m ON TRUE
  LEFT JOIN LATERAL (
    SELECT a.id AS idle_agent_id
    FROM agents a
    JOIN agent_group_members agm ON agm.agent_id = a.id AND agm.group_id = pb.agent_group_id
    JOIN users u                 ON u.id = a.user_id   AND u.email     = pt.assignee_email
    WHERE a.status = 'running'
      AND NOT EXISTS (SELECT 1 FROM tasks t WHERE t.agent_id = a.id AND t.status='agent_in_progress')
    ORDER BY a.id LIMIT 1
  ) i ON TRUE
 WHERE pt.status = 'pending'
 ORDER BY pt.created_at ASC;
```

一次拉出每条 pending 任务的"匹配 agent 总数 + 选中的空闲 agent",分别走三种分支。

### 4.2 三种分支

```
matching_count == 0:
    PlaneClient.add_comment("无可用 agent: 用户 {email} 不在分组 {group} 中")
    PlaneClient.update_issue_state_by_id(completion_state_id)
    plane_tasks.status = 'rejected'

idle_agent_id IS NULL && matching_count > 0:
    继续等下一 tick (busy);不写库,不评论

idle_agent_id 拿到值:
    进入"派发分支"
```

### 4.3 派发分支(per-agent agent_lock 保护)

1. **取锁**:`state.agent_lock(agent_id).await.lock().await`(`AppContext.agent_dispatch_locks` 上的 `Mutex`)。
2. **Plane 复核**:`PlaneClient.get_issue_full()`。
   - 网络错(transient)→ 不动 plane_task,下一 tick 重试。
   - 复核 `state_id == accept_state_id`;否则 plane_tasks.status='cancelled'(不评论 —— 用户已经手动改了)。
   - 复核 `label_ids ∩ binding.label_ids` 非空;否则 'cancelled'。
   - 复核 `assignee_emails` 含 `pt.assignee_email`;否则 'cancelled'。
3. **CLI 选择**:把 binding_label 按 `priority` 升序排,逐个看其 `cli_type` 是否在该 agent 的 `agent_cli_inits` 中;首命中即用。无命中 → `add_comment("None of the issue's bound labels map to a CLI installed on the assigned agent.")` → completion → 'rejected'。
4. **再次确认 agent 在线**:`sync_agent_status_with_creds` → `status=='running'`,否则跳过等下一 tick。
5. **下发**:`dispatch_task_for_agent(state, agent_id, title, description, ...)` 创建 `tasks` 行。**注意:dispatch_task_for_agent 本身不再加锁**,要求调用方持有 `agent_lock`(scheduler 已持有;HTTP 派发路径在 `create_task` handler 内部加锁)。
6. **成功后**:
   - `UPDATE plane_tasks SET status='dispatched', agent_id, task_id`
   - `update_issue_state_by_id(in_progress_state_id)`(失败仅 log,见已知问题)
   - `add_comment("Dispatched to {agent_name} via {cli_type}")`

### 4.4 孤儿清扫

每 N 个 tick(实现见 scheduler.rs)扫一次:`pending`/`dispatched` 但 binding 已删 / workspace.enabled=false / workspace 删 → status='rejected' + 评论 + 翻 completion(若 binding 还在)。避免遗留任务永远卡在 pending。

---

## 5. 回写(agent 执行结果 → Plane)

文件:`crates/runtime_agent/src/api/tasks.rs`
触发点:task 状态变为 `agent_completed` 或 `agent_failed` 时(后者也包括手动 Stop)。

查询时 `JOIN plane_workspaces` + `plane_bindings` 取凭证和 `completion_state_id`:

```sql
SELECT pt.id, pt.plane_issue_id, pt.plane_project_id,
       pw.base_url, pw.workspace_slug, pw.api_key,
       pb.completion_state_id
  FROM plane_tasks pt
  JOIN plane_workspaces pw ON pw.id = pt.workspace_id
  JOIN plane_bindings pb   ON pb.workspace_id = pt.workspace_id
                          AND pb.plane_project_id = pt.plane_project_id
 WHERE pt.task_id = $1 AND pt.status='dispatched'
```

行为(成功 / 失败统一):

- `plane_tasks.status` = `'completed'` 或 `'failed'`
- Plane → `completion_state_id`(无条件,通过 `update_issue_state_by_id`)
- comment 写回:`<h3>Completed</h3><pre>{escape(result_md)}</pre>` 或 `<h3>Failed</h3><pre>{escape(error_msg)}</pre>`

不再做"当前是 In Progress 才翻"的条件分支。所有"Review Failed"字面量已删除。

---

## 6. PlaneClient(HTTP 适配层)

文件:`crates/runtime_agent/src/infrastructure/plane_client.rs`

- `reqwest::Client::builder().connect_timeout(5s).timeout(15s)`,补上目前裸 `Client::new()` 的超时缺失。
- 鉴权:每次请求附 `x-api-key`。
- 缓存(进程内 `RwLock<HashMap>`,永不主动过期):
  - `states_cache: project_id → (state_name → state_id)`
  - `labels_cache: project_id → (label_id → label_name)`
  - `members_cache: workspace 级 user_id → email`
  - 缓存只在同一个 PlaneClient 实例上有效;按计划应在 `AppContext.plane_clients: Mutex<HashMap<workspace_id, Arc<PlaneClient>>>` 上提单例,使缓存跨 tick 复用。**当前实现仍是每次构造,缓存只在单次调用链内复用**(已知改进项)。
- 暴露方法:
  - `get_states(project_id)` / `get_labels(project_id)` —— 拉 + 缓存
  - `get_issue_full(project_id, issue_id) -> IssueSnapshot { title, description, state_id, label_ids[], assignee_user_ids[] }` —— 单次 GET 拿齐所需字段
  - `update_issue_state_by_id(project_id, issue_id, state_id)` —— 用 id 直接 PATCH(避免 name 漂移)
  - `member_email(user_id) -> String` —— 解析 assignee user_id 到 email
  - `add_comment(project_id, issue_id, comment_html)`

旧的 `get_issue` / `get_issue_state_name` / `update_issue_state(name)` 已全部移除。

---

## 7. HTTP API(前端用)

路由注册:`crates/runtime_agent/src/lib.rs`
前端页面:`frontend/src/pages/PlaneIntegration.tsx` / `frontend/src/pages/Agents.tsx`

### 7.1 工作区

| Method | 路径 | 说明 |
|---|---|---|
| GET | `/api/plane/workspaces` | 列出工作区;api_key / webhook_secret 仅返回掩码 |
| POST | `/api/plane/workspaces` | 新建 |
| PUT | `/api/plane/workspaces/{id}` | 更新;api_key 留空表示保留原值 |
| DELETE | `/api/plane/workspaces/{id}` | 删除(级联删 binding / plane_task) |
| POST | `/api/plane/workspaces/{id}/toggle` | 翻转 enabled |

### 7.2 项目元数据(代理 Plane API)

| Method | 路径 | 说明 |
|---|---|---|
| GET | `/api/plane/workspaces/{id}/projects` | 项目下拉 |
| GET | `/api/plane/workspaces/{id}/projects/{pid}/states` | 该项目的 states `[{id, name, group}]` |
| GET | `/api/plane/workspaces/{id}/projects/{pid}/labels` | 该项目的 labels `[{id, name, color}]` |

### 7.3 Bindings

| Method | 路径 | 说明 |
|---|---|---|
| GET | `/api/plane/workspaces/{id}/bindings` | 该工作区下的 bindings(包含 labels[]) |
| POST | `/api/plane/workspaces/{id}/bindings` | 新建 binding。Body 含 `accept/in_progress/completion_state_id+name`、`labels[{label_id, label_name, cli_type, priority}]`。后端会调 Plane API 校验 state_id / label_id 真实存在 |
| PUT | `/api/plane/bindings/{id}` | 更新 binding。labels 字段提供时整组替换;不提供时保持现状 |
| DELETE | `/api/plane/bindings/{id}` | 删除(级联删 plane_binding_labels) |
| POST | `/api/plane/bindings/{id}/toggle` | 翻转 enabled |

### 7.4 其它

| Method | 路径 | 说明 |
|---|---|---|
| GET | `/api/clis` | 硬编码 CLI 注册表 `[{value, label, wip}]`,前端下拉源 |
| GET | `/api/plane/tasks` | 最近 200 条 plane_tasks(全工作区) |
| POST | `/api/webhooks/plane/{workspace_id}` | Plane → codex-fleet 入口(公开) |

---

## 8. 配置(运行时 · 非环境变量)

所有 Plane 凭证都通过 UI 写入 `plane_workspaces`:

- `name` —— 页面展示名
- `base_url` —— Plane 根地址(无 trailing slash,例如 `http://192.168.14.63`)
- `workspace_slug` —— Plane 工作区 slug(例如 `magician`)
- `api_key` —— Plane API token
- `webhook_secret` —— 与 Plane 侧配置相同的 HMAC secret

UI 上对单条工作区显示的 webhook URL 由前端拼接 `{window.location.origin}/api/webhooks/plane/{workspace_id}`,用户复制后粘贴到 Plane。

> 备注:`.env.example` / `.env` 中原有的 `PLANE_*` 环境变量已全部删除;`shared_kernel::AppConfig` 不再包含这些字段。

---

## 9. 已知设计问题 / 改进项

1. **PlaneClient 单例化**:计划要求把 `PlaneClient` 缓存到 `AppContext.plane_clients` 上,跨 tick 复用 states/labels/members 缓存。当前实现仍每次构造,缓存只在单次调用链内有效。
2. **Plane writeback 失败的 UX 妥协**:派发时若 `update_issue_state_by_id(in_progress)` 失败,Plane 会从 accept 直接跳到 completion;暂时只 log。future work 可加 `state_transition_pending` 列做异步重试。
3. **公平性**:`ORDER BY a.id LIMIT 1` 的 idle agent 选择是确定性的但不公平。同一用户多 agent 时容易把负载打到 id 最小的那台。后续可加 `agents.last_assigned_at` 做 LRU。
4. **派发后用户手动改回 backlog**:任务跑完仍按 `completion_state_id` 写回,会覆盖用户操作。这是 trade-off,文档化在此。
5. **api_key / webhook_secret 明文入库**:当前未加密;生产环境应纳入 `master_key` 加密方案,与 `codex_configs` 等敏感凭证保持一致。
6. **assignees 字段格式实测未确认**:当前假设 user_id 数组,通过 `member_email` 解析。若 Plane 实例直接发 email,需改 webhook 解析路径。

---

## 10. 关键代码路径索引

- 入队:`crates/runtime_agent/src/api/webhooks.rs` —— `plane_webhook`
- 调度:`crates/runtime_agent/src/scheduler.rs` —— `plane_tick` / 派发分支
- HTTP 适配:`crates/runtime_agent/src/infrastructure/plane_client.rs`
- 工作区 / binding / states / labels / clis 路由:`crates/runtime_agent/src/api/plane.rs`
- agent 完成/失败回写:`crates/runtime_agent/src/api/tasks.rs` —— Plane write-back 段
- agent_dispatch_locks 定义:`crates/shared_kernel/src/context.rs`
- CLI 注册表:`crates/shared_kernel/src/clis.rs` —— `SUPPORTED_CLIS`
- 路由注册:`crates/runtime_agent/src/lib.rs`
- 迁移:`crates/backend/migrations/001_init.sql` —— Plane Integration 段
- 前端页面:
  - `frontend/src/pages/PlaneIntegration.tsx` —— Workspaces / Bindings(states + labels 编辑器)
  - `frontend/src/pages/Agents.tsx` —— 多 CLI init 编辑器
- 前端 API 封装:`frontend/src/lib/api.ts` —— `planeApi` / `clisApi`
