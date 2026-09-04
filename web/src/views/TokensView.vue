<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { del, get, postJson } from '../api/client'
import type { IssuedToken, TokenInfo } from '../api/types'

const tokens = ref<TokenInfo[]>([])
const label = ref('')
const error = ref('')
const issued = ref<IssuedToken | null>(null) // 明文仅弹层展示一次（§6.3）
const copied = ref(false)

async function refresh() {
  tokens.value = await get<TokenInfo[]>('/auth/tokens')
}

onMounted(() => refresh().catch((e) => (error.value = String(e?.message ?? e))))

async function issue() {
  error.value = ''
  try {
    issued.value = await postJson<IssuedToken>('/auth/tokens', { label: label.value })
    label.value = ''
    copied.value = false
    await refresh()
  } catch (e: any) {
    error.value = e?.message ?? String(e)
  }
}

async function copyIssued() {
  if (issued.value) {
    await navigator.clipboard.writeText(issued.value.token)
    copied.value = true
  }
}

async function revoke(t: TokenInfo) {
  const note = t.current
    ? '这是当前会话的来源 token：吊销后当前会话将同时登出。继续？'
    : `吊销 token ${t.id.slice(0, 8)}？其换发的所有 session 将级联失效。`
  if (!confirm(note)) return
  error.value = ''
  try {
    await del(`/auth/tokens/${t.id}`)
    if (t.current) {
      // 级联已杀当前 session，跳登录由下次 401 触发；这里直接刷新
      location.href = '/login'
      return
    }
    await refresh()
  } catch (e: any) {
    error.value = e?.message ?? String(e)
  }
}

function fmt(iso: string | null): string {
  return iso ? new Date(iso).toLocaleString() : 'never'
}
</script>

<template>
  <div>
    <h2>凭证管理</h2>
    <div class="card">
      <table>
        <thead>
          <tr><th>label</th><th>created</th><th>last used</th><th>user agent</th><th></th></tr>
        </thead>
        <tbody>
          <tr v-for="t in tokens" :key="t.id">
            <td>{{ t.label || '-' }} <em v-if="t.current">(current)</em></td>
            <td>{{ fmt(t.created_at) }}</td>
            <td>{{ fmt(t.last_used_at) }}</td>
            <td class="ua">{{ (t.last_user_agent ?? '-').slice(0, 40) }}</td>
            <td><button class="danger" @click="revoke(t)">吊销</button></td>
          </tr>
        </tbody>
      </table>
      <form class="issue" @submit.prevent="issue">
        <input v-model="label" placeholder="label（如 web / ci）" />
        <button :disabled="tokens.length >= 5">补发 token</button>
      </form>
      <p v-if="tokens.length >= 5" class="hint">已达活跃上限 5 个</p>
      <p v-if="error" class="err">{{ error }}</p>
    </div>

    <div v-if="issued" class="overlay" @click.self="issued = null">
      <div class="modal card">
        <h3>新 token（仅此一次展示）</h3>
        <code class="tok">{{ issued.token }}</code>
        <div class="row">
          <button @click="copyIssued">{{ copied ? '已复制' : '复制' }}</button>
          <button @click="issued = null">关闭</button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
table { width: 100%; border-collapse: collapse; font-size: 14px; }
th, td { text-align: left; padding: 6px 8px; border-bottom: 1px solid #eee; }
.ua { color: #6b7280; }
.danger { background: none; border: 1px solid #ef4444; color: #ef4444; border-radius: 6px; padding: 4px 10px; }
.issue { display: flex; gap: 8px; margin-top: 12px; }
.issue input { flex: 1; padding: 8px; border: 1px solid #d1d5db; border-radius: 6px; }
.issue button, .modal button { padding: 8px 14px; background: #2563eb; color: #fff; border: none; border-radius: 6px; }
.hint { color: #6b7280; font-size: 13px; }
.overlay { position: fixed; inset: 0; background: rgba(0,0,0,0.4); display: flex; align-items: center; justify-content: center; }
.modal { max-width: 520px; }
.tok { display: block; word-break: break-all; background: #f3f4f6; padding: 10px; border-radius: 6px; margin: 12px 0; }
.row { display: flex; gap: 8px; }
</style>
