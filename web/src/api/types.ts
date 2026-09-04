// API 类型（M6 起由 openapi-typescript 生成 schema.d.ts 替换；当前手写对齐 common::protocol）

export interface AccountSummary {
  id: string
  name: string
  created_at: string
}

export interface AgentInfo {
  id: string
  agent_key: string | null
  account_id: string
  display_name: string | null
  created_at: string
  last_seen_at: string
}

export interface LoginResponse {
  account: AccountSummary
  agent_keys: string[]
}

export interface MeResponse {
  account: AccountSummary
  agent: AgentInfo
}

export type ConversationKind = 'dm' | 'group'

export interface ParticipantInfo {
  agent_id: string
  agent_key: string | null
  joined_at: string
}

export interface ConversationSummary {
  id: string
  kind: ConversationKind
  title: string | null
  created_by: string
  created_at: string
  last_message_at: string
  members: ParticipantInfo[]
}

export interface ConversationListResponse {
  items: ConversationSummary[]
}

export interface AttachmentInfo {
  id: string
  file_name: string
  content_type: string
  size_bytes: number
  sha256: string
}

export interface Message {
  id: string
  conversation_id: string
  sender_id: string
  sender_key: string | null
  text: string | null
  attachments: AttachmentInfo[]
  created_at: string
}

export interface MessagePage {
  items: Message[]
  next_cursor: string | null
}

export interface TokenInfo {
  id: string
  label: string
  created_at: string
  last_used_at: string | null
  last_user_agent: string | null
  current: boolean
}

export interface IssuedToken {
  id: string
  label: string
  token: string
}

export type ErrorCode =
  | 'unauthorized'
  | 'forbidden'
  | 'csrf_rejected'
  | 'not_found'
  | 'conflict'
  | 'payload_too_large'
  | 'unsupported_media_type'
  | 'validation_error'
  | 'internal'

export interface ApiErrorBody {
  error: { code: ErrorCode; message: string; details?: unknown }
}
