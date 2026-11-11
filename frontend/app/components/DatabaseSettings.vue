<script setup lang="ts">
import {
  ref,
  triggerRef,
  shallowRef,
  computed,
  watch,
  onMounted,
  onScopeDispose,
  nextTick,
  useId,
  useTemplateRef,
} from 'vue'
import { useToast } from '../composables/useToast'
import { useActionFeedback } from '../composables/useToast'
import { $fetch } from '../services/http'
import AppIcon from './AppIcon.vue'
import DatabaseSettingField from './DatabaseSettingField.vue'

import type { DatabaseSettings, DatabaseSetting } from '../../shared/database'
const props = defineProps<{ database: string; provider: string }>()
const emit = defineEmits<{ close: [] }>()
const dialog = useTemplateRef<HTMLDialogElement>('dialog')
const result = shallowRef<DatabaseSettings | null>(null)
const loading = ref(true)
const saving = ref(false)
const error = ref('')
const toast = useToast()
watch(error, (message) => {
  if (message) toast.show(message, 'error')
})
const notice = ref('')
useActionFeedback(notice, 'success')
const filter = ref('')
const category = ref('')
const page = ref(0)
const draft = ref<Record<string, string | null>>({})
const connectionLimit = ref('-1')
const controller = new AbortController()
const id = useId()
const vendor = computed(() => (props.provider === 'postgres' ? 'PostgreSQL' : props.provider))
const tabs = [
  {
    id: 'general',
    label: 'General',
    icon: 'database',
    description: 'Database identity and connection limits.',
  },
  {
    id: 'performance',
    label: 'Performance',
    icon: 'settings',
    description: 'Memory, query planning, and parallel execution defaults.',
  },
  {
    id: 'queries',
    label: 'Queries',
    icon: 'play',
    description: 'Timeouts, transactions, and statement behavior.',
  },
  {
    id: 'locale',
    label: 'Locale',
    icon: 'grid',
    description: 'Language, time zones, and formatting defaults.',
  },
  {
    id: 'advanced',
    label: 'Advanced',
    icon: 'sort',
    description: 'Additional database defaults, grouped by PostgreSQL category.',
  },
  {
    id: 'server',
    label: 'Server',
    icon: 'lock',
    description: 'Server-managed settings and values outside your editing privileges.',
  },
] as const
type Tab = (typeof tabs)[number]['id']
const activeTab = ref<Tab>('general')
const active = computed(() => tabs.find((tab) => tab.id === activeTab.value)!)
const priority = [
  'work_mem',
  'maintenance_work_mem',
  'temp_buffers',
  'effective_cache_size',
  'max_parallel_workers_per_gather',
  'jit',
  'random_page_cost',
  'effective_io_concurrency',
  'statement_timeout',
  'lock_timeout',
  'idle_in_transaction_session_timeout',
  'idle_session_timeout',
  'default_transaction_isolation',
  'default_transaction_read_only',
  'search_path',
  'timezone',
  'datestyle',
  'intervalstyle',
  'client_encoding',
  'default_text_search_config',
]
const rank = (name: string) => {
  const index = priority.indexOf(name.toLowerCase())
  return index < 0 ? priority.length : index
}
function section(setting: DatabaseSetting): Tab {
  if (!setting.databaseScoped) return 'server'
  if (
    /locale|formatting/i.test(setting.category) ||
    /^(timezone|datestyle|intervalstyle|lc_|client_encoding|default_text_search_config)/i.test(
      setting.name,
    )
  )
    return 'locale'
  if (/Resource Usage|Query Tuning/i.test(setting.category)) return 'performance'
  if (
    /timeout|transaction|search_path/.test(setting.name) ||
    /Statement Behavior/i.test(setting.category)
  )
    return 'queries'
  return 'advanced'
}
const inTab = computed(
  () =>
    result.value?.settings
      .filter((setting) => section(setting) === activeTab.value)
      .sort((a, b) => rank(a.name) - rank(b.name)) || [],
)
const categories = computed(() => [...new Set(inTab.value.map((setting) => setting.category))])
const filtered = computed(() =>
  inTab.value.filter(
    (setting) =>
      (!category.value || setting.category === category.value) &&
      `${setting.name} ${setting.short_desc}`.toLowerCase().includes(filter.value.toLowerCase()),
  ),
)
const visible = computed(() => filtered.value.slice(page.value * 8, page.value * 8 + 8))
const groups = computed(() =>
  [...new Set(visible.value.map((setting) => setting.category))].map((name) => ({
    name,
    settings: visible.value.filter((setting) => setting.category === name),
  })),
)
const changes = computed(() =>
  Object.fromEntries(
    (result.value?.settings || [])
      .filter((setting) => setting.editable && draft.value[setting.name] !== setting.databaseValue)
      .map((setting) => [
        setting.name,
        { value: draft.value[setting.name] ?? null, expected: setting.databaseValue },
      ]),
  ),
)
const limitChanged = computed(
  () =>
    !!result.value?.canEdit &&
    connectionLimit.value !== String(result.value.properties.datconnlimit),
)
const changeCount = computed(() => Object.keys(changes.value).length + Number(limitChanged.value))
const confirmDiscard = ref(false)
const validLimit = computed(
  () =>
    /^-?\d+$/.test(connectionLimit.value) &&
    Number(connectionLimit.value) >= -1 &&
    Number(connectionLimit.value) <= 2147483647,
)
const propertyCards = computed(() => [
  ['Owner', result.value?.properties.owner],
  ['Encoding', result.value?.properties.encoding],
  ['Collation', result.value?.properties.datcollate],
  ['Character classification', result.value?.properties.datctype],
  ['Tablespace', result.value?.properties.tablespace],
  ['Connections allowed', result.value?.properties.datallowconn === true ? 'Yes' : 'No'],
])
watch([activeTab, filter, category], () => {
  page.value = 0
})
watch(activeTab, () => {
  category.value = ''
  filter.value = ''
})
function resetDraft() {
  draft.value = Object.fromEntries(
    (result.value?.settings || []).map((setting) => [setting.name, setting.databaseValue]),
  )
  connectionLimit.value = String(result.value?.properties.datconnlimit ?? -1)
  confirmDiscard.value = false
}
function updateDraft(name: string, value: string | null) {
  draft.value[name] = value
  notice.value = ''
}
function close() {
  if (saving.value) return
  if (changeCount.value) confirmDiscard.value = true
  else emit('close')
}
function failure(cause: unknown) {
  const problem = cause as { statusCode?: number; status?: number; data?: { message?: string } }
  const status = problem.statusCode || problem.status
  if (status === 404 || status === 405)
    return 'The running backend needs an update. Restart DBsaur, then reconnect to configure settings.'
  if (status === 401) return 'Your session ended. Reconnect to your database to continue.'
  return (
    problem.data?.message ||
    'Could not complete the request. Check your connection and database permissions, then try again.'
  )
}
async function load() {
  loading.value = true
  error.value = ''
  try {
    const settings = await $fetch<DatabaseSettings>('/api/database-settings', {
      signal: controller.signal,
    })
    if (settings.database !== props.database || settings.provider !== props.provider)
      throw new Error('Database changed')
    if (typeof settings.canEdit !== 'boolean') {
      error.value =
        'The running backend needs an update. Restart DBsaur, then reconnect to configure settings.'
      return
    }
    result.value = settings
    resetDraft()
  } catch (cause) {
    if (!controller.signal.aborted) error.value = failure(cause)
  } finally {
    loading.value = false
  }
}
async function save() {
  if (!changeCount.value || saving.value || !validLimit.value || changeCount.value > 50) return
  saving.value = true
  error.value = ''
  notice.value = ''
  try {
    if (
      !(await toast.confirm(
        `Unsaved settings for ${props.database}. Save to apply them, or cancel to discard them.`,
      ))
    ) {
      resetDraft()
      return
    }
    await $fetch('/api/database-settings', {
      method: 'PATCH',
      body: {
        database: props.database,
        confirmation: 'APPLY',
        changes: changes.value,
        ...(limitChanged.value
          ? {
              connectionLimit: Number(connectionLimit.value),
              expectedConnectionLimit: result.value?.properties.datconnlimit,
            }
          : {}),
      },
    })
    // A failed refresh must not offer to reapply already committed changes.
    if (result.value) {
      for (const setting of result.value.settings)
        if (setting.editable) setting.databaseValue = draft.value[setting.name] ?? null
      if (limitChanged.value) result.value.properties.datconnlimit = Number(connectionLimit.value)
      triggerRef(result)
    }
    notice.value =
      'Settings saved. Database defaults apply to new connections; role-specific overrides can take precedence.'
    await load()
  } catch (cause) {
    error.value = failure(cause)
  } finally {
    saving.value = false
  }
}
function tabKey(event: KeyboardEvent) {
  const index = tabs.findIndex((tab) => tab.id === activeTab.value)
  const next =
    event.key === 'Home'
      ? 0
      : event.key === 'End'
        ? tabs.length - 1
        : event.key === 'ArrowRight'
          ? (index + 1) % tabs.length
          : event.key === 'ArrowLeft'
            ? (index - 1 + tabs.length) % tabs.length
            : null
  if (next === null) return
  event.preventDefault()
  activeTab.value = tabs[next]!.id
  nextTick(() => document.getElementById(`${id}-${activeTab.value}`)?.focus())
}
onMounted(() => {
  dialog.value?.showModal()
  if (props.provider === 'demo') loading.value = false
  else void load()
})
onScopeDispose(() => controller.abort())
</script>
<template>
  <dialog
    ref="dialog"
    class="database-settings"
    aria-labelledby="settings-title"
    @cancel.prevent="close"
    @contextmenu.prevent
  >
    <div class="settings-heading">
      <span class="settings-emblem"><AppIcon name="settings" :size="24" /></span>
      <div>
        <h2 id="settings-title">Database settings</h2>
        <p>
          {{ database }} <span>· {{ vendor }}</span>
        </p>
      </div>
      <button
        class="icon-button"
        aria-label="Close database settings"
        :disabled="saving"
        @click="close"
      >
        <AppIcon name="close" />
      </button>
    </div>
    <div v-if="provider === 'demo'" class="settings-state">
      <h3>Connect to configure your database</h3>
      <p>The sample workspace has no database server settings.</p>
    </div>
    <div v-else-if="loading" class="settings-state" role="status">
      <span class="loader" />
      <p>Loading database settings…</p>
    </div>
    <template v-else-if="result">
      <div
        class="settings-tabs"
        role="tablist"
        aria-label="Database settings sections"
        @keydown="tabKey"
      >
        <button
          v-for="tab in tabs"
          :id="`${id}-${tab.id}`"
          :key="tab.id"
          role="tab"
          :aria-selected="activeTab === tab.id"
          :aria-controls="`${id}-panel`"
          :tabindex="activeTab === tab.id ? 0 : -1"
          @click="activeTab = tab.id"
        >
          <AppIcon :name="tab.icon" :size="16" />{{ tab.label }}
        </button>
      </div>
      <div class="settings-content">
        <div v-if="!result.canEdit" class="settings-alert">
          Your database user can inspect settings. Saving requires database ownership or
          administrator privileges.
        </div>
        <section :id="`${id}-panel`" role="tabpanel" :aria-labelledby="`${id}-${activeTab}`">
          <div class="settings-section-heading">
            <h3>{{ active.label }}</h3>
            <p>{{ active.description }}</p>
          </div>
          <template v-if="activeTab === 'general'">
            <div class="settings-card">
              <h4>Database identity</h4>
              <dl class="settings-identity">
                <template v-for="[label, value] in propertyCards" :key="String(label)"
                  ><dt>{{ label }}</dt>
                  <dd>{{ value ?? 'Not available' }}</dd></template
                >
              </dl>
              <p>
                Identity properties are assigned when the database is created or managed by its
                administrator.
              </p>
            </div>
            <div class="settings-card">
              <h4>Connections</h4>
              <div class="setting-field">
                <label :for="`${id}-limit`">Connection limit</label>
                <p :id="`${id}-limit-help`">
                  Maximum concurrent connections to {{ database }}. Use -1 for no limit. Existing
                  clients stay connected.
                </p>
                <input
                  :id="`${id}-limit`"
                  :value="connectionLimit"
                  @input="connectionLimit = ($event.target as HTMLInputElement).value"
                  type="number"
                  min="-1"
                  max="2147483647"
                  step="1"
                  :disabled="!result.canEdit || saving"
                  :aria-invalid="!validLimit"
                  :aria-describedby="`${id}-limit-help`"
                /><small v-if="!validLimit" class="settings-invalid"
                  >Enter a whole number between -1 and 2147483647.</small
                >
              </div>
            </div>
            <div class="settings-scope">
              <AppIcon name="database" :size="18" />
              <p>
                Changes are saved only to <strong>{{ database }}</strong>. Database defaults apply
                when clients open a new connection. Role-specific defaults can override them.
              </p>
            </div>
          </template>
          <template v-else>
            <div class="settings-filters">
              <label class="search-box"
                ><AppIcon name="search" :size="15" /><input
                  v-model="filter"
                  :placeholder="`Search ${active.label.toLowerCase()}…`"
                  :aria-label="`Search ${active.label} settings`" /></label
              ><select v-model="category" aria-label="Settings category">
                <option value="">All categories</option>
                <option v-for="name in categories" :key="name">{{ name }}</option>
              </select>
            </div>
            <p v-if="activeTab === 'server'" class="settings-scope">
              These values are managed in PostgreSQL server configuration, require additional
              privileges, or cannot be changed. Use your hosting provider or database administrator
              for server changes.
            </p>
            <div v-for="group in groups" :key="group.name" class="settings-card">
              <h4>{{ group.name }}</h4>
              <div class="settings-fields">
                <DatabaseSettingField
                  v-for="setting in group.settings"
                  :key="setting.name"
                  :setting="setting"
                  :value="draft[setting.name] ?? null"
                  :disabled="saving"
                  @update:value="updateDraft(setting.name, $event)"
                />
              </div>
            </div>
            <p v-if="!filtered.length" class="settings-state">No settings match this section.</p>
            <div v-if="filtered.length" class="settings-pagination">
              <span
                >{{ page * 8 + 1 }}–{{ Math.min((page + 1) * 8, filtered.length) }} of
                {{ filtered.length }}</span
              ><button class="button secondary small" :disabled="page === 0" @click="page--">
                Previous</button
              ><button
                class="button secondary small"
                :disabled="(page + 1) * 8 >= filtered.length"
                @click="page++"
              >
                Next
              </button>
            </div>
          </template>
        </section>
      </div>
      <div v-if="confirmDiscard" class="settings-discard" role="alert">
        <span
          >Discard {{ changeCount }} unsaved {{ changeCount === 1 ? 'change' : 'changes' }}?</span
        ><button class="button secondary small" @click="confirmDiscard = false">Keep editing</button
        ><button class="button secondary small" @click="emit('close')">Discard and close</button>
      </div>
      <footer class="settings-savebar">
        <span role="status"
          >{{
            changeCount
              ? `${changeCount} unsaved ${changeCount === 1 ? 'change' : 'changes'}`
              : 'All changes saved'
          }}<small v-if="changeCount > 50">Save at most 50 changes at a time.</small></span
        ><button
          class="button secondary small"
          :disabled="saving || !changeCount"
          @click="resetDraft"
        >
          Discard changes</button
        ><button
          class="button primary small"
          :disabled="saving || !changeCount || !validLimit || changeCount > 50"
          @click="save"
        >
          {{ saving ? 'Saving…' : 'Save changes' }}
        </button>
      </footer>
    </template>
    <div v-else class="settings-state" role="alert">
      {{ error }}<button class="button secondary small" @click="load">Try again</button>
    </div>
  </dialog>
</template>
