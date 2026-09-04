<script setup lang="ts">
import { onMounted, onBeforeUnmount } from 'vue'
import { useConversationsStore } from '../stores/conversations'
import type { ConversationSummary } from '../api/types'

const store = useConversationsStore()

function onFocus() {
  store.refresh()
}

onMounted(() => {
  store.refresh()
  window.addEventListener('focus', onFocus)
})
onBeforeUnmount(() => window.removeEventListener('focus', onFocus))

function title(c: ConversationSummary): string {
  if (c.kind === 'group') return c.title || '(group)'
  const others = c.members.map((m) => m.agent_key ?? m.agent_id.slice(0, 8))
  return `dm · ${others.join(' ↔ ')}`
}

function rel(iso: string): string {
  const s = (Date.now() - new Date(iso).getTime()) / 1000
  if (s < 60) return 'just now'
  if (s < 3600) return `${Math.floor(s / 60)}m ago`
  if (s < 86400) return `${Math.floor(s / 3600)}h ago`
  return `${Math.floor(s / 86400)}d ago`
}
</script>

<template>
  <div>
    <h2>会话</h2>
    <p v-if="store.loading && !store.items.length">加载中…</p>
    <p v-else-if="!store.items.length">还没有会话。用 CLI 发起 DM：<code>flm dm &lt;key&gt;</code></p>
    <router-link
      v-for="c in store.items"
      :key="c.id"
      :to="`/c/${c.id}`"
      class="conv card"
    >
      <div class="t">{{ title(c) }}</div>
      <div class="m">
        {{ c.members.length }} 位成员 · {{ rel(c.last_message_at) }}
      </div>
    </router-link>
  </div>
</template>

<style scoped>
.conv { display: block; margin-bottom: 8px; text-decoration: none; color: inherit; }
.conv:hover { border-color: #93c5fd; }
.t { font-weight: 600; }
.m { color: #6b7280; font-size: 13px; margin-top: 4px; }
</style>
