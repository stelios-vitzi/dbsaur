<script setup lang="ts">
import AppIcon from './AppIcon.vue'

import type { SavedConnection } from '../../shared/database'

defineProps<{
  savedConnections: SavedConnection[]
  busy: boolean
  error: string
}>()
const emit = defineEmits<{
  reconnect: [id: string]
  remove: [id: string]
  add: []
  sample: []
}>()
</script>

<template>
  <main class="launcher-main">
    <section class="launcher-content" aria-labelledby="launcher-title">
      <div class="launcher-heading">
        <span class="launcher-mark"><AppIcon name="database" :size="22" /></span>
        <div>
          <h1 id="launcher-title">Your databases</h1>
          <p>Choose a saved connection or add a database.</p>
        </div>
      </div>
      <p v-if="error" class="connection-feedback failure" role="alert">{{ error }}</p>
      <div class="connection-grid" aria-label="Database connections">
        <article class="connection-tile sample-tile">
          <button
            class="connection-tile-open"
            type="button"
            :disabled="busy"
            @click="emit('sample')"
          >
            <span class="connection-tile-icon"><AppIcon name="table" :size="20" /></span>
            <strong>Sample workspace</strong>
            <span class="connection-tile-detail">Read-only sample data</span>
            <span class="sample-tile-badge">Default</span>
          </button>
        </article>
        <article
          v-for="connection in savedConnections"
          :key="connection.id"
          class="connection-tile"
        >
          <button
            class="connection-tile-open"
            type="button"
            :disabled="busy"
            @click="emit('reconnect', connection.id)"
          >
            <span class="connection-tile-icon"><AppIcon name="database" :size="20" /></span>
            <strong>{{ connection.name }}</strong>
            <span class="connection-tile-detail">Saved connection</span>
          </button>
          <button
            class="connection-tile-remove icon-button"
            type="button"
            :disabled="busy"
            :aria-label="`Remove ${connection.name}`"
            title="Remove saved connection"
            @click="emit('remove', connection.id)"
          >
            <AppIcon name="close" :size="14" />
          </button>
        </article>
        <article class="connection-tile add-tile">
          <button class="connection-tile-open" type="button" :disabled="busy" @click="emit('add')">
            <span class="connection-tile-icon"><AppIcon name="plus" :size="20" /></span>
            <strong>Add database</strong>
            <span class="connection-tile-detail">Set up a new connection</span>
          </button>
        </article>
      </div>
      <p v-if="busy" class="launcher-status" role="status">Connecting…</p>
    </section>
  </main>
</template>
