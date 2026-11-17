<script setup lang="ts">
import { ref, watch, onMounted, onScopeDispose, useTemplateRef } from 'vue'
import { $fetch } from '../services/http'
import AppIcon from './AppIcon.vue'

import type { Map as MapLibreMap, StyleSpecification } from 'maplibre-gl'
import type { Geometry } from 'geojson'
import mapWorkerUrl from 'maplibre-gl/dist/maplibre-gl-worker.mjs?url'
import { geometryBounds, geometryMapStyle, type GeometryBounds } from '../utils/geometryMap'
import { loadMapPreferences } from '../utils/mapPreferences'

const props = defineProps<{ value: string; column: string }>()
const emit = defineEmits<{ close: [] }>()
const dialog = useTemplateRef<HTMLDialogElement>('dialog')
const container = useTemplateRef<HTMLDivElement>('container')
const loading = ref(true)
const error = ref('')
const notice = ref('')
const srid = ref<number>()
const preferences = loadMapPreferences()
const background = ref(preferences.enabled && !!preferences.url)
const tileError = ref(false)
let map: MapLibreMap | undefined
let geometry: Geometry | undefined
let bounds: GeometryBounds | null = null
let basemap: StyleSpecification | undefined
let disposed = false
const controller = new AbortController()
let backgroundVersion = 0

function fit() {
  if (bounds) map?.fitBounds(bounds, { padding: 32, maxZoom: 16, duration: 0 })
}
async function updateBackground() {
  if (!map || !geometry) return
  const version = ++backgroundVersion
  if (!background.value) {
    map.setStyle(geometryMapStyle(geometry), { diff: false })
    return
  }
  tileError.value = false
  try {
    if (!preferences.enabled || !preferences.url) return
    basemap ??= preferences.url.includes('{z}')
      ? {
          version: 8,
          sources: {
            basemap: {
              type: 'raster',
              tiles: [preferences.url],
              tileSize: 256,
              attribution: preferences.attribution
                .replaceAll('&', '&amp;')
                .replaceAll('<', '&lt;')
                .replaceAll('>', '&gt;'),
            },
          },
          layers: [{ id: 'basemap', type: 'raster', source: 'basemap' }],
        }
      : await $fetch<StyleSpecification>(preferences.url, { signal: controller.signal })
    if (!disposed && version === backgroundVersion && background.value)
      map.setStyle(geometryMapStyle(geometry, basemap), { diff: false })
  } catch {
    if (!disposed && version === backgroundVersion) tileError.value = true
  }
}
watch(background, updateBackground)
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
    geometry = preview.geojson
    bounds = geometryBounds(geometry)
    if (!bounds) {
      notice.value = 'This geometry has no displayable coordinates.'
      return
    }
    const M = await import('maplibre-gl')
    if (disposed || !container.value) return
    M.setWorkerUrl(mapWorkerUrl)
    map = new M.Map({
      container: container.value,
      style: geometryMapStyle(geometry),
      center: [0, 0],
      zoom: 2,
      maxZoom: 22,
      renderWorldCopies: false,
      attributionControl: false,
    })
    map.addControl(new M.NavigationControl({ showCompass: false }), 'top-left')
    map.addControl(new M.AttributionControl({ compact: true }), 'bottom-right')
    map.on('error', () => {
      if (background.value) {
        tileError.value = true
        background.value = false
      }
    })
    map.on('webglcontextlost', () => {
      error.value = 'The map renderer stopped. Close and reopen the preview to try again.'
    })
    map.resize()
    fit()
    await updateBackground()
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
      <label
        ><input
          v-model="background"
          type="checkbox"
          :disabled="loading || !!error || !!notice || !preferences.enabled || !preferences.url"
        />Map background</label
      >
    </div>
    <p class="geometry-caption muted">
      2D preview in WGS 84. Configure optional map backgrounds in Privacy settings.
    </p>
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
    <p v-if="tileError" class="geometry-caption" role="status">
      Map background could not load. The geometry is still available offline.
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
.geometry-toolbar label {
  display: flex;
  align-items: center;
  gap: 8px;
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
