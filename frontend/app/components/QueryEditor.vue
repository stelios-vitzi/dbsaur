<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted, nextTick, useTemplateRef } from 'vue'
import { useActionFeedback } from '../composables/useToast'
import AppIcon from './AppIcon.vue'
import SqlEditor from './SqlEditor.vue'

import type { TableInfo } from '../../shared/database'
const props = defineProps<{
  provider?: string
  initialSql: string
  busy: boolean
  activeQuery: string | null
  demo: boolean
  tables: TableInfo[]
  selectedTable: TableInfo | null
}>()
const emit = defineEmits<{ run: [sql: string]; activate: [id: string]; close: [id: string] }>()
interface SavedQuery {
  id: string
  name: string
  sql: string
}
interface QueryDraft {
  id: string
  number: number
  name?: string
  sql: string
  savedId: string | null
}
const storageKey = 'webdb.saved-queries.v1'
let nextNumber = 1
const drafts = ref<QueryDraft[]>([
  { id: 'query-1', number: 1, sql: props.initialSql, savedId: null },
])
const activeId = ref('query-1')
const activeDraft = computed(() => drafts.value.find((draft) => draft.id === activeId.value)!)
const sql = computed({
  get: () => activeDraft.value.sql,
  set: (value: string) => {
    activeDraft.value.sql = value
  },
})
const saved = ref<SavedQuery[]>([])
const selected = computed(() => saved.value.find((query) => query.id === activeDraft.value.savedId))
const naming = ref(false)
const name = ref('')
const renamingId = ref<string | null>(null)
const tabName = ref('')
const feedback = ref('')
useActionFeedback(feedback, 'success')
const storageError = ref('')
useActionFeedback(storageError)
const library = useTemplateRef<HTMLDetailsElement>('library')
const editor = useTemplateRef<{ focus: () => void }>('editor')
const nameInput = useTemplateRef<HTMLInputElement>('nameInput')
const workspace = useTemplateRef<HTMLElement>('workspace')
const editorHeight = ref(124)
const minEditorHeight = 64
const maxEditorHeight = ref(600)
const dragging = ref(false)
let resizeObserver: ResizeObserver | undefined
let drag: { pointerId: number; y: number; height: number } | null = null
function setEditorHeight(height: number) {
  editorHeight.value = Math.round(
    Math.min(maxEditorHeight.value, Math.max(minEditorHeight, height)),
  )
}
function updateResizeBounds() {
  const section = workspace.value
  if (!section?.parentElement) return
  const chrome = section.getBoundingClientRect().height - editorHeight.value
  // Keep room for the table title, toolbar, and usable grid rows.
  maxEditorHeight.value = Math.max(
    minEditorHeight,
    section.parentElement.clientHeight - chrome - 210,
  )
  setEditorHeight(editorHeight.value)
}
function startResize(event: PointerEvent) {
  if (event.button !== 0) return
  event.preventDefault()
  updateResizeBounds()
  const handle = event.currentTarget as HTMLElement
  handle.focus({ preventScroll: true })
  handle.setPointerCapture(event.pointerId)
  drag = { pointerId: event.pointerId, y: event.clientY, height: editorHeight.value }
  dragging.value = true
}
function moveResize(event: PointerEvent) {
  if (drag?.pointerId !== event.pointerId) return
  setEditorHeight(drag.height + event.clientY - drag.y)
}
function endResize(event: PointerEvent) {
  if (drag?.pointerId !== event.pointerId) return
  drag = null
  dragging.value = false
  const handle = event.currentTarget as HTMLElement
  if (handle.hasPointerCapture(event.pointerId)) handle.releasePointerCapture(event.pointerId)
}
function resizeWithKeyboard(event: KeyboardEvent) {
  if (!['ArrowUp', 'ArrowDown', 'Home', 'End'].includes(event.key)) return
  event.preventDefault()
  updateResizeBounds()
  const step = event.shiftKey ? 48 : 16
  setEditorHeight(
    event.key === 'Home'
      ? minEditorHeight
      : event.key === 'End'
        ? maxEditorHeight.value
        : editorHeight.value + (event.key === 'ArrowUp' ? -step : step),
  )
}
function closeLibrary(event: PointerEvent) {
  if (library.value && !library.value.contains(event.target as Node)) library.value.open = false
}
onMounted(() => {
  document.addEventListener('pointerdown', closeLibrary)
  resizeObserver = new ResizeObserver(updateResizeBounds)
  if (workspace.value?.parentElement) resizeObserver.observe(workspace.value.parentElement)
  resizeObserver.observe(workspace.value!)
  updateResizeBounds()
  try {
    const stored: unknown = JSON.parse(localStorage.getItem(storageKey) || '[]')
    if (!Array.isArray(stored) || stored.length > 100 || !stored.every(isSavedQuery)) {
      storageError.value = 'Saved queries could not be read. Browser data has not been changed.'
      return
    }
    saved.value = stored
  } catch {
    storageError.value = 'Browser storage is unavailable. Queries cannot be saved here.'
  }
})
onUnmounted(() => {
  document.removeEventListener('pointerdown', closeLibrary)
  resizeObserver?.disconnect()
})
function isSavedQuery(value: unknown): value is SavedQuery {
  if (!value || typeof value !== 'object') return false
  const query = value as Partial<SavedQuery>
  return (
    typeof query.id === 'string' &&
    typeof query.name === 'string' &&
    query.name.length > 0 &&
    query.name.length <= 80 &&
    typeof query.sql === 'string' &&
    query.sql.length <= 12_000
  )
}
watch(
  () => props.initialSql,
  (value, previous) => {
    const draft = activeDraft.value
    if (!draft.savedId && draft.sql === previous && !props.activeQuery) draft.sql = value
  },
)
watch(activeId, async () => {
  naming.value = false
  feedback.value = ''
  await nextTick()
  document
    .getElementById(`${activeId.value}-tab`)
    ?.scrollIntoView({ block: 'nearest', inline: 'nearest' })
})
function draftName(draft: QueryDraft) {
  const query = saved.value.find((item) => item.id === draft.savedId)
  return query
    ? `${query.name}${query.sql !== draft.sql ? ' •' : ''}`
    : draft.name || `Query ${draft.number}`
}
async function startRename(draft: QueryDraft) {
  if (props.busy) return
  activate(draft.id)
  const query = saved.value.find((item) => item.id === draft.savedId)
  tabName.value = query?.name || draft.name || `Query ${draft.number}`
  renamingId.value = draft.id
  await nextTick()
  const input = document.getElementById(`${draft.id}-name`) as HTMLInputElement | null
  input?.focus()
  input?.select()
}
function finishRename() {
  const draft = drafts.value.find((item) => item.id === renamingId.value)
  const value = tabName.value.trim().slice(0, 80)
  if (draft && value) {
    if (draft.savedId) {
      persist(
        saved.value.map((query) =>
          query.id === draft.savedId ? { ...query, name: value } : query,
        ),
      )
    } else draft.name = value
  }
  renamingId.value = null
}
async function endRename(cancel = false) {
  const id = renamingId.value
  if (cancel) renamingId.value = null
  else finishRename()
  await nextTick()
  document.getElementById(`${id}-tab`)?.focus()
}
function activate(id: string) {
  if (props.busy) return
  activeId.value = id
  emit('activate', id)
}
async function navigateTab(event: KeyboardEvent, index: number) {
  const last = drafts.value.length - 1
  const target =
    event.key === 'ArrowRight'
      ? (index + 1) % drafts.value.length
      : event.key === 'ArrowLeft'
        ? (index + last) % drafts.value.length
        : event.key === 'Home'
          ? 0
          : event.key === 'End'
            ? last
            : null
  if (target === null || props.busy) return
  event.preventDefault()
  const draft = drafts.value[target]!
  activate(draft.id)
  await nextTick()
  document.getElementById(`${draft.id}-tab`)?.focus()
}
function closeDraft(id: string) {
  if (props.busy || drafts.value.length === 1) return
  const index = drafts.value.findIndex((draft) => draft.id === id)
  if (activeId.value === id) activate(drafts.value[index === 0 ? 1 : index - 1]!.id)
  drafts.value = drafts.value.filter((draft) => draft.id !== id)
  emit('close', id)
}
function persist(queries: SavedQuery[]) {
  try {
    localStorage.setItem(storageKey, JSON.stringify(queries))
    saved.value = queries
    storageError.value = ''
    return true
  } catch {
    storageError.value = 'Could not save changes. Check browser storage and try again.'
    return false
  }
}
async function startSave() {
  name.value = selected.value?.name || activeDraft.value.name || ''
  naming.value = true
  feedback.value = ''
  await nextTick()
  nameInput.value?.focus()
}
function save() {
  if (!name.value.trim() || !sql.value.trim() || sql.value.length > 12_000) return
  if (!selected.value && saved.value.length >= 100) {
    storageError.value = 'You have 100 saved queries. Remove one before saving another.'
    return
  }
  const query: SavedQuery = {
    id: selected.value?.id || crypto.randomUUID(),
    name: name.value.trim(),
    sql: sql.value,
  }
  const queries = selected.value
    ? saved.value.map((item) => (item.id === query.id ? query : item))
    : [...saved.value, query]
  if (persist(queries)) {
    activeDraft.value.savedId = query.id
    naming.value = false
    feedback.value = 'Saved in this browser'
  }
}
async function load(query: SavedQuery) {
  if (props.busy) return
  const existing = drafts.value.find((draft) => draft.savedId === query.id)
  if (existing) activate(existing.id)
  else await newQuery(query)
  if (library.value) library.value.open = false
  editor.value?.focus()
}
function remove(id: string) {
  if (persist(saved.value.filter((query) => query.id !== id))) {
    for (const draft of drafts.value) {
      if (draft.savedId === id) draft.savedId = null
    }
    feedback.value = 'Query removed'
  }
}
async function newQuery(query?: SavedQuery) {
  if (props.busy) return
  const number = ++nextNumber
  const draft: QueryDraft = {
    id: `query-${number}`,
    number,
    sql: query?.sql || props.initialSql,
    savedId: query?.id || null,
  }
  drafts.value.push(draft)
  activate(draft.id)
  await nextTick()
  editor.value?.focus()
}
function run() {
  if (!props.busy && sql.value.trim()) emit('run', sql.value)
}
</script>

<template>
  <section ref="workspace" class="query-space" aria-label="Query workspace">
    <div class="query-toolbar">
      <div class="query-tabs" role="tablist" aria-label="Queries">
        <div
          v-for="(draft, index) in drafts"
          :key="draft.id"
          class="query-tab"
          :class="{ active: activeId === draft.id }"
          role="presentation"
        >
          <button
            v-show="renamingId !== draft.id"
            :id="`${draft.id}-tab`"
            class="query-tab-label"
            role="tab"
            :aria-selected="activeId === draft.id"
            aria-controls="query-panel"
            :tabindex="activeId === draft.id ? 0 : -1"
            :disabled="busy"
            :title="`${draftName(draft)} · Double-click to rename`"
            @click="activate(draft.id)"
            @dblclick="startRename(draft)"
            @keydown="navigateTab($event, index)"
          >
            {{ draftName(draft) }}
          </button>
          <input
            v-if="renamingId === draft.id"
            :id="`${draft.id}-name`"
            v-model="tabName"
            class="query-tab-name"
            aria-label="Rename query tab"
            maxlength="80"
            @blur="finishRename"
            @keydown.enter.prevent="endRename()"
            @keydown.esc.prevent="endRename(true)"
          />
          <button
            v-if="drafts.length > 1"
            class="query-tab-close"
            type="button"
            :disabled="busy"
            :aria-label="`Close ${draftName(draft)}`"
            @click="closeDraft(draft.id)"
          >
            <AppIcon name="close" :size="12" />
          </button>
        </div>
        <button
          class="icon-button query-add"
          type="button"
          :disabled="busy"
          title="New query"
          aria-label="New query"
          @click="newQuery()"
        >
          <AppIcon name="plus" :size="14" />
        </button>
      </div>
      <div class="query-actions">
        <details
          ref="library"
          class="query-library"
          @keydown.esc="library && (library.open = false)"
        >
          <summary>
            Saved<span v-if="saved.length"> {{ saved.length }}</span
            ><AppIcon name="down" :size="12" />
          </summary>
          <div class="query-library-panel">
            <p class="query-library-note">Saved in this browser</p>
            <p v-if="!saved.length" class="query-library-empty">Save a query to reuse it here.</p>
            <ul v-else>
              <li v-for="query in saved" :key="query.id">
                <button
                  class="query-load"
                  type="button"
                  :title="query.sql"
                  :disabled="busy"
                  @click="load(query)"
                >
                  {{ query.name }}
                </button>
                <button
                  class="icon-button"
                  type="button"
                  :aria-label="`Delete ${query.name}`"
                  :title="`Delete ${query.name}`"
                  @click="remove(query.id)"
                >
                  <AppIcon name="close" :size="13" />
                </button>
              </li>
            </ul>
          </div>
        </details>
        <button class="text-button" type="button" :disabled="!sql.trim()" @click="startSave">
          {{ selected ? 'Update' : 'Save' }}
        </button>
        <button
          class="button primary small query-run"
          type="button"
          :disabled="busy || !sql.trim()"
          title="Run query (Ctrl / ⌘ + Enter)"
          @click="run"
        >
          <AppIcon name="play" :size="12" />{{ busy ? 'Running…' : 'Run' }}
        </button>
        <div class="query-help">
          <button
            class="icon-button"
            type="button"
            aria-label="Query help and keyboard shortcuts"
            aria-describedby="query-help"
          >
            <span aria-hidden="true">?</span>
          </button>
          <div id="query-help" class="query-help-tooltip" role="tooltip">
            <span>Read only · Ctrl / ⌘ + Enter to run</span>
            <span>Ctrl + Space: suggest · Tab / Enter: accept · Esc: dismiss</span>
          </div>
        </div>
      </div>
    </div>
    <div id="query-panel" class="query-panel" role="tabpanel" :aria-labelledby="`${activeId}-tab`">
      <SqlEditor
        :provider="provider"
        :key="activeId"
        ref="editor"
        v-model="sql"
        :height="editorHeight"
        :tables="tables"
        :selected-table="selectedTable"
        @update:model-value="feedback = ''"
        @run="run"
      />
    </div>
    <form
      v-if="naming"
      class="query-save-form"
      @submit.prevent="save"
      @keydown.esc="naming = false"
    >
      <input
        ref="nameInput"
        v-model="name"
        aria-label="Query name"
        placeholder="Name this query"
        maxlength="80"
        required
      />
      <button class="button secondary small" type="submit" :disabled="!name.trim()">
        {{ selected ? 'Update query' : 'Save query' }}
      </button>
      <button class="text-button" type="button" @click="naming = false">Cancel</button>
    </form>
    <div v-if="(activeQuery && sql.trim() !== activeQuery) || demo" class="query-footnote">
      <span v-if="activeQuery && sql.trim() !== activeQuery"
        >Query changed · Run to update results</span
      >
      <span v-else-if="demo">Sample: SELECT * FROM a table, with optional LIMIT</span>
    </div>
    <div
      class="query-resize"
      :class="{ dragging }"
      role="separator"
      tabindex="0"
      aria-label="Resize query workspace"
      aria-orientation="horizontal"
      aria-controls="query-panel table-grid-region"
      :aria-valuemin="minEditorHeight"
      :aria-valuemax="maxEditorHeight"
      :aria-valuenow="editorHeight"
      :aria-valuetext="`Query editor height: ${editorHeight} pixels`"
      title="Drag to resize · Arrow keys to adjust · Double-click to reset"
      @pointerdown="startResize"
      @pointermove="moveResize"
      @pointerup="endResize"
      @pointercancel="endResize"
      @lostpointercapture="endResize"
      @keydown="resizeWithKeyboard"
      @dblclick="setEditorHeight(124)"
    />
  </section>
</template>
