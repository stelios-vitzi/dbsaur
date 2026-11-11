<script setup lang="ts">
import { ref, computed, watch, nextTick, useId, useTemplateRef, reactive } from 'vue'
import AppIcon from './AppIcon.vue'
import HelpTooltip from './HelpTooltip.vue'

import { providerOptions } from '../providers'
import { parseConnectionUrl } from '../utils/connectionUrl'
import type { ConnectionInput, Provider, SavedConnection } from '../../shared/database'
const props = defineProps<{
  busy: boolean
  error: string
  providers: Provider[]
  savedConnections: SavedConnection[]
  showSavedConnections?: boolean
  savingEnabled: boolean
  testConnection: (input: ConnectionInput) => Promise<{ ok: boolean; elapsedMs: number }>
}>()
const emit = defineEmits<{
  connect: [input: ConnectionInput]
  reconnect: [id: string]
  remove: [id: string]
}>()
const save = ref(false)
const saveName = ref('')
const formElement = useTemplateRef<HTMLFormElement>('formElement')
const urlInput = ref('')
const importDialog = useTemplateRef<HTMLDialogElement>('importDialog')
const tlsDialog = useTemplateRef<HTMLDialogElement>('tlsDialog')
const modalError = ref('')
const localError = ref('')
const testResult = ref('')
const testing = ref(false)
const readingFile = ref(false)
const showPassword = ref(false)
const locked = computed(() => props.busy || testing.value || readingFile.value)
const savedFilter = ref('')
const filteredConnections = computed(() =>
  props.savedConnections.filter((saved) =>
    saved.name.toLowerCase().includes(savedFilter.value.toLowerCase()),
  ),
)
const modalId = useId()
function openSettings(kind: 'import' | 'tls') {
  if (locked.value) return
  modalError.value = ''
  if (kind === 'import') importDialog.value?.showModal()
  else tlsDialog.value?.showModal()
}
function closeSettings() {
  if (readingFile.value) return
  importDialog.value?.close()
  tlsDialog.value?.close()
}
function modalClosed() {
  urlInput.value = ''
  modalError.value = ''
}
function modalCancel(event: Event) {
  if (readingFile.value) event.preventDefault()
}
const form = reactive<ConnectionInput>({
  provider: 'postgres',
  host: 'localhost',
  port: 5432,
  database: '',
  username: '',
  password: '',
  ssl: false,
  allDatabases: false,
  readOnly: false,
})
const options = computed(() => providerOptions(form.provider))
watch(
  () => form.provider,
  () => {
    form.port = options.value.port
    form.password = ''
    form.tlsCa = undefined
    form.tlsCert = undefined
    form.tlsKey = undefined
    showPassword.value = false
  },
)
watch(form, () => {
  testResult.value = ''
  localError.value = ''
})
watch(
  () => form.ssl,
  (enabled) => {
    if (!enabled) {
      form.tlsCa = undefined
      form.tlsCert = undefined
      form.tlsKey = undefined
    }
  },
)
function input(): ConnectionInput {
  return {
    ...form,
    database: form.allDatabases ? '' : form.database,
    saveName: save.value && props.savingEnabled ? saveName.value.trim() : undefined,
  }
}
function valid() {
  localError.value = ''
  if (!formElement.value?.reportValidity()) return false
  if (!!form.tlsCert !== !!form.tlsKey) {
    localError.value = 'Choose both a client certificate and its private key.'
    return false
  }
  return true
}
function submit() {
  if (locked.value || !valid()) return
  if (save.value && props.savingEnabled && !saveName.value.trim()) {
    localError.value = 'Enter a name for the saved connection.'
    return
  }
  emit('connect', input())
  form.password = ''
  showPassword.value = false
}
async function test() {
  if (locked.value || !valid()) return
  testResult.value = ''
  testing.value = true
  try {
    const result = await props.testConnection({ ...input(), saveName: undefined })
    testResult.value = `Connection successful (${result.elapsedMs} ms). Ready to connect.`
  } catch (cause) {
    localError.value = cause instanceof Error ? cause.message : 'Connection test failed.'
  } finally {
    testing.value = false
  }
}
async function importUrl() {
  if (locked.value) return
  modalError.value = ''
  try {
    const parsed = parseConnectionUrl(urlInput.value)
    if (
      props.providers.length &&
      !props.providers.some((provider) => provider.id === parsed.provider)
    )
      throw new Error('This database provider is unavailable on the running backend.')
    // Flush the provider watcher before assigning the imported port and password.
    form.provider = parsed.provider
    await nextTick()
    Object.assign(form, parsed, { tlsCa: undefined, tlsCert: undefined, tlsKey: undefined })
    urlInput.value = ''
    closeSettings()
  } catch (cause) {
    modalError.value = cause instanceof Error ? cause.message : 'Could not import this URL.'
  }
}
async function readPem(event: Event, field: 'tlsCa' | 'tlsCert' | 'tlsKey') {
  const target = event.target as HTMLInputElement
  const file = target.files?.[0]
  if (!file) return
  readingFile.value = true
  modalError.value = ''
  try {
    if (file.size > 32768) throw new Error('Each TLS file must be at most 32 KiB.')
    const pem = await file.text()
    const kinds =
      field === 'tlsKey' ? ['PRIVATE KEY', 'RSA PRIVATE KEY', 'EC PRIVATE KEY'] : ['CERTIFICATE']
    if (
      pem.includes('\0') ||
      !kinds.some(
        (kind) => pem.includes(`-----BEGIN ${kind}-----`) && pem.includes(`-----END ${kind}-----`),
      )
    )
      throw new Error(
        field === 'tlsKey'
          ? 'Choose an unencrypted PEM private key.'
          : 'Choose a PEM certificate file.',
      )
    form[field] = pem
    await nextTick()
  } catch (cause) {
    modalError.value = cause instanceof Error ? cause.message : 'Could not read this file.'
  } finally {
    target.value = ''
    readingFile.value = false
  }
}
</script>
<template>
  <form ref="formElement" class="connection-form" @submit.prevent="submit">
    <fieldset class="connection-fields" :disabled="locked">
      <div class="connection-heading">
        <span class="connection-symbol"><AppIcon name="database" :size="26" /></span>
        <div>
          <h1>Connect to your database</h1>
          <p>Connect, verify, then explore.</p>
        </div>
      </div>
      <section
        v-if="showSavedConnections && savedConnections.length"
        class="saved-connections"
        aria-label="Saved connections"
      >
        <h2>Saved connections</h2>
        <input
          v-model="savedFilter"
          type="search"
          aria-label="Filter saved connections"
          placeholder="Filter saved connections"
        />
        <p v-if="!filteredConnections.length" class="field-help">No matching connections.</p>
        <div v-for="saved in filteredConnections" :key="saved.id" class="saved-connection">
          <button
            type="button"
            class="text-button"
            :disabled="busy"
            @click="emit('reconnect', saved.id)"
          >
            <AppIcon name="database" :size="14" />{{ saved.name }}
          </button>
          <button
            type="button"
            class="icon-button"
            :disabled="busy"
            :aria-label="`Remove ${saved.name}`"
            title="Remove saved connection"
            @click="emit('remove', saved.id)"
          >
            <AppIcon name="close" :size="13" />
          </button>
        </div>
      </section>
      <button
        type="button"
        class="connection-settings-button"
        aria-haspopup="dialog"
        @click="openSettings('import')"
      >
        <span>Import from URL</span><AppIcon name="arrow" :size="16" />
      </button>
      <div class="connection-address">
        <label
          >Database type<select v-model="form.provider" :disabled="busy">
            <option
              v-for="provider in providers.filter((p) => p.id !== 'demo')"
              :key="provider.id"
              :value="provider.id"
            >
              {{ provider.name }}
            </option>
            <option v-if="!providers.length" value="postgres">PostgreSQL</option>
          </select></label
        >
        <div class="form-row host-row">
          <label
            >Host<input
              v-model.trim="form.host"
              required
              maxlength="253"
              autocomplete="off"
              placeholder="localhost"
              :disabled="busy" /></label
          ><label
            >Port<input
              v-model.number="form.port"
              type="number"
              min="1"
              max="65535"
              required
              :disabled="busy"
          /></label>
        </div>
      </div>
      <div class="connection-option">
        <label class="checkbox-label scope-toggle">
          <input v-model="form.allDatabases" type="checkbox" role="switch" :disabled="busy" />
          <span>Browse all databases in this instance</span>
        </label>
        <HelpTooltip
          label="About browsing all databases"
          :text="`Discover databases your user can access, then choose one in the explorer. Connects ${options.maintenance}.`"
        />
      </div>
      <label v-if="!form.allDatabases"
        >Database<input
          v-model.trim="form.database"
          required
          maxlength="128"
          autocomplete="off"
          placeholder="Database name"
          :disabled="busy"
      /></label>
      <div class="form-row">
        <label
          >Username<input
            v-model="form.username"
            required
            maxlength="128"
            autocomplete="username"
            :placeholder="options.username"
            :disabled="busy" /></label
        ><label
          >Password<span class="password-field"
            ><input
              v-model="form.password"
              :type="showPassword ? 'text' : 'password'"
              maxlength="4096"
              autocomplete="current-password"
              placeholder="Enter password"
              :disabled="busy"
            /><button
              type="button"
              class="text-button"
              :aria-pressed="showPassword"
              :aria-label="showPassword ? 'Hide password' : 'Show password'"
              @click="showPassword = !showPassword"
            >
              {{ showPassword ? 'Hide' : 'Show' }}
            </button></span
          ></label
        >
      </div>
      <div class="connection-tls">
        <input
          v-model="form.ssl"
          type="checkbox"
          aria-label="Enable verified TLS"
          :disabled="locked"
        />
        <button
          type="button"
          class="connection-settings-button"
          aria-haspopup="dialog"
          @click="openSettings('tls')"
        >
          <span>Verify TLS connection</span
          ><span class="tls-status">{{ form.ssl ? 'Enabled' : 'Disabled' }}</span
          ><AppIcon name="settings" :size="16" />
        </button>
      </div>
      <div class="connection-preferences">
        <div>
          <div class="connection-option">
            <label class="checkbox-label"
              ><input v-model="form.readOnly" type="checkbox" /><span>Read-only mode</span></label
            >
            <HelpTooltip
              label="About read-only mode"
              text="Blocks record, schema, and settings changes for this connection. Database permissions still apply."
            />
          </div>
        </div>
        <div>
          <div class="connection-option">
            <label class="checkbox-label save-toggle"
              ><input v-model="save" type="checkbox" :disabled="busy || !savingEnabled" /><span
                >Save connection and credentials</span
              ></label
            >
            <HelpTooltip
              v-if="!savingEnabled"
              label="Why saving is unavailable"
              text="Saving is unavailable. Configure the backend connection vault to enable it."
            />
          </div>
          <label v-if="save && savingEnabled"
            >Connection name<input
              v-model.trim="saveName"
              maxlength="128"
              placeholder="e.g. Local development"
              :disabled="busy"
          /></label>
        </div>
      </div>
    </fieldset>
    <p v-if="localError || error" class="connection-feedback failure" role="alert">
      {{ localError || error }}
    </p>
    <p v-if="testResult" class="connection-feedback" role="status">{{ testResult }}</p>
    <div class="connection-actions">
      <button class="button secondary" type="button" :disabled="locked" @click="test">
        {{ testing ? 'Testing…' : 'Test connection' }}
      </button>
      <button class="button primary connect-button" type="submit" :disabled="locked">
        <span>{{
          busy && !testing
            ? 'Connecting…'
            : form.allDatabases
              ? 'Connect to instance'
              : 'Connect to database'
        }}</span><AppIcon name="arrow" />
      </button>
    </div>
  </form>
  <Teleport to="body">
    <dialog
      ref="importDialog"
      class="connection-settings-modal"
      :aria-labelledby="modalId + '-import'"
      @close="modalClosed"
      @cancel="modalCancel"
    >
      <form class="connection-modal-form" @submit.prevent="importUrl">
        <div class="connection-modal-heading">
          <h2 :id="modalId + '-import'">Import from URL</h2>
          <button
            type="button"
            class="icon-button"
            aria-label="Close URL import"
            :disabled="locked"
            @click="closeSettings"
          >
            <AppIcon name="close" />
          </button>
        </div>
        <label
          >Connection URL<input
            v-model="urlInput"
            type="password"
            autofocus
            :disabled="locked"
            autocomplete="off"
            spellcheck="false"
            placeholder="postgresql://user:password@host:5432/database"
            @keydown.enter.prevent="importUrl"
        /></label>
        <p class="field-help">
          Imports supported URL fields without connecting. TLS is disabled unless specified in the
          URL. Unsupported options are rejected.
        </p>

        <p v-if="modalError" class="connection-feedback failure" role="alert">{{ modalError }}</p>
        <div class="connection-modal-actions">
          <button type="button" class="button secondary" :disabled="locked" @click="closeSettings">
            Cancel</button
          ><button type="submit" class="button primary" :disabled="locked">Import URL</button>
        </div>
      </form>
    </dialog>
    <dialog
      ref="tlsDialog"
      class="connection-settings-modal"
      :aria-labelledby="modalId + '-tls'"
      @close="modalClosed"
      @cancel="modalCancel"
    >
      <div class="connection-modal-form">
        <div class="connection-modal-heading">
          <h2 :id="modalId + '-tls'">TLS / SSL settings</h2>
          <button
            type="button"
            class="icon-button"
            aria-label="Close TLS settings"
            :disabled="locked"
            @click="closeSettings"
          >
            <AppIcon name="close" />
          </button>
        </div>
        <label class="checkbox-label"
          ><input v-model="form.ssl" type="checkbox" :disabled="locked" /><span
            >Verify TLS connection</span
          ></label
        >
        <fieldset class="connection-fields" :disabled="locked">
          <p class="field-help">
            {{
              form.ssl
                ? 'Server certificate and hostname verification are required. Uses trusted public certificate authorities by default.'
                : 'TLS is disabled. Use only on a trusted local network.'
            }}
          </p>
          <template v-if="form.ssl && form.provider !== 'sqlserver'">
            <div
              v-for="file in [
                { field: 'tlsCa', label: 'CA certificate' },
                { field: 'tlsCert', label: 'Client certificate' },
                { field: 'tlsKey', label: 'Client private key' },
              ] as const"
              :key="file.field"
              class="tls-file"
            >
              <label
                >{{ file.label }} (optional)<input
                  type="file"
                  accept=".pem,.crt,.cer,.key"
                  @change="readPem($event, file.field)"
              /></label>
              <span v-if="form[file.field]"
                >Loaded
                <button
                  type="button"
                  class="text-button"
                  :aria-label="`Remove ${file.label}`"
                  @click="form[file.field] = undefined"
                >
                  Remove
                </button></span
              >
            </div>
            <p class="field-help">
              PEM files, up to 32 KiB each. Client authentication needs both a certificate and an
              unencrypted key.
            </p>
          </template>
          <p v-else-if="form.ssl" class="field-help">
            SQL Server uses trusted public certificate authorities. Custom certificates are not
            supported yet.
          </p>
        </fieldset>
        <p v-if="modalError" class="connection-feedback failure" role="alert">{{ modalError }}</p>
        <div class="connection-modal-actions">
          <button type="button" class="button primary" :disabled="locked" @click="closeSettings">
            Done
          </button>
        </div>
      </div>
    </dialog>
  </Teleport>
</template>
