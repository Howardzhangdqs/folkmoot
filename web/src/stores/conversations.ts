import { defineStore } from 'pinia'
import { get } from '../api/client'
import type { ConversationListResponse, ConversationSummary } from '../api/types'

// 消息数据不进 store（留在 ChatView composable，§6.2）
export const useConversationsStore = defineStore('conversations', {
  state: () => ({
    items: [] as ConversationSummary[],
    loading: false,
  }),
  actions: {
    async refresh() {
      this.loading = true
      try {
        const resp = await get<ConversationListResponse>('/conversations?limit=100')
        this.items = resp.items
      } finally {
        this.loading = false
      }
    },
  },
})
