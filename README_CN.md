# Codex Fleet

> ⚠️ **开发中，正在 vibe coding，可能存在 bug。**

一个用于管理多个 AI 编程 Agent（Codex 等）的 Web 控制台，Agent 可以运行在你的远程服务器上，也可以直接跑在本机。打开浏览器，创建 Agent，发送任务，看着它们干活。

[English →](./README.md)

---

## 计划中

1. 支持 skill/MCP 配置（多来源）。
2. 需求改成linear那种瀑布式可左右拖动的。
3. 支持更多 CLI，并做成模块化配置。
4. 增加AI自动跑测试用例的菜单。
5. 解析 Codex 的结构化 JSON 输出。
6. 其他体验优化。

> 备注：用 Rust 是因为 Codex 本身用 Rust，也刚好借这个项目学一学。开发速度取决于我的 token 刷新速度（lol）。

---

## 快速启动（Docker）

```bash
git clone git@github.com:jyokotori/codex-fleet.git
cd codex-fleet
cp .env.example .env   # 生产环境请修改账号密码等配置

docker compose up -d
```

访问 **http://localhost:3000**

默认管理员账号：**`codex` / `codex`**

---

## 本地开发

```bash
# 0. 如果还没有复制过环境变量文件，先执行
cp .env.example .env

# 1. 只启动 postgres
docker compose up postgres -d

# 2. 启动后端（一个终端）
cargo run -p backend

# 3. 启动前端开发服务器，支持热更新（另一个终端）
cd frontend && npm install && npm run dev
```

前端开发地址：**http://localhost:5173** （`/api` 和 `/ws` 会自动代理到后端）

---

## 当前功能

### 服务器管理
添加远程服务器，一键测试 SSH 连通性。支持免密 SSH、密码认证、SSH Key。添加好之后，该服务器上的所有 Agent 都自动走这套连接。

### Agent 管理
创建 Agent 时，选择一台远程服务器，选好 CLI 工具（目前仅支持 Codex），可以选择关联一个 Git 仓库。Docker 可选。
初始化时会先在服务器上固定创建两类目录：
- `~/.codex-fleet/{agent_id}/agent`：存放 Agent 配置
- `~/.codex-fleet/{agent_id}/workspace`：项目工作目录
如果启用 Docker，这两个目录会挂载到容器内的 `/agent` 和 `/workspace`，并应用 Docker 配置（端口、环境变量、挂载、初始化脚本）。

每个 Agent 可以单独配置：
- **Codex Config** — 绑定一组 `config.toml` + `auth.json`，让 Agent 启动就有认证信息和配置
- **AGENTS.md** — 把共享的项目说明文件注入 Agent 工作区
- **Docker 配置** — 自定义端口映射、环境变量、目录挂载、初始化脚本
- **运行时控制** — Docker Agent 在列表页只显示一个随容器状态变化的动作按钮（`停止`、`启动` 或 `重启`）；其中 `停止` 和 `重启` 需要二次确认，`启动` 直接执行。非 Docker Agent 不提供 Start/Stop/Restart 按钮。Agent 详情页头部只保留 `派发任务` 和 `复制命令`
- **状态同步** — 前端仍然只读取数据库中的 Agent 状态（`provisioning`、`running`、`stopped`、`error`），但后端会把 Docker 实际运行状态同步回这个字段；非 Docker Agent 则按 SSH 是否可达同步为 `running` / `stopped`
- **复制为新 Agent** — 列表页的复制操作会打开新建弹窗，并带入服务器、CLI、Git、Docker 和配置等参数，允许你调整后再创建；密码和 Token 不会被复制
- **删除确认** — 删除任意 Agent 前都需要显式确认；实际会删除 `~/.codex-fleet/{agent_id}` 和数据库记录，Docker Agent 还会额外删除容器

### 任务下发
手动派发前，Agent 必须处于空闲状态，且同步后的状态必须为 `running`。这条规则对 Docker 和非 Docker Agent 都一样；后端在真正执行前会再同步一次状态。调度器只会把 `waiting` 状态的工作项自动派发给 `running` 且空闲的 Agent。

### 需求管理
可以创建项目和工作项，把工作项分配给 Agent 或用户，关联 Agent 执行结果，并在需求详情页对 Agent 结果做审核处理。

### 实时日志 & 终端
- **日志标签** — 实时显示 Agent 会话的输出，自动滚动
- **终端标签** — 完整的交互式终端，可以直接在运行环境里敲命令（容器或宿主机）
- **复制命令** — 非 Docker Agent 复制直连宿主机的 SSH 命令；Docker Agent 复制 SSH 后进入容器 shell 的命令

### 配置管理
把可复用的配置统一存储，随时挂到任意 Agent 上：
- **Codex 配置** — 把 `config.toml` 和 `auth.json` 组合成一个命名配置组
- **AGENTS.md** — 可复用的 Agent 指令文件
- **Docker 配置** — 可复用的 Docker 运行配置（端口、挂载、环境变量、初始化脚本）

### 通知
配置 Webhook，任务完成或失败时自动推送通知。

### 用户与权限管理
- JWT 访问令牌 + Refresh Token
- 基于角色的权限控制（RBAC）+ 细粒度权限码
- 管理员专属用户管理：新增用户、重置密码、启用/禁用、解锁
- 普通用户自助能力：修改自己的密码

---

## 架构演进顺序

后端按以下顺序演进：

1. IAM
2. 配置中心
3. 服务器 + Agent 运行时
4. 通知中心

---

## 更新 SQLx 离线缓存

修改了 SQL 查询后，需要重新生成 `.sqlx/` 缓存，否则 Docker 构建时会报错：

```bash
cargo install sqlx-cli --no-default-features --features native-tls,postgres
./scripts/prepare-sqlx.sh
```

---

## 开源许可

Apache 2.0
