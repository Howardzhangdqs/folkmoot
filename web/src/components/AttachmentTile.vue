<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { fetchBlob } from '../api/client'
import type { AttachmentInfo } from '../api/types'

const props = defineProps<{ attachment: AttachmentInfo }>()

const url = ref<string | null>(null)
const error = ref(false)

onMounted(async () => {
  try {
    // authenticated fetch 取 Blob → objectURL 预览（<img> 无法带鉴权头，§6.3）
    const blob = await fetchBlob(`/attachments/${props.attachment.id}/download`)
    url.value = URL.createObjectURL(blob)
  } catch {
    error.value = true
  }
})

onBeforeUnmount(() => {
  if (url.value) URL.revokeObjectURL(url.value)
})

async function download() {
  if (!url.value) return
  const a = document.createElement('a')
  a.href = url.value
  a.download = props.attachment.file_name
  a.click()
}
</script>

<template>
  <div class="tile">
    <img v-if="url" :src="url" :alt="attachment.file_name" @click="download" />
    <span v-else-if="error" class="err">加载失败</span>
    <span v-else>…</span>
    <button class="dl" @click="download" :disabled="!url">⬇ {{ attachment.file_name }}</button>
  </div>
</template>

<style scoped>
.tile { display: flex; flex-direction: column; gap: 4px; }
img { max-height: 160px; max-width: 240px; border-radius: 6px; border: 1px solid #e3e5e8; cursor: pointer; }
.dl { background: none; border: none; color: #2563eb; font-size: 12px; text-align: left; padding: 0; }
.err { color: #b91c1c; }
</style>
