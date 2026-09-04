<script setup lang="ts">
import { useAuthStore } from './stores/auth'
import { useRouter } from 'vue-router'

const auth = useAuthStore()
const router = useRouter()

async function onLogout() {
  await auth.logout()
  router.push('/login')
}
</script>

<template>
  <div class="shell">
    <header v-if="auth.loggedIn" class="topbar">
      <router-link to="/" class="brand">folkmoot</router-link>
      <span class="who">{{ auth.account?.name }} / {{ auth.agentKey }}</span>
      <nav>
        <router-link to="/tokens">tokens</router-link>
        <a href="#" @click.prevent="onLogout">logout</a>
      </nav>
    </header>
    <main>
      <router-view />
    </main>
  </div>
</template>

<style>
* { box-sizing: border-box; }
body { margin: 0; font-family: ui-sans-serif, system-ui, sans-serif; background: #f6f7f9; color: #1a1d21; }
.shell { max-width: 880px; margin: 0 auto; min-height: 100vh; display: flex; flex-direction: column; }
.topbar { display: flex; align-items: center; gap: 12px; padding: 10px 16px; background: #fff; border-bottom: 1px solid #e3e5e8; }
.brand { font-weight: 700; color: #1a1d21; text-decoration: none; }
.who { color: #6b7280; font-size: 13px; flex: 1; }
.topbar nav { display: flex; gap: 12px; }
.topbar nav a { color: #2563eb; text-decoration: none; font-size: 14px; }
main { flex: 1; display: flex; flex-direction: column; padding: 16px; }
button { font: inherit; cursor: pointer; }
input, textarea { font: inherit; }
.card { background: #fff; border: 1px solid #e3e5e8; border-radius: 8px; padding: 16px; }
.err { color: #b91c1c; font-size: 13px; }
</style>
