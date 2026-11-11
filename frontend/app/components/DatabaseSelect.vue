<script setup lang="ts">
import {
  ref,
  computed,
  watch,
  onMounted,
  onScopeDispose,
  nextTick,
  useId,
  useTemplateRef,
} from 'vue'
import AppIcon from './AppIcon.vue'

const props = defineProps<{
  databases: string[]
  modelValue: string
  disabled: boolean
}>()
const emit = defineEmits<{ 'update:modelValue': [database: string] }>()
const root = useTemplateRef<HTMLElement>('root')
const open = ref(false)
const highlighted = ref(0)
const listId = useId()
const options = computed(() => [...new Set([props.modelValue, ...props.databases])])
function reveal() {
  open.value = true
  highlighted.value = Math.max(0, options.value.indexOf(props.modelValue))
}
function choose(index: number) {
  const database = options.value[index]
  open.value = false
  if (database !== undefined && !props.disabled) emit('update:modelValue', database)
}
function keydown(event: KeyboardEvent) {
  if (props.disabled) return
  if (event.key === 'Escape' || event.key === 'Tab') {
    open.value = false
    return
  }
  if (['ArrowDown', 'ArrowUp', 'Home', 'End', 'Enter', ' '].includes(event.key)) {
    event.preventDefault()
    if (!open.value) {
      reveal()
      return
    }
    if (event.key === 'Enter' || event.key === ' ') choose(highlighted.value)
    else if (event.key === 'Home') highlighted.value = 0
    else if (event.key === 'End') highlighted.value = options.value.length - 1
    else
      highlighted.value =
        (highlighted.value + (event.key === 'ArrowDown' ? 1 : -1) + options.value.length) %
        options.value.length
  } else if (event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
    event.preventDefault()
    if (!open.value) reveal()
    const index = options.value.findIndex(
      (name, index) =>
        index > highlighted.value && name.toLowerCase().startsWith(event.key.toLowerCase()),
    )
    const match =
      index >= 0
        ? index
        : options.value.findIndex((name) => name.toLowerCase().startsWith(event.key.toLowerCase()))
    if (match >= 0) highlighted.value = match
  }
}
function outside(event: PointerEvent) {
  if (!root.value?.contains(event.target as Node)) open.value = false
}
function blur(event: FocusEvent) {
  if (!root.value?.contains(event.relatedTarget as Node | null)) open.value = false
}
watch(highlighted, async () => {
  await nextTick()
  root.value?.querySelector('[data-highlighted="true"]')?.scrollIntoView({ block: 'nearest' })
})
watch(
  () => props.disabled,
  () => {
    open.value = false
  },
)
watch(
  () => props.modelValue,
  () => {
    open.value = false
  },
)
onMounted(() => document.addEventListener('pointerdown', outside))
onScopeDispose(() => document.removeEventListener('pointerdown', outside))
</script>
<template>
  <div ref="root" class="database-select" @focusout="blur">
    <button
      class="database-select-trigger"
      role="combobox"
      aria-label="Database"
      aria-haspopup="listbox"
      :aria-expanded="open"
      :aria-controls="listId"
      :aria-activedescendant="open ? `${listId}-${highlighted}` : undefined"
      :disabled="disabled"
      :title="modelValue"
      @click="open ? (open = false) : reveal()"
      @keydown="keydown"
    >
      <AppIcon name="database" :size="14" />
      <span>{{ modelValue }}</span>
      <AppIcon name="down" :size="14" />
    </button>
    <ul v-if="open" :id="listId" class="database-select-menu" role="listbox" aria-label="Databases">
      <li
        v-for="(database, index) in options"
        :id="`${listId}-${index}`"
        :key="database"
        role="option"
        :aria-selected="database === modelValue"
        :data-highlighted="index === highlighted"
        :title="database"
        @pointermove="highlighted = index"
        @pointerdown.prevent
        @click="choose(index)"
      >
        <span>{{ database }}</span>
        <AppIcon v-if="database === modelValue" name="check" :size="14" />
      </li>
    </ul>
  </div>
</template>
