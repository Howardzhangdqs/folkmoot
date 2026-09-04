import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

export default defineConfig({
  plugins: [vue()],
  server: {
    port: 5173,
    proxy: {
      // dev：相对路径经 vite 代理到 server，dev/release 代码零差异（§6.5）
      '/api': 'http://127.0.0.1:7420',
      '/healthz': 'http://127.0.0.1:7420',
    },
  },
})
