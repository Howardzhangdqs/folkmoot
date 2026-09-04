import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'

describe('stores/auth', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    localStorage.clear()
    vi.restoreAllMocks()
  })

  it('登录后 localStorage 仅存 agentKey，token 不落盘', async () => {
    const loginResp = { account: { id: 'a1', name: 'zhang', created_at: '' }, agent_keys: ['ops'] }
    const meResp = { account: loginResp.account, agent: { id: 'g1', agent_key: 'ops', account_id: 'a1', display_name: null, created_at: '', last_seen_at: '' } }
    const spy = vi.fn()
      .mockResolvedValueOnce(new Response(JSON.stringify(loginResp), { status: 200 }))
      .mockResolvedValueOnce(new Response(JSON.stringify(meResp), { status: 200 }))
    vi.stubGlobal('fetch', spy)

    const { useAuthStore } = await import('../auth')
    const auth = useAuthStore()
    await auth.login('fm1_secret', 'ops')

    expect(auth.loggedIn).toBe(true)
    expect(localStorage.getItem('folkmoot.agentKey')).toBe('ops')
    // token 明文不出现在 localStorage 任何键值中
    for (let i = 0; i < localStorage.length; i++) {
      const k = localStorage.key(i)!
      expect(localStorage.getItem(k)).not.toContain('fm1_secret')
    }
    // 登录请求用 Bearer 头
    expect((spy.mock.calls[0][1].headers as Record<string, string>)['Authorization']).toBe('Bearer fm1_secret')
  })

  it('logout 清除 agentKey', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('{}', { status: 200 })))
    const { useAuthStore } = await import('../auth')
    const auth = useAuthStore()
    auth.agentKey = 'ops'
    localStorage.setItem('folkmoot.agentKey', 'ops')
    await auth.logout()
    expect(auth.agentKey).toBeNull()
    expect(localStorage.getItem('folkmoot.agentKey')).toBeNull()
  })
})
