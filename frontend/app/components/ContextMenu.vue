<script setup lang="ts">
import { ref, onMounted, onScopeDispose, useTemplateRef } from 'vue'

import type { ContextAction } from '../../shared/database'
const props = defineProps<{ x: number; y: number; label: string; actions: ContextAction[] }>()
const emit = defineEmits<{ close: [] }>()
const menu = useTemplateRef<HTMLElement>('menu')
const left = ref(props.x)
const top = ref(props.y)
const previous = document.activeElement as HTMLElement | null
function close() {
  emit('close')
  if (previous?.isConnected) previous.focus()
}
function outside(event: PointerEvent) {
  if (!menu.value?.contains(event.target as Node)) emit('close')
}
function keydown(event: KeyboardEvent) {
  if (event.key === 'Escape' || event.key === 'Tab') {
    if (event.key === 'Escape') event.preventDefault()
    close()
  }
  const buttons = [
    ...(menu.value?.querySelectorAll<HTMLButtonElement>('button:not(:disabled)') || []),
  ]
  if (['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) {
    event.preventDefault()
    const index = buttons.indexOf(document.activeElement as HTMLButtonElement)
    const next =
      event.key === 'Home'
        ? 0
        : event.key === 'End'
          ? buttons.length - 1
          : (index + (event.key === 'ArrowDown' ? 1 : -1) + buttons.length) % buttons.length
    buttons[next]?.focus()
  }
}
onMounted(() => {
  const bounds = menu.value!.getBoundingClientRect()
  left.value = Math.max(8, Math.min(props.x, window.innerWidth - bounds.width - 8))
  top.value = Math.max(8, Math.min(props.y, window.innerHeight - bounds.height - 8))
  menu.value?.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus()
  document.addEventListener('pointerdown', outside)
  window.addEventListener('resize', close)
  document.addEventListener('scroll', close, true)
})
onScopeDispose(() => {
  document.removeEventListener('pointerdown', outside)
  window.removeEventListener('resize', close)
  document.removeEventListener('scroll', close, true)
})
function choose(action: ContextAction) {
  close()
  void action.run()
}
</script>
<template>
  <Teleport to="body">
    <div
      ref="menu"
      class="context-menu"
      role="menu"
      :aria-label="label"
      :style="{ left: `${left}px`, top: `${top}px` }"
      @keydown="keydown"
      @contextmenu.prevent
    >
      <div class="context-menu-label">{{ label }}</div>
      <button
        v-for="action in actions"
        :key="action.label"
        role="menuitem"
        :disabled="action.disabled"
        @click="choose(action)"
      >
        {{ action.label }}
      </button>
    </div>
  </Teleport>
</template>
