<script setup lang="ts">
import { ref, onMounted, onScopeDispose, useTemplateRef } from 'vue'
import { $fetch } from '../services/http'
import AppIcon from './AppIcon.vue'

import type { Map as MapLibreMap } from 'maplibre-gl'
import type { Geometry } from 'geojson'
import mapWorkerUrl from 'maplibre-gl/dist/maplibre-gl-worker.mjs?url'
import {
  basemapProtocol,
  fetchBasemapResource,
  geometryBounds,
  mapBackgrounds,
  type GeometryBounds,
} from '../utils/geometryMap'

const props = defineProps<{ value: string; column: string }>()
const emit = defineEmits<{ close: [] }>()
const dialog = useTemplateRef<HTMLDialogElement>('dialog')
const container = useTemplateRef<HTMLDivElement>('container')
const loading = ref(true)
const error = ref('')
const notice = ref('')
const srid = ref<number>()
const offline = ref(false)
let map: MapLibreMap | undefined
let bounds: GeometryBounds | null = null
let disposed = false
const controller = new AbortController()

function fit() {
  if (bounds) map?.fitBounds(bounds, { padding: 32, maxZoom: 16, duration: 0 })
}
onMounted(async () => {
  dialog.value?.showModal()
  try {
    const preview = await $fetch<{ srid: number; empty: boolean; geojson: Geometry | null }>(
      '/api/geometry',
      { method: 'POST', body: { value: props.value }, signal: controller.signal },
    )
    if (disposed) return
    srid.value = preview.srid
    if (preview.empty) {
      notice.value = 'This geometry is empty. There are no coordinates to display.'
      return
    }
    if (!preview.geojson) {
      notice.value =
        'This geometry has no coordinate reference system (SRID 0). Assign the correct SRID in your database before viewing it on a map.'
      return
    }
    const geometry = preview.geojson
    bounds = geometryBounds(geometry)
    if (!bounds) {
      notice.value = 'This geometry has no displayable coordinates.'
      return
    }
    const M = await import('maplibre-gl')
    if (disposed || !container.value) return
    M.setWorkerUrl(mapWorkerUrl)
    M.addProtocol(basemapProtocol, fetchBasemapResource)
    map = new M.Map({
      container: container.value,
      center: [0, 0],
      zoom: 2,
      maxZoom: 22,
      renderWorldCopies: false,
      attributionControl: false,
    })
    map.addControl(new M.NavigationControl({ showCompass: false }), 'top-left')
    map.addControl(new M.AttributionControl({ compact: false }), 'bottom-right')
    const backgrounds = mapBackgrounds(geometry)
    let background = 0
    map.on('error', (event) => {
      const sourceId = (event as { sourceId?: string }).sourceId
      if (sourceId === 'webdb-geometry' || offline.value) return
      const failed = background
      // Replace the style after MapLibre finishes dispatching the error.
      queueMicrotask(() => {
        if (disposed || background !== failed) return
        background += 1
        offline.value = background === backgrounds.length - 1
        map?.setStyle(backgrounds[background], { diff: false })
      })
    })
    map.on('webglcontextlost', () => {
      error.value = 'The map renderer stopped. Close and reopen the preview to try again.'
    })
    map.resize()
    fit()
    map.setStyle(backgrounds[background], { diff: false })
  } catch (cause) {
    if (disposed) return
    const failure = cause as { data?: { message?: string } }
    error.value =
      failure.data?.message ||
      'Could not load the map preview. Check that WebGL is available, reconnect, or try another geometry.'
  } finally {
    loading.value = false
  }
})
onScopeDispose(() => {
  disposed = true
  controller.abort()
  map?.remove()
})
</script>

<template>
  <dialog
    ref="dialog"
    class="geometry-dialog"
    :aria-label="`Map of ${column}`"
    @close="emit('close')"
    @click="$event.target === dialog && emit('close')"
  >
    <div class="geometry-heading">
      <div>
        <strong>{{ column }}</strong><span class="muted"
          >Geometry preview<span v-if="srid !== undefined"> · SRID {{ srid }}</span></span
        >
      </div>
      <button class="icon-button" aria-label="Close map" autofocus @click="emit('close')">
        <AppIcon name="close" :size="18" />
      </button>
    </div>
    <div class="geometry-toolbar">
      <button
        class="button secondary small"
        :disabled="loading || !!error || !!notice"
        @click="fit"
      >
        <AppIcon name="fit" :size="16" />Fit geometry
      </button>
    </div>
    <p class="geometry-caption muted">2D preview in WGS 84.</p>
    <div class="geometry-stage" :aria-busy="loading">
      <div ref="container" class="geometry-map" aria-label="Interactive geometry map" />
      <p
        v-if="loading || error || notice"
        class="geometry-message"
        :role="error ? 'alert' : 'status'"
      >
        {{ loading ? 'Loading geometry…' : error || notice }}
      </p>
    </div>
    <p v-if="offline && !error" class="geometry-caption" role="status">
      Basemap unavailable. Showing the geometry on a coordinate grid.
    </p>
  </dialog>
</template>

<style scoped>
.geometry-dialog {
  width: min(960px, calc(100vw - 32px));
  max-width: none;
  max-height: 92vh;
  overflow: auto;
  padding: 0;
  border: 1px solid var(--raised);
  border-radius: 10px;
  background: var(--sidebar);
  color: var(--text);
}
.geometry-dialog::backdrop {
  background: #0009;
}
.geometry-heading,
.geometry-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 16px 20px;
}
.geometry-heading > div {
  display: grid;
  gap: 4px;
}
.geometry-toolbar {
  flex-wrap: wrap;
  padding-block: 0;
}
.geometry-caption {
  margin: 12px 20px;
  font-size: 12px;
}
.geometry-stage {
  position: relative;
}
.geometry-map {
  height: min(60vh, 560px);
  min-height: 240px;
  background: #edf1ef;
}
.geometry-map :deep(.maplibregl-ctrl-attrib) {
  color: #253b33;
  background-color: #fff;
}
.geometry-message {
  position: absolute;
  inset: 0;
  display: grid;
  place-content: center;
  margin: 0;
  padding: 32px;
  text-align: center;
  background: #edf1ef;
  color: #253b33;
}
</style>
