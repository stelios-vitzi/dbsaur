import { provide } from 'vue'
import { createApp } from 'vue'
import App from './app.vue'
import { router } from './router'
import { createToastState, toastKey } from './composables/useToast'
import 'maplibre-gl/dist/maplibre-gl.css'
import './assets/main.css'

const app = createApp(App)
app.provide(toastKey, createToastState())
app.use(router)
void router.isReady().then(() => app.mount('#app'))
