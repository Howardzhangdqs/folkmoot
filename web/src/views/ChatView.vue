<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { get } from '../api/client'
import { useMessagePoller } from '../composables/useMessagePoller'
import { useAuthStore } from '../stores/auth'
import MessageList from '../components/MessageList.vue'
import MessageInput from '../components/MessageInput.vue'
import type { ConversationSummary, Message, MessagePage } from '../api/types'

const props = defineProps<{ id: string }>()
const router = useRouter()
const auth = useAuthStore()

const messages = ref<Message[]>([])
const conv = ref<ConversationSummary | null>(null)
const oldestCursor = ref<string | null>(null)
const error = ref('')

function onUnauthorized() {
  auth.account = null
  router.push('/login')
}

const poller = useMessagePoller(
  props.id,
  (incoming) => {
    const seen = new Set(messages.value.map((m) => m.id))
    messages.value.push(...incoming.filter((m) => !seen.has(m.id)))
  },
  onUnauthorized,
)

async function loadEarlier() {
  if (!oldestCursor.value) return
  const page = await get<MessagePage>(
    `/conversations/${props.id}/messages?limit=50&before=${oldestCursor.value}`,
  )
  messages.value.unshift(...page.items)
  oldestCursor.value = page.next_cursor
}

onMounted(async () => {
  try {
    const [c, page] = await Promise.all([
      get<ConversationSummary>(`/conversations/${props.id}`),
      get<MessagePage>(`/conversations/${props.id}/messages?limit=50`),
    ])
    conv.value = c
    messages.value = page.items
    oldestCursor.value = page.next_cursor
    // 长轮询从最新消息之后开始
    poller.start(page.items.length ? page.items[page.items.length - 1].id : null)
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  }
})

function onSent(m: Message) {
  if (!messages.value.some((x) => x.id === m.id)) messages.value.push(m)
}
</script>

<template>
  <div class="chat">
    <div class="head card">
      <router-link to="/">← 返回</router-link>
      <strong>{{ conv?.title || conv?.kind || '' }}</strong>
      <span class="members">
        {{ conv?.members.map((m) => m.agent_key ?? m.agent_id.slice(0, 8)).join(', ') }}
      </span>
    </div>
    <p v-if="error" class="err">{{ error }}</p>
    <MessageList
      :messages="messages"
      :has-more="!!oldestCursor"
      @load-earlier="loadEarlier"
    />
    <MessageInput :conv-id="props.id" @sent="onSent" />
  </div>
</template>

<style scoped>
.chat { display: flex; flex-direction: column; flex: 1; gap: 8px; }
.head { display: flex; gap: 12px; align-items: baseline; }
.head a { color: #2563eb; text-decoration: none; }
.members { color: #6b7280; font-size: 13px; }
</style>
