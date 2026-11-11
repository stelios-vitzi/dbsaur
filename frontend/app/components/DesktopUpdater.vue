<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount } from 'vue'
import { useToast } from '../composables/useToast'
import { $fetch } from '../services/http'

interface UpdateStatus {
  phase: string
  version?: string
  downloaded: number
  total?: number
  message?: string
}
const status = ref<UpdateStatus>()
const desktop = window.location.origin === 'http://127.0.0.1:34618'
const busy = ref(false)
const error = ref('')
const { confirm } = useToast()
let timer: ReturnType<typeof setTimeout> | undefined
let stopped = false

async function refresh() {
  try {
    status.value = await $fetch<UpdateStatus>('/api/desktop/update')
  } catch {
    // The local desktop API may have stopped.
    error.value = 'Could not reach the updater. Reopen the app to try again.'
    return
  }
  if (!stopped && ['checking', 'downloading', 'installing'].includes(status.value.phase))
    timer = setTimeout(refresh, 750)
}

async function action(kind: 'check' | 'install') {
  if (busy.value) return
  busy.value = true
  try {
    if (
      kind === 'install' &&
      !(await confirm('Install the update and restart DBsaur? Save your work before continuing.'))
    )
      return
    error.value = ''
    await $fetch(`/api/desktop/update/${kind}`, { method: 'POST' })
    await refresh()
  } catch {
    error.value = 'Could not start the update action. Please try again.'
  } finally {
    busy.value = false
  }
}

function checkAgain() {
  void action('check')
}

onMounted(() => {
  if (desktop) void refresh()
})
onBeforeUnmount(() => {
  stopped = true
  clearTimeout(timer)
})
</script>

<template>
  <aside class="desktop-updater" aria-label="Application updates">
    <template v-if="status && status.phase !== 'unavailable'">
      <span role="status" aria-live="polite">
        <template v-if="status.phase === 'available'"
          >DBsaur {{ status.version }} is available.</template
        >
        <template v-else-if="status.phase === 'checking'">Checking for updates…</template>
        <template v-else-if="status.phase === 'downloading'">
          Downloading update…
          <template v-if="status.total">
            {{ Math.min(100, Math.round((status.downloaded / status.total) * 100)) }}%
          </template>
        </template>
        <template v-else-if="status.phase === 'installing'"
          >Installing update. DBsaur will restart…</template
        >
        <template v-else-if="status.phase === 'current'">DBsaur is up to date.</template>
        <template v-else>{{ status.message }}</template>
      </span>
      <button
        v-if="status.phase === 'available'"
        class="button primary small"
        :disabled="busy"
        @click="action('install')"
      >
        Install and restart
      </button>
    </template>
    <p v-else-if="!error" role="status">
      {{
        status?.message ||
        (desktop ? 'Loading update status…' : 'Updates are available in installed desktop builds.')
      }}
    </p>
    <button
      v-if="status && ['idle', 'available', 'current', 'error'].includes(status.phase)"
      class="text-button"
      :disabled="busy"
      @click="checkAgain"
    >
      Check for updates
    </button>
    <span v-if="error" role="alert">{{ error }}</span>
  </aside>
</template>

<style scoped>
.desktop-updater {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: flex-start;
  gap: 0.65rem;
  font-size: 0.8rem;
}
</style>
