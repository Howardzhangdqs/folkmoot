import { defineStore } from 'pinia'
import { del, get, loginWithToken, setAgentKey } from '../api/client'
import type { AccountSummary, LoginResponse, MeResponse } from '../api/types'

const AGENT_KEY_STORAGE = 'folkmoot.agentKey'

// token 仅登录请求瞬时使用，不落 localStorage（§6.4）；localStorage 仅存 agentKey
export const useAuthStore = defineStore('auth', {
  state: () => ({
    account: null as AccountSummary | null,
    agentKey: localStorage.getItem(AGENT_KEY_STORAGE) as string | null,
    ready: false,
  }),
  getters: {
    loggedIn: (s) => s.account !== null,
  },
  actions: {
    /** 两步登录：token 换 cookie → 选定 agent key → GET /me 即时 upsert */
    async login(token: string, agentKey: string) {
      await loginWithToken<LoginResponse>('/auth/login', token)
      await this.selectAgent(agentKey)
    },
    /** 选定/输入 agent key（cookie + key → upsert） */
    async selectAgent(key: string) {
      setAgentKey(key)
      const me = await get<MeResponse>('/me')
      this.account = me.account
      this.agentKey = key
      localStorage.setItem(AGENT_KEY_STORAGE, key)
    },
    /** 页面刷新后用已有 cookie + 存的 agentKey 恢复登录态 */
    async restore(): Promise<boolean> {
      if (!this.agentKey) {
        this.ready = true
        return false
      }
      try {
        setAgentKey(this.agentKey)
        const me = await get<MeResponse>('/me')
        this.account = me.account
        return true
      } catch {
        this.account = null
        return false
      } finally {
        this.ready = true
      }
    },
    async logout() {
      try {
        await del('/auth/login')
      } finally {
        this.account = null
        this.agentKey = null
        setAgentKey(null)
        localStorage.removeItem(AGENT_KEY_STORAGE)
      }
    },
  },
})
