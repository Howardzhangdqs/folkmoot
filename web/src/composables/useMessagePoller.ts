// useMessagePoller 状态机（§6.6）：
// 循环 GET messages?after=<lastId>&wait=25 → 追加、推进游标；
// 网络错误指数退避 1s→…→30s；401 抛给调用方（跳登录）；
// visibilitychange 隐藏时中止、恢复可见时立即 catch-up（不丢不重）。

import { onBeforeUnmount, ref } from 'vue'
import { ApiError, get } from '../api/client'
import type { Message, MessagePage } from '../api/types'

export function useMessagePoller(
  convId: string,
  onMessages: (msgs: Message[]) => void,
  onUnauthorized: () => void,
) {
  const lastId = ref<string | null>(null)
  const running = ref(false)
  let stopped = false
  let backoffMs = 1000
  let timer: ReturnType<typeof setTimeout> | null = null
  let abort: AbortController | null = null

  async function tick() {
    if (stopped || document.hidden) return
    try {
      const path = lastId.value
        ? `/conversations/${convId}/messages?after=${lastId.value}&wait=25&limit=50`
        : `/conversations/${convId}/messages?limit=50`
      const page = await get<MessagePage>(path)
      backoffMs = 1000
      if (page.items.length > 0) {
        onMessages(page.items)
        lastId.value = page.items[page.items.length - 1].id
        timer = setTimeout(tick, 0)
      } else {
        timer = setTimeout(tick, 0) // 服务端已 wait=25，立即续上下一轮
      }
    } catch (e) {
      if (e instanceof ApiError && e.status === 401) {
        stop()
        onUnauthorized()
        return
      }
      // 网络错误：指数退避后重试
      timer = setTimeout(tick, backoffMs)
      backoffMs = Math.min(backoffMs * 2, 30_000)
    }
  }

  function onVisibility() {
    if (!document.hidden && !stopped) {
      // 恢复可见：立即以现有游标 catch-up
      if (timer) clearTimeout(timer)
      void tick()
    } else if (timer) {
      clearTimeout(timer)
      timer = null
    }
  }

  function start(startAfter: string | null) {
    lastId.value = startAfter
    stopped = false
    running.value = true
    document.addEventListener('visibilitychange', onVisibility)
    void tick()
  }

  function stop() {
    stopped = true
    running.value = false
    if (timer) clearTimeout(timer)
    abort?.abort()
    document.removeEventListener('visibilitychange', onVisibility)
  }

  onBeforeUnmount(stop)

  return { start, stop, running }
}
