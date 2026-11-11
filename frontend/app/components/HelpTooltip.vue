<script setup lang="ts">
import { ref, useId } from 'vue'

defineProps<{ label: string; text: string }>()
const id = useId()
const open = ref(false)
</script>

<template>
  <span class="help-tooltip" @mouseenter="open = true" @mouseleave="open = false">
    <button
      type="button"
      class="hint"
      :aria-label="label"
      :aria-describedby="open ? id : undefined"
      @focus="open = true"
      @blur="open = false"
      @click="open = true"
      @keydown.esc.prevent.stop="open = false"
    >
      ?
    </button>
    <span v-show="open" :id="id" role="tooltip" class="help-tooltip-content">{{ text }}</span>
  </span>
</template>
