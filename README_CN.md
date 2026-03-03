# Codex Fleet

> ⚠️ **开发中，正在 vibe coding，可能存在 bug。**

一个用于管理多个 AI 编程 Agent（Codex 等）的 Web 控制台，Agent 可以运行在你的远程服务器上，也可以直接跑在本机。打开浏览器，创建 Agent，发送任务，看着它们干活。

[English →](./README.md)

---

## 快速启动

```bash
git clone git@github.com:jyokotori/codex-fleet.git
cd codex-fleet
cp .env.example .env   # 生产环境请修改 CODEX_MASTER_KEY

docker compose up -d
```

访问 **http://localhost:3000**

默认管理员账号：**`codex` / `codex`**

> `.env` 已加入 `.gitignore`。默认配置（`COMPOSE_PROFILES=bundled-db`）会自动启动内置 PostgreSQL 容器。
> 如需使用外部数据库，将 `.env` 中的 `COMPOSE_PROFILES` 留空，并修改 `DATABASE_URL`。

---

## 能做什么

### 服务器管理
添加远程服务器，一键测试 SSH 连通性。支持免密 SSH、密码认证、SSH Key。添加好之后，该服务器上的所有 Agent 都自动走这套连接。

### Agent 管理
创建 Agent 时，选择一台服务器（或者选「本机」，不需要 SSH），选好 CLI 工具（目前支持 Codex，更多 WIP），可以选择关联一个 Git 仓库。创建后控制台自动完成所有初始化：拉取代码、启动 Docker 容器、在容器内建好 tmux 会话。

每个 Agent 可以单独配置：
- **Codex Config** — 绑定一组 `config.toml` + `auth.json`，让 Agent 启动就有认证信息和配置
- **AGENTS.md** — 把共享的项目说明文件注入 Agent 工作区
- **Docker 配置** — 自定义端口映射、环境变量、目录挂载、初始化脚本

### 任务下发
打开一个 Agent 详情页，输入任务，点发送，任务直接进入 Agent 的 tmux 会话。同一页面可以看到所有历史任务和状态。

### 实时日志 & 终端
- **日志标签** — 实时显示 Agent tmux 会话的输出，自动滚动
- **终端标签** — 完整的交互式终端，可以直接在容器里敲命令

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

## 当前能力 vs 近期计划

### 当前能力（Current）
- IAM 一期已接入：管理员菜单与用户管理接口可用
- 配置中心、运行时 Agent、通知能力已拆分为独立 crate
- 服务器/Agent/任务/通知现有 API 行为保持兼容

### 近期计划（Planned）
- 持续提炼各 crate 的 DDD 分层（`api -> application -> domain -> infrastructure`）
- 通知改造为事件驱动并扩展第三方需求管理系统集成
- 继续完善细粒度鉴权策略与可视化审计能力

---

## 架构演进顺序

后端按以下顺序演进：

1. IAM
2. 配置中心
3. 服务器 + Agent 运行时
4. 通知中心

---

## 从源码构建

**前置条件：** Rust 1.88+、Node 20+、Docker、`sqlx-cli`

```bash
# 安装带 PostgreSQL 支持的 sqlx-cli
cargo install sqlx-cli --no-default-features --features native-tls,postgres

# 启动临时 postgres 并生成 sqlx 离线缓存
./scripts/prepare-sqlx.sh

# 构建前端
cd frontend && npm install && npm run build && cd ..

# 启动（需要 postgres 在运行）
export DATABASE_URL=postgres://codexfleet:codexfleet@localhost:5432/codexfleet
cargo run -p backend
```

访问 **http://localhost:3000**

---

## 开源许可

Apache 2.0
