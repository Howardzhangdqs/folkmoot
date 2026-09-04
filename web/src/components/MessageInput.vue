<script setup lang="ts">
import { onBeforeUnmount, ref } from 'vue'
import { ApiError, postMultipart } from '../api/client'
import type { Message } from '../api/types'

const props = defineProps<{ convId: string }>()
const emit = defineEmits<{ sent: [Message] }>()

const MAX_ATTACHMENTS = 9 // 与 server max_attachments_per_message 默认对齐
const text = ref('')
const files = ref<{ file: File; url: string }[]>([])
const sending = ref(false)
const error = ref('')

function pick(e: Event) {
  const input = e.target as HTMLInputElement
  const selected = Array.from(input.files ?? [])
  const room = MAX_ATTACHMENTS - files.value.length
  if (selected.length > room) {
    error.value = `最多 ${MAX_ATTACHMENTS} 个附件`
  } else {
    error.value = ''
  }
  for (const f of selected.slice(0, room)) {
    files.value.push({ file: f, url: URL.createObjectURL(f) })
  }
  input.value = ''
}

function remove(i: number) {
  URL.revokeObjectURL(files.value[i].url)
  files.value.splice(i, 1)
}

async function send() {
  const t = text.value.trim()
  if ((!t && !files.value.length) || sending.value) return
  sending.value = true
  error.value = ''
  try {
    const form = new FormData()
    if (t) form.set('text', t)
    for (const f of files.value) form.append('file', f.file, f.file.name)
    const msg = await postMultipart<Message>(`/conversations/${props.convId}/messages`, form)
    emit('sent', msg)
    text.value = ''
    for (const f of files.value) URL.revokeObjectURL(f.url)
    files.value = []
  } catch (e) {
    // 413/415/422 就地提示（§6.3）
    error.value = e instanceof ApiError ? `${e.code}: ${e.message}` : String(e)
  } finally {
    sending.value = false
  }
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault()
    void send()
  }
}

onBeforeUnmount(() => {
  for (const f of files.value) URL.revokeObjectURL(f.url)
})
</script>

<template>
  <div class="input card">
    <div v-if="files.length" class="previews">
      <div v-for="(f, i) in files" :key="f.url" class="pv">
        <img :src="f.url" :alt="f.file.name" />
        <button class="rm" @click="remove(i)" title="移除">×</button>
      </div>
    </div>
    <div class="row">
      <textarea
        v-model="text"
        rows="2"
        placeholder="Enter 发送，Shift+Enter 换行"
        @keydown="onKeydown"
      />
      <label class="pick" title="添加图片">
        📎
        <input type="file" multiple accept="image/png,image/jpeg,image/gif,image/webp" hidden @change="pick" />
      </label>
      <button :disabled="sending" @click="send">{{ sending ? '发送中…' : '发送' }}</button>
    </div>
    <p v-if="error" class="err">{{ error }}</p>
  </div>
</template>

<style scoped>
.previews { display: flex; gap: 8px; margin-bottom: 8px; flex-wrap: wrap; }
.pv { position: relative; }
.pv img { height: 56px; border-radius: 6px; border: 1px solid #e3e5e8; }
.rm { position: absolute; top: -6px; right: -6px; border-radius: 50%; border: none; background: #ef4444; color: #fff; width: 18px; height: 18px; font-size: 12px; line-height: 1; }
.row { display: flex; gap: 8px; align-items: flex-end; }
textarea { flex: 1; resize: vertical; padding: 8px; border: 1px solid #d1d5db; border-radius: 6px; }
.pick { font-size: 20px; cursor: pointer; }
button { padding: 8px 14px; background: #2563eb; color: #fff; border: none; border-radius: 6px; }
button:disabled { opacity: 0.6; }
</style>
