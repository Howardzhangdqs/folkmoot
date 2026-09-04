import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError } from '../client'
import type { ApiErrorBody } from '../types'

// fetch mock 助手
function mockFetch(status: number, body: unknown) {
  return vi.fn().mockResolvedValue(
    new Response(JSON.stringify(body), {
      status,
      headers: { 'Content-Type': 'application/json' },
    }),
  )
}

describe('api/client', () => {
  beforeEach(() => vi.restoreAllMocks())

  it('错误体映射为类型化 ApiError（§4.5）', async () => {
    const err: ApiErrorBody = { error: { code: 'not_found', message: 'conversation not found' } }
    vi.stubGlobal('fetch', mockFetch(404, err))
    const { get } = await import('../client')
    await expect(get('/conversations/x')).rejects.toMatchObject({
      code: 'not_found',
      status: 404,
    })
  })

  it('请求自动携带 credentials 与 X-Agent-Key', async () => {
    const spy = mockFetch(200, { account: {}, agent: {} })
    vi.stubGlobal('fetch', spy)
    const { get, setAgentKey } = await import('../client')
    setAgentKey('ops')
    await get('/me')
    const [url, init] = spy.mock.calls[0]
    expect(url).toBe('/api/v1/me')
    expect(init.credentials).toBe('same-origin')
    expect((init.headers as Headers).get('X-Agent-Key')).toBe('ops')
    setAgentKey(null)
  })
})
