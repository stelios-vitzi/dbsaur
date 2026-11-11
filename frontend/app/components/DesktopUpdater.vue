<script setup lang="ts">
interface UpdateStatus {
  phase: string
  version?: string
  downloaded: number
  total?: number
  message?: string
}
const status = ref<UpdateStatus>()
const dismissed = ref(false)
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
    if (status.value) error.value = 'Could not reach the updater. Reopen the app to try again.'
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
  dismissed.value = false
  void action('check')
}

onMounted(() => {
  if (window.location.origin === 'http://127.0.0.1:34618') void refresh()
})
onBeforeUnmount(() => {
  stopped = true
  clearTimeout(timer)
})
</script>

<template>
  <aside
    v-if="status && status.phase !== 'unavailable'"
    class="desktop-updater"
    aria-label="Application updates"
  >
    <template v-if="!dismissed || ['downloading', 'installing'].includes(status.phase)">
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
      <button
        v-if="['available', 'current', 'error'].includes(status.phase)"
        class="button secondary small"
        @click="dismissed = true"
      >
        Later
      </button>
    </template>
    <button
      v-if="['available', 'current', 'error'].includes(status.phase)"
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
  justify-content: flex-end;
  gap: 0.65rem;
  padding: 0.4rem 1rem;
  background: var(--surface);
  border-top: 1px solid var(--border);
  font-size: 0.8rem;
}
</style>
