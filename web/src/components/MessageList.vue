<script setup lang="ts">
import { nextTick, ref, watch } from 'vue'
import type { Message } from '../api/types'
import AttachmentTile from './AttachmentTile.vue'

const props = defineProps<{ messages: Message[]; hasMore: boolean }>()
const emit = defineEmits<{ loadEarlier: [] }>()

const listEl = ref<HTMLElement | null>(null)

// 新消息到底部自动滚动
watch(
  () => props.messages.length,
  async () => {
    await nextTick()
    const el = listEl.value
    if (el) el.scrollTop = el.scrollHeight
  },
)

function sender(m: Message): string {
  return m.sender_key ?? m.sender_id.slice(0, 8)
}
function time(iso: string): string {
  return new Date(iso).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
}
</script>

<template>
  <div ref="listEl" class="list card">
    <button v-if="hasMore" class="earlier" @click="emit('loadEarlier')">加载更早</button>
    <div v-for="m in messages" :key="m.id" class="msg">
      <span class="ts">{{ time(m.created_at) }}</span>
      <span class="sender">{{ sender(m) }}</span>
      <!-- 纯文本渲染，保留换行，无 v-html（§6.3） -->
      <span v-if="m.text" class="text">{{ m.text }}</span>
      <div v-if="m.attachments.length" class="atts">
        <AttachmentTile v-for="a in m.attachments" :key="a.id" :attachment="a" />
      </div>
    </div>
    <p v-if="!messages.length" class="empty">还没有消息</p>
  </div>
</template>

<style scoped>
.list { flex: 1; overflow-y: auto; min-height: 200px; max-height: 60vh; }
.earlier { display: block; margin: 0 auto 8px; background: none; border: none; color: #2563eb; }
.msg { padding: 4px 0; display: flex; gap: 8px; align-items: baseline; flex-wrap: wrap; }
.ts { color: #9ca3af; font-size: 12px; }
.sender { font-weight: 600; font-size: 13px; }
.text { white-space: pre-wrap; word-break: break-word; }
.atts { display: flex; gap: 8px; flex-wrap: wrap; width: 100%; }
.empty { color: #9ca3af; text-align: center; }
</style>
