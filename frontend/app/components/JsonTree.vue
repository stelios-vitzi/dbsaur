<script setup lang="ts">
import { computed } from 'vue'
import JsonView from './JsonView.vue'

const props = withDefaults(defineProps<{ value: unknown; depth?: number }>(), { depth: 0 })
const entries = computed(() =>
  props.value !== null && typeof props.value === 'object' ? Object.entries(props.value) : [],
)
function isContainer(value: unknown) {
  return value !== null && typeof value === 'object' && Object.keys(value).length > 0
}
function describe(value: unknown) {
  const count = Object.keys(value as object).length
  return Array.isArray(value) ? `Array (${count})` : `Object (${count})`
}
</script>

<template>
  <ul class="json-tree" aria-label="Document fields">
    <li v-for="[key, item] in entries" :key="key">
      <details v-if="isContainer(item) && depth < 32">
        <summary>
          <span class="field-name">{{ JSON.stringify(key) }}</span>:
          <span class="field-type">{{ describe(item) }}</span>
        </summary>
        <JsonTree :value="item" :depth="depth + 1" />
      </details>
      <div v-else class="json-leaf">
        <span class="field-name">{{ JSON.stringify(key) }}</span>: <JsonView :value="item" />
      </div>
    </li>
  </ul>
</template>

<style scoped>
.json-tree {
  list-style: none;
  margin: 0;
  padding: 0 0 0 18px;
  font: 12px/1.7 var(--font-code);
  overflow-wrap: anywhere;
}
.json-tree li {
  padding: 2px 0;
}
summary {
  cursor: pointer;
}
summary:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}
.field-name {
  color: var(--accent);
}
.field-type {
  color: var(--muted);
}
.json-leaf :deep(.json-view) {
  display: inline;
  margin: 0;
  padding: 0;
  background: transparent;
}
</style>
