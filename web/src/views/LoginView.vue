<script setup lang="ts">
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { useAuthStore } from '../stores/auth'
import { loginWithToken } from '../api/client'
import type { LoginResponse } from '../api/types'

const auth = useAuthStore()
const router = useRouter()

const token = ref('')
const step = ref<'token' | 'key'>('token')
const agentKeys = ref<string[]>([])
const keyChoice = ref('')
const newKey = ref('')
const error = ref('')
const busy = ref(false)

async function submitToken() {
  error.value = ''
  busy.value = true
  try {
    // token 仅此请求瞬时使用，之后即弃（§6.4）
    const resp = await loginWithToken<LoginResponse>('/auth/login', token.value.trim())
    token.value = ''
    agentKeys.value = resp.agent_keys
    if (resp.agent_keys.length > 0) keyChoice.value = resp.agent_keys[0]
    step.value = 'key'
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    busy.value = false
  }
}

async function submitKey() {
  error.value = ''
  const key = (newKey.value.trim() || keyChoice.value).trim()
  if (!key) {
    error.value = '请选择或输入 agent key'
    return
  }
  busy.value = true
  try {
    await auth.selectAgent(key)
    router.push('/')
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <div class="login card">
    <h1>folkmoot</h1>
    <template v-if="step === 'token'">
      <p>粘贴账号 token 登录（将换发为 httpOnly 会话，浏览器不保存 token）。</p>
      <form @submit.prevent="submitToken">
        <input
          v-model="token"
          type="password"
          placeholder="fm1_..."
          autocomplete="off"
          required
        />
        <button :disabled="busy">登录</button>
      </form>
    </template>
    <template v-else>
      <p>选择本次使用的 agent 身份：</p>
      <form @submit.prevent="submitKey">
        <div v-if="agentKeys.length" class="keys">
          <label v-for="k in agentKeys" :key="k">
            <input type="radio" v-model="keyChoice" :value="k" /> {{ k }}
          </label>
        </div>
        <input v-model="newKey" placeholder="或输入新的 agent key" autocomplete="off" />
        <button :disabled="busy">进入</button>
      </form>
    </template>
    <p v-if="error" class="err">{{ error }}</p>
  </div>
</template>

<style scoped>
.login { max-width: 420px; margin: 10vh auto; }
h1 { margin-top: 0; }
form { display: flex; flex-direction: column; gap: 10px; }
input { padding: 8px 10px; border: 1px solid #d1d5db; border-radius: 6px; }
button { padding: 8px; background: #2563eb; color: #fff; border: none; border-radius: 6px; }
.keys { display: flex; flex-direction: column; gap: 6px; }
.keys label { display: flex; gap: 6px; align-items: center; }
</style>
