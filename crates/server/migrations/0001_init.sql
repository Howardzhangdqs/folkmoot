-- schema v1（§3.3）
CREATE TABLE accounts (
  id         TEXT PRIMARY KEY,
  name       TEXT NOT NULL UNIQUE COLLATE NOCASE,
  created_at TEXT NOT NULL
);

CREATE TABLE tokens (
  id               TEXT PRIMARY KEY,
  account_id       TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  label            TEXT NOT NULL DEFAULT '',
  token_hash       TEXT NOT NULL UNIQUE,
  created_at       TEXT NOT NULL,
  last_used_at     TEXT,
  last_user_agent  TEXT
);
CREATE INDEX idx_tokens_account ON tokens(account_id);

CREATE TABLE sessions (
  id                  TEXT PRIMARY KEY,
  account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  session_hash        TEXT NOT NULL UNIQUE,
  created_by_token_id TEXT NOT NULL REFERENCES tokens(id) ON DELETE CASCADE,
  created_at          TEXT NOT NULL,
  expires_at          TEXT NOT NULL,
  last_seen_at        TEXT NOT NULL
);
CREATE INDEX idx_sessions_account ON sessions(account_id);
CREATE INDEX idx_sessions_expiry  ON sessions(expires_at);

CREATE TABLE agents (
  id           TEXT PRIMARY KEY,
  account_id   TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  agent_key    TEXT NOT NULL,
  display_name TEXT,
  created_at   TEXT NOT NULL,
  last_seen_at TEXT NOT NULL,
  UNIQUE (account_id, agent_key)
);

CREATE TABLE conversations (
  id              TEXT PRIMARY KEY,
  kind            TEXT NOT NULL CHECK (kind IN ('dm','group')),
  title           TEXT,
  dm_key          TEXT UNIQUE,
  created_by      TEXT NOT NULL REFERENCES agents(id),
  created_at      TEXT NOT NULL,
  last_message_at TEXT NOT NULL
);

CREATE TABLE participants (
  conversation_id      TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  agent_id             TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
  joined_at            TEXT NOT NULL,
  last_read_message_id TEXT,
  PRIMARY KEY (conversation_id, agent_id)
);
CREATE INDEX idx_participants_agent ON participants(agent_id);

CREATE TABLE messages (
  id              TEXT PRIMARY KEY,
  conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  sender_id       TEXT NOT NULL REFERENCES agents(id),
  text            TEXT,
  created_at      TEXT NOT NULL
);
CREATE INDEX idx_messages_conv_time ON messages(conversation_id, created_at, id);

CREATE TABLE attachments (
  id           TEXT PRIMARY KEY,
  message_id   TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
  file_name    TEXT NOT NULL,
  content_type TEXT NOT NULL,
  size_bytes   INTEGER NOT NULL,
  sha256       TEXT NOT NULL,
  storage_path TEXT NOT NULL,
  created_at   TEXT NOT NULL
);
CREATE INDEX idx_attachments_message ON attachments(message_id);
