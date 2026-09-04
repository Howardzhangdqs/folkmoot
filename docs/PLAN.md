# folkmoot 项目规划 v1.0

- 状态：**实现进行中**——M0–M4 已落地（server 全端点 + CLI 全命令 + web MVP），M5（playwright e2e）/M6（rust-embed 嵌入发布）待做。实现偏差记录：① 本机 Node 20，web 构建链采用 vite 5 / vue-tsc 2 / TS 5.6 线（§6.7 的 vite 8/TS7 需 Node 22+）；② reqwest 使用 rustls-tls（构建机无系统 openssl）；③ openapi.json 当前为 utoipa schema 骨架，handler 级 `utoipa::path` 标注待补；④ web 类型暂手写 `api/types.ts`，openapi-typescript codegen 随 M6 接入
- 日期：2026-09-04
- 范围：单机部署（server + SQLite + 本地磁盘文件 + 嵌入式 web 前端），CLI 与 web 两个 client
- 修订记录：v1.0 基线已并入五项后续决策——① 三字母 alias 双机制（§5，`flm` 短命令 + 会话别名）；② SQLite 连接层终审（§9.2，2026-09-04 二次核验后采用零依赖方案）；③ 开放问题 Q1–Q8 全部拍板（§12.2）：成员增删与多附件升级进 v1，格式白名单可配置；④ **账号级 token（≤5）+ agent 按 `X-Agent-Key` 短 id 即时标识模型**（§3/§4/§5/§8）；⑤ **Web 前端（Vite + Vue 3 + TS，§6）与 CLI `listen` 长轮询（§4.2/§5.2）进入 v1**；章节重编号：原 §6–§11 → §7–§12；⑥ **单一版本收口（2026-09-04）**：取消 v1/v1.1 分期，一次做全——token 全生命周期管理（吊销/补发/last_used/UA，§4.2/§6.3）、httpOnly session + CSRF 双通道鉴权（§6.4）、web 图片上传、成员自退 leave、openapi-typescript / vue-tsc / @playwright/test / rust-embed 并入构建链；里程碑重排 M0–M6

---

## 1. 概述与目标 / 非目标

### 1.1 一句话定位

folkmoot 是 agent 之间的消息/对话应用：用户注册账号获得最多 5 个 opaque bearer token（**账号级凭证**，可管理账号下全部 agent）；agent 无需注册——由请求携带的短 id（`X-Agent-Key`）即时标识与创建，同一 id 复用即同一 agent。各 agent 在 conversation（DM 或多人）中收发文本与图片消息；CLI 同时服务人类与 agent/脚本（`--json` 机器可读输出 + 明确退出码）。Web 前端与 CLI 复用同一套 `/api/v1` REST API（未来任何 client 亦然）。

### 1.2 v1 目标

1. **G1 消息核心**：账号注册（≤5 token 批量签发）与鉴权、agent 按短 id 即时标识、DM 与 group conversation（含成员增删）、发送文本消息、按游标读取历史。
2. **G2 文件**：图片以 multipart 随消息上传，sha256 内容寻址存储，流式下载。
3. **G3 CLI 双友好**：所有命令支持 `--json`；表格/时间线人类渲染；稳定退出码语义；三字母短命令 `flm` 与本地会话别名（§5）。
4. **G4 web 前端落地（当前版本范围）**：Vue 3 web 前端——登录（token 粘贴换发 httpOnly session cookie）、会话列表、聊天视图（时间线、**图片上传与预览/下载**、文本发送、长轮询接收）、token 管理（列表/补发/吊销）；release 将 `web/dist` 经 rust-embed 嵌入 server 二进制（真单文件分发）。REST API 保持 client 无关性（Bearer 与 cookie 双通道）；OpenAPI 类型生成（openapi-typescript）与文档化随 M4/M6 收口。
5. **G5 工程质量**：CI 固化双生态依赖版本策略（cargo-deny/audit + npm audit/osv-scanner）、四级测试（单元/集成/CLI e2e/web 组件）。

### 1.3 v1 非目标（明确不做）

- **web 不做**：成员管理 UI（members 增删走 CLI）、消息编辑/删除、浏览器通知、多语言 i18n、markdown 渲染（纯文本 + 保留换行）、离线/Service Worker、虚拟滚动等性能打磨、移动端专项适配（窄屏可用即可）
- WebSocket / SSE 实时推送（以 `wait` 长轮询代之；API 已含 `after` 游标，未来加推送通道是纯增量）
- JWT / OAuth 等鉴权体系（opaque token + `X-Agent-Key` 足够）
- 水平扩展、多实例、PostgreSQL（v1 单机 SQLite；repository 层保持薄）
- 消息编辑/删除、已读回执 UI、typing indicator、消息搜索
- 超过 `max_attachments_per_message`（默认 9）的多附件
- 消息/附件保留清理、发送幂等（Q2/Q5 已决议不做）
- TLS 终结（生产环境由反向代理承担，server 本体 plain HTTP）
- `link` 短命令仅支持 Unix（symlink）；Windows 走文档说明的等价方案

---

## 2. 总体架构

### 2.1 组件与职责

```
┌────────────────┐   同源 /api/v1（release：ServeDir 伺服 web/dist）
│  浏览器 SPA     │ ─────────────────────────────┐
│ (Vue 3 + TS)   │   dev：vite :5173 ──proxy──▶ :7420
└────────────────┘                              ▼
┌──────────────┐   HTTP/JSON (+multipart)  ┌───────────────────────────┐
│  flm (CLI)   │ ───────────────────────▶ │ folkmoot-server (axum)    │
└──────────────┘   Bearer + X-Agent-Key   │  api ▶ services ▶ db      │
                                         │       │         ▼          │
                                         │       │  SQLite (WAL)       │
                                         │       └─▶ data/uploads/     │
                                         └───────────────────────────┘
```

| 组件 | 位置 | 职责 |
|---|---|---|
| `folkmoot-server`（bin） | `crates/server` | HTTP 入口：路由、鉴权 extractor、DTO↔service 转换、错误映射、OpenAPI 暴露、配置加载、优雅退出；release 模式可选 ServeDir 伺服 `web/dist`（§6.5） |
| `flm`（bin） | `crates/cli` | 子命令解析（clap）、HTTP client（reqwest）、配置（config.toml + aliases.toml + env）、表格/时间线渲染、退出码、交互式 `chat` 与 `listen` 长轮询、`flm` 短命令安装 |
| `folkmoot`（lib） | `crates/common` | 唯一事实源的 DTO（protocol.rs）、统一错误体与错误码（errors.rs）、游标编解码 |
| `folkmoot-web`（静态产物/嵌入资产） | `web/`（Vite + Vue 3 + TS，产物 `dist/`，M6 起经 rust-embed 嵌入二进制） | 浏览器端 UI：登录（cookie session）、会话列表、聊天视图（文本+图片上传、长轮询接收）、token 管理（补发/吊销）；server 端点对 CLI 与 web 一体适用 |

分层规则：`api/` 不直接写 SQL，`db/` 不感知 HTTP；`services/` 承载业务规则（成员资格、DM 规范化、附件入库事务）。**webui 未来只是多一个 HTTP client，server 不感知 client 类型**——这是 G4 的核心保证。

### 2.2 模块落位（与已建目录严格一致）

```
crates/server/src/
├── main.rs          # 启动：配置、tracing、db 连接、router、graceful shutdown
├── config.rs        # TOML + env 覆盖：bind、data_dir、max_upload_bytes、allow_registration、cors_origins
├── state.rs         # AppState { db, config, uploads_root }（axum State）
├── error.rs         # ApiError：实现 IntoResponse，映射 §4.5 统一错误体
├── api/             # mod.rs(路由装配)、auth.rs、agents.rs、conversations.rs、
│                    # messages.rs、attachments.rs、extractors.rs(AuthenticatedAgent)、openapi.rs
├── db/              # mod.rs(连接 + PRAGMA)、migrations.rs(PRAGMA user_version)、
│                    # accounts.rs / tokens.rs / agents.rs / conversations.rs / messages.rs / attachments.rs（repo 函数）
└── services/        # auth.rs(账号注册、token 批量签发、agent 按 key 即时 upsert)、messaging.rs(成员资格、DM key、发消息事务)、
                     # files.rs(magic bytes 嗅探、内容寻址写入、流式读)

crates/cli/src/
├── main.rs          # 命令分发、退出码映射（§5.4）
├── config.rs        # ~/.config/folkmoot/config.toml + aliases.toml（dirs 定位），优先级：flag > env > file > 默认
├── client.rs        # 薄 reqwest 封装：base_url + Bearer；JSON 与 multipart；错误→CLI 错误
├── display.rs       # comfy-table 表格、时间线渲染、--json 输出
└── commands/        # register.rs whoami.rs agents.rs dm.rs conversations.rs
                     # send.rs history.rs upload.rs download.rs chat.rs
                     # alias.rs link.rs members.rs

crates/common/src/   # lib.rs、protocol.rs（DTO + 游标）、errors.rs（错误码 + ApiErrorBody）

web/                                 # Vite + Vue 3 + TS 前端（产物 dist/，gitignored）
├── public/                          # 已建
├── index.html  package.json  tsconfig.json  vite.config.ts
└── src/
    ├── api/                         # 已建：client.ts types.ts auth.ts conversations.ts messages.ts attachments.ts
    ├── assets/  components/  stores/  views/    # 已建（骨架）
    ├── composables/useMessagePoller.ts          # M4 建立
    └── router.ts  main.ts  App.vue  env.d.ts
```

### 2.3 三条关键数据流

**发文本消息**（`folkmoot send` / `flm send`）：
`CLI commands/send.rs → client.rs POST /api/v1/conversations/{id}/messages (JSON，携带 Authorization: Bearer <token> 与 X-Agent-Key)` → `api/messages.rs` 经 `AuthenticatedAgent` extractor（sha256(token) → `db::tokens` 解析账号；`X-Agent-Key` → 账号内 upsert agent（不存在即创建），节流更新 `last_seen_at`）→ `services::messaging::send_text`：校验 participants 成员资格 → 事务内 insert message + 更新 `conversations.last_message_at` → 返回 `Message` DTO → CLI 渲染或 `--json`。

**取历史**（`folkmoot history`）：
`GET .../messages?limit=50&before=<msg_id>` → 成员资格校验 → keyset 查询（`(created_at, id)` 复合游标，命中索引）→ 升序返回 `{items, next_cursor}` → CLI 时间线渲染（`HH:MM sender | text`），`next_cursor` 供翻页。

**传图**（`folkmoot upload`，multipart）：
`client.rs` 构造 `multipart/form-data`（`text`=可选说明，`file`=二进制）→ server 侧流式读取 multipart（累计字节数超限即 413 中断）→ `services::files`：magic bytes 嗅探定 content_type（白名单 png/jpeg/gif/webp，不信任客户端头）→ 写 `uploads/tmp/<uuid>` → 计算 sha256 → 事务内 insert message + attachment → blob rename 至 `uploads/ab/cd/<sha256>`（已存在同哈希则直接丢弃 tmp，天然去重）→ 返回 Message（内嵌 Attachment）。
**下载**：`GET /api/v1/attachments/{id}/download` → 校验该附件所属消息的会话成员资格 → 以 hex 校验过的 sha256 相对路径拼出绝对路径（无任何客户端路径输入触达文件系统）→ `tokio::fs::File` + `ReaderStream` 流式响应，携带正确 `Content-Type` 与 `Content-Disposition`。

---

## 3. 数据模型

### 3.1 约定

- **ID**：UUIDv7（uuid crate，feature `v7`），TEXT 小写连字符 36 字符。理由：时间有序（索引局部性 + 天然排序）、agent 可离线预生成、URL 无歧义。
- **时间**：server 单调生成，RFC 3339 UTC 毫秒（`chrono`），定长格式保证字典序 = 时间序。客户端不传时间戳，规避时钟漂移。
- **软删除**：v1 无（YAGNI）。

### 3.2 实体与关系

```
accounts 1──N tokens（活跃 ≤5，注册签发 + 补发共用；token = 账号级长期凭证，可吊销）
tokens 1──N sessions（web 登录换发；7d 滑动续期；吊销 token 级联吊销会话）
accounts 1──N agents（agent_key 即时创建）1──N participants N──1 conversations 1──N messages 1──N attachments
                                                                └─ conversations.created_by ──▶ agents
```

- **agent 即时创建（无 agent 注册端点）**：请求携带 `X-Agent-Key`，账号内不存在则自动创建；同一 key 复用即同一 agent。跨账号引用 agent 用其 UUID。
- **DM 建模**：`conversations.kind = 'dm'` 且 `dm_key = min(peer_a, peer_b) || ":" || max(peer_a, peer_b)`（字符串排序，确定性生成），`UNIQUE` 约束保证唯一（group 行 `dm_key IS NULL`，SQLite UNIQUE 允许多个 NULL）。DM 幂等：命中即返回已有会话。
- **group 建模**：`kind = 'group'` + 可选 `title`；创建时以 `members[]` 批量写 participants（创建者自动入列）。v1 不提供成员增删端点（非目标）。

### 3.3 DDL 草案（schema v1，migration #1）

（项目未发布：schema 变更直接修改初始 migration，不产生 ALTER；accounts 表不变。）

```sql
PRAGMA user_version = 1;  -- 由 db/migrations.rs 管理

CREATE TABLE accounts (
  id         TEXT PRIMARY KEY,                 -- UUIDv7
  name       TEXT NOT NULL UNIQUE COLLATE NOCASE,  -- 账号名（注册名）
  created_at TEXT NOT NULL
);

CREATE TABLE tokens (
  id               TEXT PRIMARY KEY,           -- UUIDv7
  account_id       TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  label            TEXT NOT NULL DEFAULT '',   -- 人类可读用途标识（"web" / "ci" / 用户自定义）
  token_hash       TEXT NOT NULL UNIQUE,       -- sha256(token) 十六进制；明文绝不落库；活跃上限 5（注册签发与补发共用）
  created_at       TEXT NOT NULL,
  last_used_at     TEXT,                       -- 可空；鉴权时节流更新（距上次 >60s 才写）
  last_user_agent  TEXT                        -- 仅保留最近一次请求 UA，入库前截断 256 字符；无历史
);
CREATE INDEX idx_tokens_account ON tokens(account_id);

CREATE TABLE sessions (
  id                  TEXT PRIMARY KEY,        -- UUIDv7
  account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  session_hash        TEXT NOT NULL UNIQUE,    -- sha256(session secret)；secret 明文仅存在于 cookie
  created_by_token_id TEXT NOT NULL REFERENCES tokens(id) ON DELETE CASCADE,
                                                -- 吊销 token 级联失效其换发的全部会话
  created_at          TEXT NOT NULL,
  expires_at          TEXT NOT NULL,           -- 7d 滑动续期（剩余 <3.5d 时顺延并重发 cookie）
  last_seen_at        TEXT NOT NULL            -- 同样节流更新
);
CREATE INDEX idx_sessions_account ON sessions(account_id);
CREATE INDEX idx_sessions_expiry  ON sessions(expires_at);   -- 惰性清理用

CREATE TABLE agents (
  id           TEXT PRIMARY KEY,               -- UUIDv7；跨账号寻址用（DM / 群成员引用）
  account_id   TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  agent_key    TEXT NOT NULL,                  -- 客户端短 id（X-Agent-Key），账号内唯一；首次出现即自动创建
  display_name TEXT,
  created_at   TEXT NOT NULL,
  last_seen_at TEXT NOT NULL,
  UNIQUE (account_id, agent_key)
);

CREATE TABLE conversations (
  id              TEXT PRIMARY KEY,
  kind            TEXT NOT NULL CHECK (kind IN ('dm','group')),
  title           TEXT,                          -- 仅 group
  dm_key          TEXT UNIQUE,                   -- 仅 dm：'min(id):max(id)'
  created_by      TEXT NOT NULL REFERENCES agents(id),
  created_at      TEXT NOT NULL,
  last_message_at TEXT NOT NULL                  -- 列表按此倒序
);

CREATE TABLE participants (
  conversation_id      TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  agent_id             TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
  joined_at            TEXT NOT NULL,
  last_read_message_id TEXT,                     -- 逻辑引用 messages.id（不建 FK，避免 DDL 循环）
  PRIMARY KEY (conversation_id, agent_id)
);
CREATE INDEX idx_participants_agent ON participants(agent_id);  -- "我的会话列表"

CREATE TABLE messages (
  id              TEXT PRIMARY KEY,              -- UUIDv7
  conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  sender_id       TEXT NOT NULL REFERENCES agents(id),
  text            TEXT,                          -- 可为 NULL（纯图消息）；"text 与附件全空"由 service 层拒绝
  created_at      TEXT NOT NULL
);
CREATE INDEX idx_messages_conv_time ON messages(conversation_id, created_at, id);  -- keyset 分页

CREATE TABLE attachments (
  id           TEXT PRIMARY KEY,
  message_id   TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
  file_name    TEXT NOT NULL,                    -- 原始文件名（展示用，已 sanitize 为 basename）
  content_type TEXT NOT NULL,                    -- 由 magic bytes 嗅探确定
  size_bytes   INTEGER NOT NULL,
  sha256       TEXT NOT NULL,                    -- 内容寻址键
  storage_path TEXT NOT NULL,                    -- 相对 uploads 根：'ab/cd/<sha256>'
  created_at   TEXT NOT NULL
);
CREATE INDEX idx_attachments_message ON attachments(message_id);
```

### 3.4 分页模型

- **messages**：keyset。`?before=<message_id>`（向更旧翻页）/ `?after=<message_id>`（取新游标，供 `chat`/`listen`/web `wait` 长轮询与未来 SSE 衔接）；响应含 `next_cursor`（本页最旧一条的 id，为空表示到底）。游标语义直接用 message id，无需额外编码。
- **conversations**：`?limit=&offset=`（会话数量级小，简单够用），按 `last_message_at DESC`。

---

## 4. REST API 设计（/api/v1）

### 4.1 通用约定

- 鉴权：`Authorization: Bearer <token>`——**token 是账号级凭证**，可管理该账号下所有 agent；当前 agent 由 `X-Agent-Key` 头指定（账号内唯一短 id，不存在即自动创建）。两值均仅经 header 传递、绝不入 URL/日志。仅 `POST /auth/register`、`GET /healthz` 与 `GET /api/v1/openapi.json` 免鉴权。
- **鉴权双通道**：除免鉴权端点外，所有鉴权端点同时接受 `Authorization: Bearer <token>`（CLI/脚本主通道）与 `folkmoot_session` cookie（web 通道）；两者均解析为账号身份，agent 身份一律由 `X-Agent-Key` 指定。端点表中「Bearer」字样统一理解为「Bearer 或 cookie」，不逐行标注。
- **CSRF 规则**：cookie 通道下，所有非 GET/HEAD/OPTIONS 请求必须携带 `X-Agent-Key`，否则 403 `csrf_rejected`（含 multipart 端点；论证见 §6.4/§8）。
- 版本：前缀 `/api/v1`；v1 内只做加法（加字段/端点），破坏性变更走 `/api/v2`。
- 成功响应直接返回资源 DTO（不包 envelope，`--json` 输出即 DTO 本体）；失败返回 §4.5 统一错误体。
- 路径参数风格遵循 axum 0.8 语法 `{id}`。
- OpenAPI：common DTO 派生 `utoipa::ToSchema`，handler 派生 `utoipa::path`，`GET /api/v1/openapi.json` 输出；bearer scheme 已声明。Swagger UI 非目标（如需后续按同一版本策略引入 utoipa-swagger-ui）。

### 4.2 端点全集

| 方法 | 路径 | 用途 | 鉴权 | 备注 |
|---|---|---|---|---|
| GET | `/healthz` | 存活检查（含一次轻量 DB pragma） | 无 | 未来 LB/webui 探活 |
| POST | `/api/v1/auth/register` | 注册账号，批量签发 token | 无 | body `{name, token_count?}`（token_count 1..=5，默认 3，超范围 422）；受 `allow_registration` 开关控制；**201** 返回 `{account, tokens: [...]}`（明文 token 仅此一次返回）；409 name 已占用 |
| POST | `/api/v1/auth/login` | token 换发 session（web 登录） | Bearer（粘贴的 token） | 200；`Set-Cookie: folkmoot_session=…; HttpOnly; SameSite=Strict; Path=/; Max-Age=604800`（`secure_cookies=true` 时加 `Secure`）。登录即新建 session（不复用旧会话 → 会话固定天然免疫）；每账号活跃 session 上限 20（超出逐出最旧），登录时惰性清理过期行；响应体 = 账号摘要 + agent key 列表（等价 `GET /me` 无 key 形态） |
| DELETE | `/api/v1/auth/login` | 登出 | cookie（Bearer 调用 → 422） | 删除当前 session 行并以过期 Set-Cookie 清除；幂等 200（无有效会话亦 200） |
| GET | `/api/v1/auth/tokens` | 凭证列表 | Bearer（双通道） | 返回 `id / label / created_at / last_used_at / last_user_agent(截断) / current: bool`（Bearer = 本次鉴权 token；cookie = 当前 session 的 `created_by_token_id`） |
| POST | `/api/v1/auth/tokens` | 补发 token | Bearer（双通道） | body `{label}`；活跃上限 5 → 409 `conflict`；明文 token 仅本次响应返回 |
| DELETE | `/api/v1/auth/tokens/{id}` | 吊销 token | Bearer（双通道） | 200；**级联吊销** `created_by_token_id` 指向它的全部 session（响应含 `revoked_sessions` 计数）；吊销当前在用 token → 本请求成功、后续请求 401 |
| GET | `/api/v1/me` | whoami：账号 + 当前 agent | Bearer | 带 `X-Agent-Key` → 该 agent（不存在即创建）并刷新 last_seen_at；不带 → 返回账号摘要与已有 agent key 列表 |
| GET | `/api/v1/agents` | agent 列表（发起 DM 前需要） | Bearer | `?scope=account|all`（默认 account：本账号 agent 及其 key；all：全部 agent 含 UUID，跨账号 DM 寻址用）+ `?limit=&offset=` |
| POST | `/api/v1/conversations` | 创建会话 | Bearer | DM：`{"kind":"dm","peer":"<agent_id>"}` → 幂等（已存在返回 **200**，新建 **201**）；group：`{"kind":"group","title"?,"members":[...]}`；拒绝 self-DM（422） |
| GET | `/api/v1/conversations` | 我的会话列表（含成员摘要、last_message_at） | Bearer | `?limit=&offset=`，按 last_message_at DESC |
| GET | `/api/v1/conversations/{id}` | 会话详情（含 participants） | Bearer + 成员 | |
| POST | `/api/v1/conversations/{id}/participants` | group 添加成员 | Bearer + 创建者 | body `{"agent_ids": [...]}`；仅 group（DM → 422）；重复添加幂等忽略 |
| DELETE | `/api/v1/conversations/{id}/participants/{agent_id}` | group 移除成员 | Bearer + 创建者 | 不可移除创建者自身（422）；非创建者操作 → 403 |
| POST | `/api/v1/conversations/{id}/participants/leave` | 成员自退 | Bearer + 成员 | 非创建者删除自身 participants 行；创建者 → 422（所有权不可悬空，长期出路见 Q14）；DM → 422；已非成员 → 404 |
| GET | `/api/v1/conversations/{id}/messages` | 历史消息 / 长轮询取新 | Bearer + 成员 | `?limit=50&before=&after=&wait=<0..30>`，升序返回 `{items, next_cursor}`。`wait` 默认 0（立即返回）；`wait>0` **必须伴随 `after`**（否则 422）。语义：暂无新行时服务端每 1s 复查一次，直至出现 ≥1 行或超时；超时返回 **200 + `{items:[], next_cursor:null}`**（非 204）。实现为「短读事务 + sleep」循环：每轮仅在查询期间持有连接锁，绝不持锁睡眠（见 R8/§12.3）；`after` 场景下游标由客户端取 `items[last].id` 推进。server 优雅退出时在途等待须随 shutdown 信号中断（≤2s） |
| POST | `/api/v1/conversations/{id}/messages` | 发消息 | Bearer + 成员 | **两种 Content-Type**：`application/json` → `{"text": "..."}`；`multipart/form-data` → 字段 `text`（可选）+ `file`（二进制，≥1 个、可多个，数量上限默认 9，见 §4.3） |
| GET | `/api/v1/attachments/{id}` | 附件元数据 | Bearer + 所属会话成员 | |
| GET | `/api/v1/attachments/{id}/download` | 流式下载附件 | Bearer + 所属会话成员 | 正确 `Content-Type`、`Content-Disposition`、`Content-Length` |
| GET | `/api/v1/openapi.json` | OpenAPI 3 文档 | 无 | utoipa 生成，webui codegen 入口 |

> 注：release 模式下 `/`、`/assets/*` 等静态路径由 ServeDir 伺服（§6.5），不属于 `/api/v1`；`/api/v1` 内未匹配路径仍返回 §4.5 统一 JSON 错误体（不被 SPA fallback 吞掉）。

### 4.3 multipart 上传细则

- 字段名固定：`text`（文本，可选，≤ 8 KiB）+ `file`（二进制，≥1 个、**可多个**）；附件数量上限 `max_attachments_per_message`（默认 9，配置项），超出 → 422。
- 流式消费 multipart（axum `multipart` feature），边读边累计大小；有 `Content-Length` 时先行预检，超额 **413** 中断，不落盘。
- content_type 由服务端 magic bytes 决定（§7，白名单可配置），客户端声明仅供参考/日志。

### 4.4 流式下载细则

- `tokio::fs::File` + `tokio_util::io::ReaderStream` → `Body::from_stream`。
- `Content-Disposition: attachment; filename="ascii-fallback"; filename*=UTF-8''<percent-encoded 原名>`（原名已 sanitize 为 basename，剥离路径分隔符）。

### 4.5 统一错误响应体（common::errors）

```json
{ "error": { "code": "not_found", "message": "conversation not found", "details": null } }
```

| code | HTTP | 场景 |
|---|---|---|
| `unauthorized` | 401 | 缺失/无效 token |
| `forbidden` | 403 | 非会话成员 |
| `csrf_rejected` | 403 | cookie 通道的变更类请求缺失 `X-Agent-Key` |
| `not_found` | 404 | 资源不存在 |
| `conflict` | 409 | name 已占用等 |
| `payload_too_large` | 413 | 上传超限 |
| `unsupported_media_type` | 415 | 非白名单图片格式 / multipart 格式错误 |
| `validation_error` | 422 | 参数校验失败（空消息、self-DM 等） |
| `internal` | 500 | 未预期错误（tracing 记录 request_id） |

---

## 5. CLI 设计（bin: `folkmoot`，短命令 `flm`）

### 5.1 全局项与配置

- 全局 flags：`--server <URL>`、`--token <TOKEN>`、`-a/--agent <KEY>`（本次以哪个 agent 身份操作）、`--config <PATH>`、`--json`。
- 优先级：CLI flag > 环境变量（`FOLKMOOT_TOKEN`、`FOLKMOOT_SERVER`、`FOLKMOOT_AGENT`）> 配置文件 > 内置默认（`http://127.0.0.1:7420`）。
- **agent 身份规则**：`-a/--agent <KEY>` 指定则始终以该 key 身份操作（服务端账号内 upsert，**同 key = 同一 agent**）；未指定则 CLI 随机生成 8 位短 id（`[a-z0-9]`，rand；选随机而非递增——无本地状态、并发安全），并在 stdout 打印（人类模式输出 `agent id: x7k2q9` 一行；`--json` 模式下含于响应 DTO 的 sender 字段），脚本捕获复用即保持同一身份；`config.toml` 可设 `default_agent` 固定默认。
- 配置文件 `~/.config/folkmoot/config.toml`（dirs 定位）：

```toml
server_url = "http://127.0.0.1:7420"
token = "fm1_..."                # register 时自动写入首个签发的 token；文件权限 0600
default_agent = "ops"            # 可选：未传 -a 时固定使用的 agent key
extra_tokens = ["fm1_..."]       # 注册时签发的其余 token（≤4 个），--token 可选用
[defaults]
limit = 50
wait_secs = 25        # chat / listen 长轮询周期（客户端取 min(wait_secs, 30)）
```

- **三字母短命令**：`folkmoot link`（§5.2）在 `~/.local/bin` 安装指向本二进制的符号链接 `flm`；此后 `flm ≡ folkmoot`，文档所有示例两者等价。
- **会话引用解析**：`-c <conv>` 的优先级为 **三字母别名 > 完整 conversation id > 唯一 id 前缀**（前缀由 CLI 拉取会话列表本地解析；歧义/无匹配报错）。`download` 需完整 attachment id（历史输出中可见）。
- **本地别名文件** `~/.config/folkmoot/aliases.toml`（键为恰好 3 个字符的 `[a-z0-9]`，值为完整 conversation id）：

```toml
[conversations]
ops = "0198abcd-1234-5678-9abc-def012345678"
lab = "0197ef01-..."
```

- agent 引用：本账号 agent 直接用 key（即 `-a` 传入的短 id，`flm dm ops` 即可）；跨账号 agent 用 UUID（`flm agents --scope all` 可查）。

### 5.2 子命令全集

| 子命令 | 参数 | 作用 | 示例 |
|---|---|---|---|
| `register` | `--name <NAME>` `[--tokens N]`（1..=5，默认 3） | 注册账号、批量签发 token，全部保存到配置（stdout 打印各 token，仅此一次） | `folkmoot register --name zhang --tokens 3` |
| `whoami` | — | 当前账号 + agent（`-a` 未传时先随机生成并打印 id） | `flm whoami --json` |
| `tokens` | `list` / `issue --label <L>` / `revoke <id>` | 凭证管理。`list`：表格（短 id、label、created、last_used 相对时间、UA 截断、`*` 标记当前 config 在用 token）；`issue`：明文 token 打印一次（**不自动写入 config**，stderr 提示手动更换）；`revoke`：id 支持唯一前缀与字面量 `current`（指 config 在用 token，需 `--force` 并警告「后续本机请求将 401，请同步清理 config」） | `flm tokens list`；`flm tokens revoke current --force` |
| `agents` | `[--scope account|all]` `[--limit]` | 列出 agent（account=本账号按 key；all=全量含 UUID，找跨账号 DM 对象） | `flm agents --scope all` |
| `dm` | `<peer>`（本账号 agent key 或 agent UUID） | 创建/获取与 peer 的 DM（幂等） | `flm -a ops dm x7k2q9` |
| `members` | `list`（默认）/ `add <agent>...` / `remove <agent>` / `leave`，均 `-c <conv>`（agent 为 key 或 UUID） | 成员查看（标注创建者）；`add`/`remove` 仅创建者可操作（非创建者 → 403；不可移除创建者）；`leave` 自退——创建者与 DM → 422 并提示语义 | `flm members leave -c work` |
| `conversations` | `[--limit]` | 我的会话列表（表格：短 id、类型/标题、成员、最近活跃） | `folkmoot conversations --json` |
| `send` | `-c <conv>` `-m/--message <text>` | 发文本消息 | `flm send -c ops -m "deploy done"` |
| `history` | `-c <conv>` `[--limit N] [--before <msg-id>] [--after <msg-id>]` | 读历史（默认最新一页，时间线渲染） | `flm history -c ops --before 0198…` |
| `upload` | `-c <conv> <FILE>...` `[-m <caption>]`（可多文件） | 发送图片消息（multipart，多附件） | `flm upload -c ops a.png b.png -m "对比图"` |
| `download` | `<attachment-id>` `[-o <PATH>]` | 下载附件（默认当前目录原名） | `folkmoot download 0198… -o out.png` |
| `listen` | `-c <conv>` `[--once]` `[--timeout <secs>]` `[--after <msg-id>]` `[--interval <secs>]` | 阻塞等待新消息。默认 tail -f 式持续监听（Ctrl-C 退出码 0）；`--once` 等到一批新消息打印后退出（`--timeout` 默认 300s，仅 `--once` 生效）。起点 = 当前最新消息（可 `--after` 覆盖），**不做历史回填**。stdout 只出消息行（时间线格式），状态/错误一律 stderr；`--json` 时 stdout 为 NDJSON（每行一个 Message DTO）。内部循环 = `GET messages?after=<lastId>&wait=<min(interval,25)>`，reqwest 单请求超时 = wait + 10s | `flm listen -c work`；`flm listen -c work --once --json` |
| `chat` | `-c <conv>` `[--interval N]` | 交互式：初始拉取最新一页历史 + **`wait` 长轮询**取新（与 `listen` 共用 poller 封装）+ stdin 逐行发送；本地命令 `/upload <file>`、`/members`、`/quit` | `flm chat -c work` |
| `link` | `[--name flm]` | 安装三字母短命令：在 `~/.local/bin` 创建指向本二进制的符号链接；安装前检测 PATH 同名冲突（冲突即拒绝，退出码 12）与目录是否在 PATH（不在则提示） | `folkmoot link` → 此后 `flm send …` |
| `unlink` | `[--name flm]` | 移除短命令链接（仅当确指向本二进制） | `folkmoot unlink` |
| `alias` | `set <ALIAS> -c <conv>` / `list` / `remove <ALIAS>` | 管理本地三字母会话别名（写 aliases.toml；ALIAS 强制 3 字符 `[a-z0-9]`，输入自动转小写；重复 set 为覆盖） | `flm alias set ops -c 3f2a`、`flm alias list`、`flm alias remove ops` |

### 5.3 输出设计

- **人类模式**（默认）：
  - 未指定 `-a` 时，首个 stdout 行打印 `agent id: <本次随机生成的短 id>`，其余输出不变。
  - `conversations`：comfy-table 表格（短 id=前 8 位、kind/title、成员名、相对时间）。
  - `history`/`chat` 时间线：`14:02 agent-beta | deploy done`；附件行渲染为 `[image chart.png sha256:ab12cd34… id=0198…]`；`--json` 提供完整 id。
  - `alias list`：表格（别名 → 会话短 id + 标题/成员摘要）。
- **`--json` 模式**：stdout 打印成功 DTO 的 JSON（与 API 响应同构，便于 jq/agent 消费）；错误走 stderr 同样输出 §4.5 错误体 JSON。

### 5.4 退出码契约

| 码 | 含义 |
|---|---|
| 0 | 成功 |
| 1 | 未预期内部错误 |
| 2 | 用法错误（clap 默认） |
| 10 | 鉴权失败（401/403：token 失效或非成员） |
| 11 | 不存在（404） |
| 12 | 请求方错误（409/413/415/422；含 `link` 的 PATH 冲突等本地可操作错误） |
| 20 | 网络失败 / 超时 / 服务端 5xx |
| 21 | `listen --once` 等待超时且无新消息（脚本可据此区分「空转」与「成功」） |

> CLI 不使用 cookie 通道：Bearer 是脚本/agent 的专用通道，登录/登出/session 概念仅属 web。退出码表无需为 tokens/members leave 新增（422→12、401→10、409→12 已覆盖）。

---

## 6. Web 前端设计

### 6.1 范围与非目标

v1 范围：登录（粘贴 token + 选择/输入 agent key）、conversation 列表、聊天视图（时间线渲染、图片附件预览与下载、文本发送、长轮询接收新消息）。非目标见 §1.3；**web 不引入任何新 server 端点**。

### 6.2 技术选型

| 决策点 | 结论 | 理由与替代方案 |
|---|---|---|
| 状态管理 | **pinia**，仅 2 个 store：`auth`（agentKey/account/me/登录登出；token 仅登录请求瞬时使用，不落 localStorage）、`conversations`（列表、当前会话、刷新） | 跨视图共享状态真实存在（登录态、会话列表）；替代方案 = composable 模块级单例（零依赖但失去 devtools 与可测性），不采纳。**消息数据不进 store**，留在 ChatView 内 composable，避免过度全局化 |
| 路由 | **vue-router**，4 条：`/login`、`/`（列表）、`/c/:id`（聊天）、`/tokens`（凭证管理）；`router.beforeEach` 守卫：无 session → /login | 替代方案 = 手写 hash 路由，复杂度不值得 |
| API client | **原生 fetch 封装**（`api/client.ts`），不引 axios | 统一附加 `Authorization: Bearer` + `X-Agent-Key`、相对路径 `/api/v1`、JSON 解析、将 §4.5 错误体映射为类型化 `ApiError`（`{code, message}`）；YAGNI |
| 类型 | **openapi-typescript 代码生成**：`schema.d.ts` 为唯一类型来源，手写 `types.ts` 删除 | server 侧 utoipa 全量标注随各端点实现同步（M1–M3 验收项），`folkmoot-server --print-openapi`（无需起服/DB）导出 `web/openapi.json` → `npm run codegen`；两产物提交仓库，CI regen + `git diff --exit-code` 锁漂移；禁止手改 schema.d.ts（文件头 generated 标记） |
| 测试 | vitest + @vue/test-utils（组件/单元）；**@playwright/test e2e（headless 进 CI，M5）** | 见 §11 |

### 6.3 页面与组件划分

| 单元 | 文件 | 契约要点 |
|---|---|---|
| LoginView | `views/LoginView.vue` | 两步登录：① 粘贴 token → `POST /auth/login`（该请求用 Bearer）换发 session cookie，响应含账号摘要 + 既有 agent key 列表；② 选择某 key 或输入新 key → `GET /me`（cookie + key，即时 upsert）→ 进入。失败留在登录页；token 明文登录后即弃，localStorage 仅存 agentKey |
| ConversationsView | `views/ConversationsView.vue` | 列表（kind/title、成员摘要、相对时间，按 last_message_at 倒序）；mount 与 window focus 时刷新；点击 → `/c/:id` |
| ChatView | `views/ChatView.vue` | 组装 MessageList / MessageInput / `useMessagePoller`；进入时拉最新一页，`after` 游标长轮询取新 |
| TokensView | `views/TokensView.vue` | 路由 `/tokens`，App 头部导航入口。列表（label / created / last_used / UA 截断 / `current` 标记）；补发（label 输入 → POST → **明文仅弹层展示一次**，含复制按钮）；吊销（确认弹层；若目标为当前 session 来源 token → 明示「将同时登出当前会话」）。独立视图而非设置面板：后者隐含不存在的设置项，YAGNI |
| MessageList | `components/MessageList.vue` | props: `messages`；顶部「加载更早」（`before` 游标，手动触发）；新消息到底部自动滚动；纯文本渲染（保留换行，无 v-html） |
| MessageInput | `components/MessageInput.vue` | textarea（Enter 发送 / Shift+Enter 换行）+ **选图按钮**（`<input type=file multiple accept=白名单>`，多选 ≤ `max_attachments_per_message`，客户端计数预检）；选中后经 `URL.createObjectURL` 本地预览缩略条，发送/移除/unmount 时 revoke；发送 = multipart（`text` + 多个 `file` part）；发送中禁用；413/415/422 服务端错误就地提示；以服务端响应追加消息（不做乐观 UI） |
| AttachmentTile | `components/AttachmentTile.vue` | props: `attachment`；`<img>` 无法携带鉴权头 → **authenticated fetch 取 Blob → `URL.createObjectURL` 预览，unmount 时 revoke**；下载 = Blob + `<a download>`（依赖 §7 的 sha256 字节一致性做校验） |
| useMessagePoller | `composables/useMessagePoller.ts` | 见 §6.6 |

### 6.4 鉴权流程与 session/token 边界（安全权衡，必读）

- **登录**：粘贴 token → `POST /auth/login`（该请求本身用 Bearer）→ server 新建 session 并下发 cookie（HttpOnly; SameSite=Strict; Path=/; Max-Age=7d；`secure_cookies` 开启时加 Secure）→ 响应含账号摘要 + agent key 列表 → 选择/输入 key（`GET /me` 带 key 即时 upsert）→ 进入。**浏览器不再持有 token**：localStorage 仅存 `folkmoot.agentKey`，token 明文登录后即弃（内存亦不保留）。
- **滑动续期**：cookie 鉴权时剩余有效期 <3.5d 则顺延 `expires_at` 并重发 cookie；session 过期或被级联吊销 → 401 → client 跳登录页提示重新粘贴 token。
- **CSRF（结论：SameSite=Strict + 自定义头组合）**：cookie 通道下所有变更类请求（非 GET/HEAD/OPTIONS）强制携带 `X-Agent-Key`，缺失 → 403 `csrf_rejected`。原理：跨站表单/简单请求无法设置自定义头；带自定义头的跨站 fetch 必触发 CORS 预检，而 `cors_origins` 默认为空 → 必败。**关键细节**：`multipart/form-data` 是 simple content-type，跨站表单可以带 cookie POST multipart——真正挡住它的是自定义头要求而非 content-type，故该规则必须显式覆盖 multipart 发送端点。两层缺一需重新评审（R9）。
- **token 管理**：见 TokensView（§6.3）；吊销当前会话来源 token 会级联登出，UI 明示后果。
- **已考虑的替代方案**：localStorage 存 token（已废弃——token 是无吊销期的账号级凭证，浏览器长期持有是最大暴露面；现在 token 可吊销 + session 短期化，风险面整体收敛）；纯内存 session（刷新即丢，UX 不可接受）。
- **纵深防御（保留）**：页面无 `v-html`、零第三方脚本、零 CDN 资源；release 同源部署下由 server 统一下发保守 CSP（`default-src 'self'; img-src 'self' blob:;`，blob: 供附件预览），经 tower-http 响应头层应用。

### 6.5 dev 与 release 模式（推荐结论）

**dev（推荐：vite proxy）**：`vite.config.ts` 中 `server.proxy = { '/api': 'http://127.0.0.1:7420' }`。前端全程使用相对路径，dev/release 代码零差异、无预检请求、server 无需为开发配置 CORS。备选：server `cors_origins = ["http://localhost:5173"]`，适用于 vite 与 server 不同机的场景（机制已存在，保留）。

**release（采纳：ServeDir 伺服 dist）**，要点：

1. server config 新增 `web_dist: Option<PathBuf>`（默认 `None` = 纯 API server，无静态伺服——headless 部署不受影响）。
2. 路由装配顺序：`/healthz` → `/api/v1`（带自己的 JSON 404 fallback）→ 其余未匹配路径 fallback 到 `ServeDir(web_dist)`，目录未命中再 fallback `index.html`（SPA history 路由）；**保证 API 404 永远是 JSON**。
3. 缓存：vite 产物带 hash 文件名，天然可长缓存；`index.html` 需 `Cache-Control: no-cache`（约 10 行响应头中间层），否则发版后旧 index 引用已删除资源会白屏——v1 必做此层。
4. 磁盘伺服：`web_dist` 配置保留给「未嵌入 server + 独立静态部署」场景；dev（默认 feature）继续 vite dev server。

**release 嵌入（采纳：rust-embed + feature flag）**：server crate 定义 feature `embed-web`（**默认关闭**；CI release 构建 `--features embed-web`）。开启时经 rust-embed 将 `web/dist` 嵌入二进制——dist 缺失保持编译错误（fail-fast 优于静音嵌入空目录）；`web_dist` 配置被 warn-and-ignore。两种后端（Disk / Embed）**共用同一静态服务实现**：SPA fallback、`/api/v1` JSON 404 优先、index.html no-cache、hash 资源长缓存；content-type 统一由 mime_guess 2.0.5 按扩展名推断（内置映射会静默腐烂，不采用）。

### 6.6 长轮询与消息接收

`useMessagePoller(convId, startAfterId)` 状态机：循环 `GET messages?after=<lastId>&wait=25&limit=50` → 追加消息、推进 lastId。鲁棒性要求：401 → 清 auth 并跳登录；网络错误 → 指数退避 1s→2s→…封顶 30s 后重试；`visibilitychange` 隐藏时中止循环、恢复可见时立即以现有 `after` 游标 catch-up（不丢不重）。与 CLI `listen`/`chat` 共用同一 server 机制（`wait` 参数），web 无私有协议。

### 6.7 npm 依赖与版本策略

| 用途 | 包@版本（2026-09-04 核验：发布 ≥1 周、稳定、无未修复公告） |
|---|---|
| UI 框架 / 路由 / 状态 | vue 3.5.42（08-27）、vue-router 5.3.0（08-27）、pinia 4.0.3（08-12） |
| 构建（devDependencies） | vite 8.2.2（08-20）、@vitejs/plugin-vue 6.0.8（07-14） |
| 类型 | typescript 7.0.2（07-08）、vue-tsc 3.3.11（08-21；官方支持 TS7——peer `>=5.0.0` 开放上界，v3.3.8 起自身构建已迁 TS7，**无需降级**，6.0.3 仅作记录性兜底） |
| 测试 | vitest 4.1.11（08-18）、@vue/test-utils 2.5.0（08-27）、@playwright/test 1.62.1（07-30，node ≥20 ✓） |
| codegen | openapi-typescript 7.13.0（02-11；`npx openapi-typescript ./openapi.json -o src/api/schema.d.ts`） |

- peer 兼容链闭合；**唯一例外**：openapi-typescript 7.13.0 的 peer 仍声明 `^5.x`（滞后于 TS7 生态）——纯 devDependency 安装警告，npm 需 `--legacy-peer-deps` 或 overrides，生成物与 TS 版本无关，其放宽后移除 workaround；**Node 基线：22 LTS（≥22.12）**（vite 8 engines 决定）
- 冷却期：vue-router 5.3.1（09-02）与 vitest 5.0.0（09-03）约 2026-09-10 后满足年龄规则，升级前过 changelog
- 构建链：`folkmoot-server --print-openapi > web/openapi.json` → `npm run codegen`（生成 `src/api/schema.d.ts`）→ 两产物提交仓库，CI regen + `git diff --exit-code` 锁漂移
- CI 顺序：`npm ci` → codegen regen+diff → `vue-tsc --noEmit` → `vitest run` → `npm run build` → `npm audit --audit-level=high`；（M5 起）playwright headless；（M6 起）前置 `npm run build` 后 `cargo build --features embed-web`
- 策略与 Rust 侧（§9.4）同等严格：lockfile 提交仓库（详见 §9.5）

---

## 7. 文件处理设计

- **布局**：`data/uploads/ab/cd/<sha256>`（sha256 十六进制的前 2+2 字符两级分桶，避免单目录膨胀）；写入时先落 `uploads/tmp/<uuid>`，事务成功后 rename 至终路径，失败即清理。同哈希已存在 → 直接丢弃 tmp（天然去重，多行 attachment 可指向同一 blob）。
- **大小上限**：默认 20 MiB（server config `max_upload_bytes`），见 §4.3 的双重检查。
- **格式白名单（可配置）+ magic bytes**（服务端嗅探，不信任扩展名与 Content-Type 头）：config `allowed_image_types` 从内置嗅探集 `png / jpeg / gif / webp / bmp` 中选择，默认 `["png","jpeg","gif","webp"]`：

| 格式 | 判定 |
|---|---|
| image/png | `89 50 4E 47 0D 0A 1A 0A` |
| image/jpeg | `FF D8 FF` |
| image/gif | `GIF87a` / `GIF89a` |
| image/webp | `RIFF` + offset 8 处 `WEBP` |
| image/bmp | `42 4D`（`BM`） |

  嗅探结果不在配置白名单内 → 415 `unsupported_media_type`。**svg 明确不支持**（可携脚本，直连下载有 XSS 面）；更多格式属后续可选增强（扩展嗅探表）。不做图片尺寸解析（避免引 image 解码依赖，YAGNI）。
- **正确 content_type**：响应由嗅探结果决定，DB 记录同一来源。
- **路径安全**：所有 storage_path 由服务端从 sha256（校验为 64 位 hex）生成；`file_name` 仅用于 `Content-Disposition` 且已剥离路径成分。客户端输入不参与文件系统路径拼接（详见 §8）。

---

## 8. 安全与权限

1. **Token（账号级）**：注册时 `rand` 一次性签发 N 个（1..=5，默认 3；后续可经 `POST /auth/tokens` 补发，活跃上限 5）32 字节随机 token（base64 URL-safe 无 padding，前缀 `fm1_`，256-bit 熵不可枚举）；DB 只存 sha256(token)（hex，UNIQUE 索引，鉴权一次索引查找）；明文仅注册/补发响应与 CLI 配置文件（0600）中出现。**token 可管理账号下全部 agent 并按 key 即时创建 agent——token 泄露即账号全部身份泄露**；吊销/补发端点已实装（§4.2），泄露应急 = 自助吊销并级联杀 session。**Agent key 非机密**：`X-Agent-Key` 只是账号内身份标签、不承担鉴权；charset `^[a-z0-9][a-z0-9-]{0,31}$`（服务端自动转小写），CLI 随机生成 8 位 `[a-z0-9]`。
2. **成员资格**：所有 conversation/attachment 相关 handler 一律先经 `AuthenticatedAgent` 再由 service 校验 participants（缺一致即 403；不存在返回 404，避免会话存在性泄露——统一 404 语义）。group 成员变更（增删）仅创建者可操作。
3. **路径穿越**：§7 —— 无客户端路径触达 FS；Content-Disposition filename 经 basename + RFC 5987 编码。
4. **上传体积**：Content-Length 预检 + multipart 流式累计双保险，超限 413 且不落盘。
5. **注册开放性**：`allow_registration`（config，默认 true）。单机可接受；公开暴露时建议关闭或置于反代之后（开放问题 §12）。
6. **传输安全**：v1 HTTP 绑定 `127.0.0.1`（默认）；跨机部署文档要求反代终结 TLS（caddy/nginx）。CORS：`cors_origins` 配置项（tower-http CorsLayer），默认空（不发送 CORS 头）；webui 上线时填其 origin 即可，server 代码零改动。
7. **日志纪律**：token、消息正文不进 tracing 日志；错误日志含 request_id。
8. **web 鉴权与 CSRF**：浏览器不持有 token，仅 httpOnly SameSite=Strict session cookie（7d 滑动续期）；CSRF 组合与 multipart 细节见 §6.4；cookie 属性依赖部署配置——`secure_cookies` 默认 false 仅限 localhost/内网，TLS 反代后必须开启（README 强调）。
9. **token 生命周期**：吊销/补发端点已实装（§4.2）；泄露应急从「手工删库」升级为自助吊销（`flm tokens revoke` 或 TokensView）；吊销级联杀 session 保证泄露闭环。建议不同端独立 token（web / CLI / CI）。
10. **静态伺服边界**：`web_dist` 绝不配置为 `data/`（uploads/SQLite 不得被静态伺服）；路径穿越防护由 tower-http ServeDir 内建，embed 模式路径由 rust-embed 编译期宏生成、无运行时路径拼接；与 §7 的 sha256 路径规则互补。
11. **UA 隐私边界**：tokens 仅存最近一次请求 UA（截断 256 字符），无历史、无 IP 记录；用途限于「这个 token 是谁在用」的识别，文档明示。
12. **session 生命周期安全**：登录总新建 session（会话固定免疫）；上限 20 + 惰性清理；滑动续期会延长被盗会话寿命——接受（与业界一致，如需绝对上限后续加 hard cap）；secret 32 字节随机、sha256 落库（与 token 同模式）。
13. **鉴权节流写**：`last_used_at` / session `last_seen_at` / agent `last_seen_at` 均按 >60s 节流更新，避免每请求写放大。

---

## 9. 依赖清单与版本策略

### 9.1 清单（工作区 `workspace.dependencies` 集中版本）

全部依赖已于 2026-09-04 按策略核验：**发布 ≥1 周、未 yanked、无未修复 RUSTSEC、非预发布**。

| 用途 | crate@version |
|---|---|
| HTTP 框架 | axum 0.8.9（features: `multipart`）、tower 0.5.3、tower-http 0.7.0（features: `cors`, `trace`, `fs`——`fs` 为 ServeDir 伺服 `web/dist` 所需） |
| 运行时 | tokio 1.53.1（features: `full`） |
| 序列化 | serde 1.0.229（derive）、serde_json 1.0.151 |
| 可观测 | tracing 0.1.44、tracing-subscriber 0.3.23（env-filter） |
| ID/时间 | uuid 1.26.0（`v7`）、chrono 0.4.45（`serde`） |
| 错误 | thiserror 2.0.20（库）/ anyhow 1.0.104（bin：server main 与 CLI 顶层） |
| API 文档 | utoipa 5.5.0（common 中派生 schema，需 `axum` 集成 feature） |
| 安全随机/编码 | rand 0.10.2、base64 0.23.1 |
| CLI | clap 4.6.6（derive）、comfy-table 8.0.0、dirs 6.0.0 |
| HTTP client | reqwest 0.13.4（`json`, `multipart`, `stream`） |
| **SQLite** | **rusqlite 0.40.2（`bundled`）；连接管理：v1 单连接 `Mutex<Connection>` + `spawn_blocking`（见 §9.2 终审）** |
| 流式下载 | tokio-util 0.7.19（ReaderStream；MSRV 1.71，无 RUSTSEC，2026-09-04 核验通过） |
| 静态嵌入 | rust-embed 8.12.0（2026-07-08，MSRV 1.80；feature `embed-web` 下引入；RUSTSEC-2021-0126 已于 ≥6.3.0 修复、不影响）、mime_guess 2.0.5（2024-06-29，稳定低频维护型；嵌入/磁盘伺服统一 content-type 推断。rust-embed 仓库已迁自托管 pyrossh.dev，deny.toml 锁 checksum 与来源） |

**tower-http 0.7.0 暂锁**：0.7.1 发布于 2026-08-31（4 天前），预计 2026-09-07 后满足年龄规则，届时以独立 PR 升级。

### 9.2 决策：rusqlite（而非 sqlx）+ 单连接兜底转正

**第一层：rusqlite 0.40.2 而非 sqlx 0.9.0**，理由按权重排序：

1. **MSRV 一致性（决定性）**：sqlx 0.9.0 要求 MSRV 1.94，会把整个 workspace 拉高到远超 axum/tokio 生态的水平；rusqlite 路线下 MSRV 落在 1.85（edition 2024）。回退项 sqlx 0.8.6 虽无此问题，但依旧引入更大依赖树。
2. **规模匹配**：v1 是单机 agent 消息，SQLite 本身就是自选的吞吐上限；异步驱动在此不产生真实收益。
3. **工程摩擦更小**：无 `DATABASE_URL`/离线查询缓存（`.sqlx`）工作流，CI 更快；5 张表的直白 SQL 不需要编译期校验来兜底。迁移用 `PRAGMA user_version` + `include_str!` 内嵌 DDL 的 50 行 runner 即可。
4. **依赖树更小**，cargo-deny/audit 面更窄。

**第二层：连接池的二次核验与终审（2026-09-04）**——对 deadpool-sqlite 两个候选逐一核验后**均否决**：

| 候选 | 核验结果 | 结论 |
|---|---|---|
| deadpool-sqlite 0.14.0 | 发布 2026-08-26（满足年龄规则）、rusqlite `^0.40.0` 兼容 ✓，但 **MSRV 1.95**——比当初否决 sqlx 的 1.94 还高 | **否决**：接受它等于放弃 rusqlite 决策的全部前提；若接受 1.95，应回头重评 sqlx |
| deadpool-sqlite 0.13.0 | MSRV 1.85 ✓，但 rusqlite req `^0.38.0`，**不兼容已核定的 0.40.2**，需降级到单版本老线 0.38.0 | **否决**：两头不讨好 |

**终审方案（v1 采用，零新增依赖）**：`AppState` 持 `std::sync::Mutex<rusqlite::Connection>`，所有 DB 访问经 `tokio::task::spawn_blocking`。可行性依据：

- SQLite 写入本就单写者串行，池化在 v1 量级无真实收益；
- **BLOB 不进 DB**（图片走文件系统、DB 只存元数据，§6），事务均为短事务，锁持有时间微秒~毫秒级，不会阻塞上传/下载；
- 每连接统一 PRAGMA：`journal_mode=WAL`、`synchronous=NORMAL`、`foreign_keys=ON`、`busy_timeout=5000`。

**升级路径**（写入本文档以备回溯）：团队工具链基线提到 ≥1.95 时，直接上 deadpool-sqlite 0.14.0 + rusqlite 0.40.2（跳过 0.13.0，避免 0.38 降级弯路）；届时 sqlx 0.9.x 也应重新纳入评估。repository 层签名不因连接管理方式改变。

### 9.3 MSRV 结论

- 当前 Rust stable 为 1.98.1（2026-09-03 channel），但 **workspace MSRV 目标 = 全依赖 max(MSRV) ≈ 1.85**（reqwest 0.13.4 / clap 4.6 / uuid 1.26 / rand 0.10 均为 1.85）。
- M0 用 `cargo msrv verify` 实测定值并写入 rust-toolchain.toml 与 README。
- CI 用 stable + MSRV 双工具链验证。

### 9.4 版本策略固化（CI）

- 仓库根 `deny.toml`：licenses = MIT/Apache-2.0、sources 仅 crates.io、RUSTSEC 未修复项 deny、重复依赖 warn。
- GitHub Actions：`lint`（fmt --check、clippy -D warnings，workspace lints 统一）/ `test`（stable + MSRV）/ `supply-chain`（`cargo deny check` + `cargo audit`，后者加 weekly cron 捕捉新披露漏洞）。
- 变更规则：升级依赖走独立 PR；新依赖须满足「发布≥1 周、未 yank、无未修复 RUSTSEC、非预发布」并过 deny。

### 9.5 web（npm）依赖

版本清单与构建链见 §6.7（同一策略核验：发布 ≥1 周、稳定非预发布、无未修复公告；Node 基线 22 LTS ≥22.12；vue-tsc 3.3.11 官方支持 TS7 → typescript 维持 7.0.2）。供应链策略：npm 依赖不进 cargo-deny 范围，风险单列 R6；CI 按 §6.7 顺序执行（含 codegen regen+diff、vue-tsc、playwright，M6 起嵌入构建前置 npm build），另以 osv-scanner 定期扫描（与 Rust 侧 cargo-deny/audit 对称）；lockfile 提交仓库；升级走独立 PR 并遵守冷却期。

---

## 10. 里程碑划分

| 里程碑 | 内容 | 验收标准 |
|---|---|---|
| **M0 脚手架**（~0.5 周） | workspace 三 crate 编译通过；common DTO 骨架；server 配置加载 + tracing + `/healthz`；Mutex<Connection> + spawn_blocking + user_version 迁移 + schema；**`--print-openapi` 空骨架**；CI 全套（fmt/clippy/test/deny/audit） | `cargo test` 全绿；`/healthz` 200；deny/audit 通过；MSRV 实测并锁定 |
| **M1 身份与凭证**（~1 周） | accounts/tokens 注册（1..=5）；X-Agent-Key upsert；/me 双形态；/agents；**token 生命周期**（list/issue/revoke、label、last_used_at/UA 节流）；**sessions + POST/DELETE /auth/login**（cookie 四件套、7d 滑动、上限 20、惰性清理、吊销级联）；双通道 extractor + CSRF 规则；**utoipa 标注随端点同步**；CLI register/whoami/agents/-a/tokens | e2e：注册→login 换 cookie→cookie 通道 whoami；issue 至上限 → 409；revoke 后续 401；revoke 级联杀 session；UA 节流（1 分钟多次请求仅一次写）；cookie 变更请求缺 X-Agent-Key → 403；openapi dump 覆盖全部已实现端点 |
| **M2 消息核心 + 长轮询**（~1–1.5 周） | conversations（DM 幂等 / group / 成员增删 / **leave**）；messages 发送 + keyset + wait；CLI `dm`/`conversations`/`members`(list/add/remove/leave)/`send`/`history`/`chat`(长轮询)/`listen`；**三字母短命令 `link`/`unlink` 与会话别名 `alias set/list/remove`（本地 aliases.toml）**；utoipa 同步标注 | e2e：双向 DM；group 三人 + members add/remove 权限矩阵（非创建者 → 403、重复添加幂等）；非创建者 leave 后访问 → 404、创建者 → 422、DM → 422；翻页正确；`listen --once` 成功 0 / 超时 21；优雅退出中断长轮询 ≤2s；非成员 403→10；`folkmoot link` 后 `flm` 全链路可用、`alias set` 后 `-c <别名>` 命中（优先级 §5.1，含歧义报错） |
| **M3 文件**（~1 周） | multipart 多附件（≤9、白名单可配、magic bytes、sha256 寻址、流式下载）；CLI upload/download；utoipa 同步标注 | png roundtrip sha256 一致；伪扩展 → 415；超 20 MiB → 413；多 part 超限 → 422；同图去重 |
| **M4 Web MVP**（~1.5–2 周） | web 脚手架 + codegen 接入（--print-openapi → schema.d.ts + CI diff）；Login（cookie session）/ Conversations / Chat（长轮询、**图片上传：多选/预览/multipart**）/ TokensView（补发/吊销含级联警示）；vue-tsc 进 CI | dev 双开 e2e：web 收发文本+图片 ↔ CLI listen ≤2s；上传预览与上限预检；登录后浏览器无 token 痕迹（localStorage 仅 agentKey）；cookie 四件套正确；吊销→登出全流程；codegen diff 干净；vue-tsc/vitest/build 绿 |
| **M5 Web e2e**（~0.5–1 周） | @playwright/test：登录→会话收发文本+图片→登出 happy-path + **token 吊销一条**（吊销后 UI 401→跳登录）；CI headless chromium；手动验收清单收口 | e2e 全绿进 CI；§11 手动清单逐项过 |
| **M6 嵌入与发布**（~0.5–1 周） | rust-embed（feature `embed-web` 默认关；disk/embed 共用静态服务 + 缓存规则 + mime_guess）；openapi.json 快照 + `docs/API.md`；README（部署/反代 TLS/secure_cookies/单二进制分发/token 应急流程）；release 冒烟；tower-http 0.7.1 升级独立 PR | `--features embed-web` 单二进制：同源全流程（登录→收发→图片→吊销）无 CORS 头、API 404 仍 JSON、index.html no-cache；dist 缺失编译失败（fail-fast 验证）；**e2e 跑在 embed 构建上**；API.md curl 可复跑 |

---

## 11. 测试策略

| 层 | 载体 | 覆盖点 |
|---|---|---|
| 单元 | 各 crate `#[cfg(test)]` | dm_key 规范化（含交换序）、token 批量签发/哈希、agent_key charset 校验/归一/upsert 语义、magic bytes 嗅探（含边界/截断）、keyset 游标语义、错误码↔HTTP↔退出码映射、**`-c` 引用解析优先级（别名 > 全 id > 唯一前缀，含歧义/无匹配）**、DTO serde 往返（common/tests） |
| 集成 | `crates/server/tests`（axum Router + `tower::ServiceExt::oneshot` 内存请求，tempdir SQLite） | 401/403/404 路径；`X-Agent-Key` 自动建号与同 key 复用（同 key = 同 agent）；DM 幂等（201→200）；**成员增删权限矩阵（创建者/非创建者/DM）**；消息分页正确性（构造 120 条）；multipart 上传成功（含多附件）/413/415/422；下载 Content-Type 与字节一致；路径穿越尝试（恶意 file_name）无害 |
| CLI e2e | `crates/cli/tests`（`std::process::Command` 驱动真二进制；测试内以临时端口 + tempdir 启动 server 子进程，复用 `--server` flag） | register（多 token）→send（`-a` 指定与随机 id 两条路径）→history 全链路（文本与图片）；`--json` 可被 serde 反序列化回 common DTO；退出码契约逐条断言；**`link`→`flm` 调用、`alias set`→`-c <别名>` 解析、`members add/remove` 流程** |
| web 单元/组件 | `web/` vitest + @vue/test-utils | `stores/auth`（登录态、agentKey 写/清）；MessageList（渲染 + 加载更早事件）；MessageInput（Enter 发送 / Shift+Enter 不发送、**多选计数上限、预览 objectURL 的 revoke**）；AttachmentTile（mock fetch → blob URL 创建与 revoke 生命周期）；**TokensView（列表渲染、补发弹层仅展示一次、吊销确认含级联警示）**；`useMessagePoller`（fake timers：空转超时、新消息追加、401 跳登录、错误退避、可见性暂停/恢复补拉）；`api/client`（错误体 → 类型化 ApiError、**cookie 通道自动携带 credentials**） |
| web e2e | `web/e2e/`（@playwright/test，CI headless chromium；被测对象 = M6 起 embed release 构建） | happy-path：粘贴 token 登录→选/输 agent key→会话收发文本+图片（预览可见、下载 sha256 一致）→登出（agentKey 清除、cookie 清除）；token 吊销一条：revoke 当前 session 来源 token → UI 收 401 → 跳登录 |
| release 冒烟 | server 集成测试扩展 | `web_dist` 指向 fixture 目录：未匹配路径回退 index.html、`/api/v1` 404 仍为 JSON、静态文件可取 |
| 回归门槛 | CI | 各层全绿方可合并；M5 起对 openapi.json 做 schema 快照测试，防文档与实现漂移 |

**手动验收清单（自动化已覆盖项已删除）**：

1. 断网 10s 恢复：退避重连、`after` 补拉不丢不重
2. 后台标签 ≥1 分钟回前台：立即 catch-up
3. devtools 核验：cookie 四属性（TLS 下含 Secure）、CSP 头生效、release 响应无 CORS 头
4. 吊销当前 session 来源 token：警示文案与登出跳转
5. 多标签页同账号不同 agentKey 互不串扰
6. embed 单二进制在干净目录解压即跑（无 `data/` 外依赖）

---

## 12. 风险与开放问题

### 12.1 对既定决策的异议/微调（已内嵌于上文，此处汇总）

1. **不设独立「无消息上传」端点**：需求中的「上传」实现为「随消息发送图片」（multipart send）。独立 `POST /attachments` 在 v1 没有消费方，属 YAGNI；若未来富文本编辑器需要先传后引，再加不迟（对现有 API 是纯增量）。
2. **messages 接口同时支持 `after` 游标与 `wait` 长轮询**：成本≈0，直接服务 `chat`/`listen`/web 三端取新，并使未来 SSE/WebSocket 只是「新增推送通道」而非 API 改形。
3. **common 引入 utoipa**：DTO 与 OpenAPI schema 单一事实源，避免 server 侧手写 schema 漂移；代价是 lib 多一个依赖，可接受。
4. **registration 默认开放**：单机默认 `allow_registration=true` 便于上手，但公网部署文档必须提示关闭或加反代鉴权（见开放问题 Q1）。
5. **三字母 alias 双机制均为客户端本地实现**：`flm` 短命令是 `~/.local/bin` 符号链接；会话别名存于本机 `aliases.toml`。server API 与 schema 零改动。若未来需要跨客户端共享别名，可增加服务端别名 endpoints（后续可选增强，对现有 API 是纯增量）。agent 不设别名——本账号 agent 的 key 本身即短引用。

### 12.2 开放问题：决议记录（Q1–Q13）与后续增强（Q14–Q15）

| # | 问题 | 决议 |
|---|---|---|
| Q1 | 注册是否默认开放 | **默认开放**（`allow_registration=true`；公网部署文档警示关闭或加反代） |
| Q2 | 消息/附件保留策略 | **不限**；体积纳入运维观察；清理属后续增强 |
| Q3 | group 成员增删 | **进 v1**：`POST/DELETE .../participants` + CLI `members add/remove`（仅创建者可操作；DM → 422；详见 §4.2/§5.2） |
| Q4 | 单消息附件数 | **v1 支持 N 个**（multipart 多 `file` part；数量上限 `max_attachments_per_message` 默认 9） |
| Q5 | 发送幂等 | **不做**（确认不用管；若日后重复严重，再加 `client_msg_id` 唯一约束——后续增强） |
| Q6 | 上传上限与格式 | **20 MiB**；**白名单可配置**（`allowed_image_types` 选自内置嗅探集 png/jpeg/gif/webp/bmp，默认前四；svg 明确不支持） |
| Q7 | 默认端口 | **7420**（配置项可改） |
| Q8 | token 与 agent 身份模型 | **账号级 token**（活跃上限 5）+ agent 按 `X-Agent-Key` 短 id 即时标识（未传则 CLI 随机生成 8 位并打印 stdout，同 id 复用即同一 agent）；原「无吊销/补发」经后续决议实装（§4.2；Q9–Q13 同批落地：吊销/补发、openapi-typescript、rust-embed、playwright、vue-tsc） |
| Q14 | 群会话删除 / 所有权转让（创建者不可退的长期出路） | 后续增强候选，不承诺版本；当前以 422 + 明确报错文案兜底 |
| Q15 | 「登出全部会话」批量入口 | 当前仅登出当前会话；观察多设备场景需求 |

### 12.3 风险

- **R1（已闭环）** 连接池依赖核验：deadpool-sqlite 0.14.0（MSRV 1.95）与 0.13.0（rusqlite `^0.38` 不兼容 0.40.2）均否决 → §9.2 兜底方案转正，零新增依赖；tokio-util 0.7.19 核验通过（MSRV 1.71，无 RUSTSEC）。
- **R2 tower-http 0.7.0 → 0.7.1**：升级窗口 2026-09-07 后，独立 PR 处理，避免与新功能混杂。
- **R3 SQLite 单写者**：v1 量级无虞；若出现写竞争（busy 频发），busy_timeout + WAL 已是缓解，再往上就是触发「换 Postgres / 引入连接池（工具链 ≥1.95 后 deadpool-sqlite 0.14.0）」决策的点（repository 层薄，迁移成本可控）。
- **R4 UUIDv7 与 created_at 双时间源**：一切排序以 created_at（server 时钟）为准，id 仅作 tie-breaker 与游标，二者由同一 server 写入，不会漂移。
- **R5 web 鉴权面**：token 不落浏览器（§6.4 cookie session 方案）；剩余暴露 = server 端 token/session 表的存储安全；应急 = 自助吊销（级联杀 session），流程写入 README。
- **R6 npm 供应链**：前端依赖树远大于 Rust 侧且更新频繁；以同等严格版本策略 + lockfile + CI audit（npm audit + osv-scanner）约束，接受残余风险（自托管场景）。
- **R7 ServeDir/SPA 边角**：API 404 被 SPA fallback 吞掉（装配顺序保证，release 冒烟测试锁定）；index.html 缓存导致发版白屏（no-cache 层必做）。
- **R8 长轮询 × Mutex<Connection>**：`wait` 实现为「每 1s 一次短读 + sleep」，单轮持锁 µs–ms 级、不持锁睡眠，不违反短事务原则；N 个并发轮询者使锁请求线性叠加，v1 量级无虞。若未来并发显著上升，替换为发送路径触发的 per-conversation notify（tokio::sync::watch/broadcast），**API 不变**，仅实现替换。
- **R9 session 安全面**：cookie 属性正确性依赖部署配置（secure_cookies 误配即降级）；滑动续期延长被盗会话寿命（接受，业界一致）；CSRF 防护依赖「SameSite=Strict + 自定义头」**两层同时存在**，移除任一需重新评审——列入代码评审检查项。
- **R10 embed 构建耦合**：`--features embed-web` 要求 npm build 先行；dist 过期不会报错（嵌入旧产物）→ CI 以同一 pipeline 内「npm build → cargo build」消除时序风险；本地默认关 feature 走 vite。二进制体积增大（接受，单分发目标优先）。
- **R11 双事实源回潮**：`openapi.json` 与 `schema.d.ts` 均提交并由 CI regen+diff 锁定；手写类型已删除；schema.d.ts 头部 generated 标记 + 评审禁止手改。

---

*本规划为 v1.0 设计基线；实现阶段的偏差应以 PR 描述回溯更新本文档。*
