<script setup lang="ts">
import { onMounted, useTemplateRef } from 'vue'
defineProps<{ message: string; details: string }>()
const emit = defineEmits<{ respond: [accepted: boolean] }>()
const dialog = useTemplateRef<HTMLDialogElement>('dialog')
onMounted(() => dialog.value?.showModal())
</script>
<template>
  <Teleport to="body">
    <dialog
      ref="dialog"
      class="mutation-dialog"
      aria-label="Review database change"
      @cancel.prevent="emit('respond', false)"
    >
      <h2>Review database change</h2>
      <p>{{ message }}</p>
      <p>
        This is the exact pending operation and its values. SQL record changes use bound parameters
        and server metadata; this operation is not a generated SQL script or an executed dry run.
      </p>
      <pre class="mutation-record">{{ details }}</pre>
      <div class="mutation-actions">
        <button class="button secondary" autofocus @click="emit('respond', false)">Cancel</button>
        <button class="button" @click="emit('respond', true)">Save change</button>
      </div>
    </dialog>
  </Teleport>
</template>
