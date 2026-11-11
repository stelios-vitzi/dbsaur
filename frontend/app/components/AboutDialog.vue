<script setup lang="ts">
import { onMounted, useTemplateRef } from 'vue'
import AppIcon from './AppIcon.vue'
import DesktopUpdater from './DesktopUpdater.vue'

defineEmits<{ close: [] }>()
const dialog = useTemplateRef<HTMLDialogElement>('dialog')
onMounted(() => dialog.value?.showModal())
</script>

<template>
  <dialog ref="dialog" class="about-dialog" aria-labelledby="about-title" @close="$emit('close')">
    <div class="about-heading">
      <h2 id="about-title">About DBsaur</h2>
      <button class="icon-button" aria-label="Close About" @click="dialog?.close()">
        <AppIcon name="close" :size="16" />
      </button>
    </div>
    <p>A quiet place for your data.</p>
    <section class="about-updates" aria-labelledby="about-updates-title">
      <h3 id="about-updates-title">Software updates</h3>
      <DesktopUpdater />
    </section>
    <form method="dialog"><button class="button secondary small">Close</button></form>
  </dialog>
</template>

<style scoped>
.about-dialog {
  width: min(420px, calc(100vw - 32px));
  max-height: calc(100dvh - 32px);
  overflow: auto;
  padding: 22px;
  color: var(--text);
  background: var(--sidebar);
  border: 1px solid var(--border);
  border-radius: 10px;
  box-shadow: 0 20px 70px #0006;
}
.about-dialog::backdrop {
  background: #0008;
}
.about-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 10px;
}
.about-dialog > p {
  color: var(--secondary);
}
.about-updates {
  margin-block: 24px;
  padding-top: 18px;
  border-top: 1px solid var(--border);
}
.about-updates h3 {
  margin-bottom: 12px;
}
.about-dialog form {
  display: flex;
  justify-content: flex-end;
}
</style>
