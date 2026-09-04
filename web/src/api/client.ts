// 原生 fetch 封装（§6.2）：统一 cookie 通道 credentials + X-Agent-Key + 错误体映射

import type { ApiErrorBody, ErrorCode } from './types'

export class ApiError extends Error {
  constructor(
    public code: ErrorCode,
    message: string,
    public status: number,
  ) {
    super(message)
  }
}

let agentKey: string | null = null

export function setAgentKey(key: string | null) {
  agentKey = key
}

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const headers = new Headers(init.headers)
  if (agentKey) headers.set('X-Agent-Key', agentKey)
  const resp = await fetch(`/api/v1${path}`, {
    credentials: 'same-origin', // cookie 通道
    ...init,
    headers,
  })
  if (resp.status === 204) return undefined as T
  const text = await resp.text()
  const body = text ? JSON.parse(text) : null
  if (!resp.ok) {
    const err = body as ApiErrorBody | null
    throw new ApiError(err?.error.code ?? 'internal', err?.error.message ?? `http ${resp.status}`, resp.status)
  }
  return body as T
}

export function get<T>(path: string): Promise<T> {
  return request<T>(path)
}

export function postJson<T>(path: string, body: unknown): Promise<T> {
  return request<T>(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
}

export function del<T>(path: string): Promise<T> {
  return request<T>(path, { method: 'DELETE' })
}

export function postMultipart<T>(path: string, form: FormData): Promise<T> {
  return request<T>(path, { method: 'POST', body: form })
}

/** 附件下载为 Blob（<img> 无法携带鉴权头，§6.3 AttachmentTile） */
export async function fetchBlob(path: string): Promise<Blob> {
  const headers = new Headers()
  if (agentKey) headers.set('X-Agent-Key', agentKey)
  const resp = await fetch(`/api/v1${path}`, { credentials: 'same-origin', headers })
  if (!resp.ok) {
    const body = (await resp.json().catch(() => null)) as ApiErrorBody | null
    throw new ApiError(body?.error.code ?? 'internal', body?.error.message ?? `http ${resp.status}`, resp.status)
  }
  return resp.blob()
}

/** 登录专用：粘贴的 token 走 Bearer（仅此请求持有 token，§6.4） */
export async function loginWithToken<T>(path: string, token: string): Promise<T> {
  const resp = await fetch(`/api/v1${path}`, {
    method: 'POST',
    credentials: 'same-origin',
    headers: { Authorization: `Bearer ${token}` },
  })
  const text = await resp.text()
  const body = text ? JSON.parse(text) : null
  if (!resp.ok) {
    const err = body as ApiErrorBody | null
    throw new ApiError(err?.error.code ?? 'internal', err?.error.message ?? `http ${resp.status}`, resp.status)
  }
  return body as T
}
