<script setup lang="ts">
import { ref } from 'vue'
import ContextMenu from './ContextMenu.vue'
import type { ContextAction } from '../../shared/database'

defineProps<{ menus: { label: string; actions: ContextAction[] }[] }>()
const open = ref<{ label: string; actions: ContextAction[]; x: number; y: number }>()
function show(event: Event, menu: { label: string; actions: ContextAction[] }) {
  const bounds = (event.currentTarget as HTMLElement).getBoundingClientRect()
  open.value = { ...menu, x: bounds.left, y: bounds.bottom + 2 }
}
</script>

<template>
  <nav class="app-menu-bar" aria-label="Application menu">
    <button
      v-for="menu in menus"
      :key="menu.label"
      :aria-expanded="open?.label === menu.label"
      aria-haspopup="menu"
      @click="show($event, menu)"
      @keydown.down.prevent="show($event, menu)"
      @keydown.up.prevent="show($event, menu)"
    >
      {{ menu.label }}
    </button>
    <ContextMenu v-if="open" :key="open.label" v-bind="open" compact @close="open = undefined" />
  </nav>
</template>

<style scoped>
.app-menu-bar {
  display: flex;
  align-items: center;
  flex-shrink: 0;
  gap: 2px;
  padding: 0;
}
.app-menu-bar > button {
  padding: 4px 9px;
  border-radius: 4px;
  color: var(--secondary);
  font-size: 12px;
}
.app-menu-bar > button:hover,
.app-menu-bar > button[aria-expanded='true'] {
  color: var(--text);
  background: var(--raised);
}
</style>
