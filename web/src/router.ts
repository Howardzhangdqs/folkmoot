import { createRouter, createWebHistory } from 'vue-router'
import { useAuthStore } from './stores/auth'
import LoginView from './views/LoginView.vue'
import ConversationsView from './views/ConversationsView.vue'
import ChatView from './views/ChatView.vue'
import TokensView from './views/TokensView.vue'

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/login', component: LoginView },
    { path: '/', component: ConversationsView },
    { path: '/c/:id', component: ChatView, props: true },
    { path: '/tokens', component: TokensView },
  ],
})

router.beforeEach(async (to) => {
  const auth = useAuthStore()
  if (!auth.ready) await auth.restore()
  if (!auth.loggedIn && to.path !== '/login') return '/login'
  if (auth.loggedIn && to.path === '/login') return '/'
})
