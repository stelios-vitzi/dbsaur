<script setup lang="ts">
import { ref, onMounted, useTemplateRef } from 'vue'
import { $fetch } from '../services/http'
import { encryptVault, decryptVault, recoveryPassphrase } from '../utils/vaultBackup'
const emit = defineEmits<{ close: []; imported: [] }>()
const dialog = useTemplateRef<HTMLDialogElement>('dialog')
const passphrase = ref('')
const busy = ref(false)
const error = ref('')
const notice = ref('')
const backup = ref<File>()
onMounted(() => {
  generate()
  dialog.value?.showModal()
})
function generate() {
  passphrase.value = recoveryPassphrase()
  notice.value =
    'Store this recovery passphrase separately from the backup. It is shown only in this dialog and cannot be recovered.'
}
async function transfer(importing: boolean) {
  if (busy.value) return
  busy.value = true
  error.value = ''
  notice.value = ''
  try {
    if (importing) {
      if (!backup.value || backup.value.size > 64 * 1024 * 1024)
        throw new Error('Choose a vault backup of at most 64 MiB.')
      const profiles = await decryptVault(await backup.value.text(), passphrase.value)
      await $fetch('/api/vault/import', {
        method: 'POST',
        body: { confirmation: 'IMPORT', profiles },
      })
      notice.value = 'Connections imported. Existing connections were preserved.'
      emit('imported')
    } else {
      if (passphrase.value.length < 12)
        throw new Error('Choose a passphrase of at least 12 characters before exporting.')
      const profiles = await $fetch<unknown>('/api/vault/export', {
        method: 'POST',
        body: { confirmation: 'EXPORT' },
      })
      const blob = new Blob([await encryptVault(profiles, passphrase.value)], {
        type: 'application/octet-stream',
      })
      const url = URL.createObjectURL(blob)
      const anchor = document.createElement('a')
      anchor.href = url
      anchor.download = 'connections.dbsaur-vault'
      anchor.click()
      setTimeout(() => URL.revokeObjectURL(url), 60_000)
      notice.value =
        'Encrypted backup downloaded. Keep its passphrase separately; test recovery before relying on it.'
    }
    passphrase.value = ''
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : 'Vault transfer failed.'
  } finally {
    busy.value = false
  }
}
</script>
<template>
  <dialog
    ref="dialog"
    class="mutation-dialog"
    aria-label="Vault backup and recovery"
    @cancel="busy && $event.preventDefault()"
    @close="emit('close')"
  >
    <h2>Vault backup and recovery</h2>
    <p>
      Export saved profiles and credentials in an encrypted file. Import adds connections without
      contacting their database servers. Protect the file and use a unique passphrase.
    </p>
    <label
      >Backup passphrase<input
        v-model="passphrase"
        type="password"
        autocomplete="new-password"
        :disabled="busy"
    /></label>
    <button type="button" class="button secondary small" :disabled="busy" @click="generate">
      Generate recovery passphrase
    </button>
    <p v-if="notice" role="status">{{ notice }}</p>
    <details v-if="passphrase">
      <summary>Show passphrase for secure storage</summary>
      <code>{{ passphrase }}</code>
    </details>
    <label
      >Restore backup<input
        type="file"
        accept=".dbsaur-vault"
        :disabled="busy"
        @change="backup = ($event.target as HTMLInputElement).files?.[0]"
    /></label>
    <p v-if="error" role="alert">{{ error }}</p>
    <div class="mutation-actions">
      <button class="button secondary" :disabled="busy" @click="emit('close')">Close</button>
      <button
        class="button secondary"
        :disabled="busy || !backup || !passphrase"
        @click="transfer(true)"
      >
        Import connections
      </button>
      <button class="button" :disabled="busy || !passphrase" @click="transfer(false)">
        Export encrypted backup
      </button>
    </div>
  </dialog>
</template>
