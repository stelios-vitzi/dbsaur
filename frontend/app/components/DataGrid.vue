<script setup lang="ts">
import { computed, ref, watch, useId } from 'vue'
import AppIcon from './AppIcon.vue'
import GeometryMap from './GeometryMap.vue'

import type { TablePage } from '../../shared/database'
import { isSpatialType } from '../utils/geometry'
const props = defineProps<{
  data: TablePage
  sort?: string
  descending: boolean
  sortable?: boolean
  selectedRow?: number | null
}>()
const emit = defineEmits<{
  select: [index: number]
  edit: [index: number, column: string]
  sort: [column: string]
  context: [event: MouseEvent, index: number, column?: string]
}>()
const selectedCell = ref<{ row: number; column: string | null } | null>(null)
const headerId = useId()
const geometry = ref<{ value: string; column: string } | null>(null)
const columnWidths = ref<Record<number, number>>({})
const resize = ref<{ index: number; startX: number; startWidth: number } | null>(null)
const minimumColumnWidth = 72
const maximumColumnWidth = 720
const textMeasure = document.createElement('canvas').getContext('2d')!
const automaticColumnWidths = computed(() => {
  const font = getComputedStyle(document.documentElement).getPropertyValue('--font-ui')
  textMeasure.font = `500 12px ${font}`
  const headings = props.data.columns.map(
    (column) => textMeasure.measureText(column.name).width + 28 + 18 + (column.primaryKey ? 18 : 0),
  )
  textMeasure.font = `400 12px ${font}`
  return props.data.columns.map((column, index) => {
    if (isSpatialType(column.dataType)) return 160
    let width = headings[index] ?? minimumColumnWidth
    for (const row of props.data.rows) {
      if (width >= 160) break
      const value = row[column.name]
      width = Math.max(
        width,
        textMeasure.measureText(display(value)).width + 28 + (typeof value === 'boolean' ? 12 : 0),
      )
    }
    return Math.ceil(Math.max(minimumColumnWidth, Math.min(160, width)))
  })
})
const widths = computed(() =>
  props.data.columns.map(
    (_, index) => columnWidths.value[index] ?? automaticColumnWidths.value[index] ?? 160,
  ),
)
watch(
  () => props.data,
  () => {
    selectedCell.value = null
    geometry.value = null
  },
)
function display(value: unknown) {
  return value === null ? 'NULL' : typeof value === 'object' ? JSON.stringify(value) : String(value)
}
function startResize(event: PointerEvent, index: number) {
  if (event.button !== 0) return
  const handle = event.currentTarget as HTMLElement
  const cell = handle.parentElement
  if (!cell) return
  event.preventDefault()
  event.stopPropagation()
  handle.setPointerCapture(event.pointerId)
  resize.value = {
    index,
    startX: event.clientX,
    startWidth: columnWidths.value[index] || cell.getBoundingClientRect().width,
  }
}
function moveResize(event: PointerEvent) {
  if (!resize.value) return
  const { index, startX, startWidth } = resize.value
  columnWidths.value[index] = Math.min(
    maximumColumnWidth,
    Math.max(minimumColumnWidth, startWidth + event.clientX - startX),
  )
}
function stopResize() {
  resize.value = null
}
function keyResize(event: KeyboardEvent, index: number) {
  if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return
  event.preventDefault()
  event.stopPropagation()
  const cell = (event.currentTarget as HTMLElement).parentElement
  const current =
    columnWidths.value[index] || cell?.getBoundingClientRect().width || widths.value[index] || 160
  const step = event.shiftKey ? 48 : 16
  columnWidths.value[index] =
    event.key === 'Home'
      ? minimumColumnWidth
      : event.key === 'End'
        ? maximumColumnWidth
        : Math.min(
            maximumColumnWidth,
            Math.max(minimumColumnWidth, current + (event.key === 'ArrowRight' ? step : -step)),
          )
}
</script>
<template>
  <div class="grid-scroll" tabindex="0" aria-label="Table data, scroll to see more columns">
    <table
      class="data-grid"
      :style="{
        width: `${58 + widths.reduce((total, width) => total + width, 0)}px`,
      }"
    >
      <colgroup>
        <col class="row-number-column" />
        <col
          v-for="(column, index) in data.columns"
          :key="column.name + '-width'"
          :style="{ width: widths[index] + 'px' }"
        />
      </colgroup>
      <thead>
        <tr>
          <th class="row-number">#</th>
          <th
            v-for="(column, index) in data.columns"
            :key="column.name"
            :aria-sort="sort === column.name ? (descending ? 'descending' : 'ascending') : 'none'"
          >
            <button
              :aria-label="sortable === false ? column.name : `Sort by ${column.name}`"
              :aria-describedby="`${headerId}-type-${index}`"
              :aria-disabled="sortable === false"
              @click="sortable !== false && emit('sort', column.name)"
            >
              <span class="column-label"
                ><AppIcon v-if="column.primaryKey" name="key" :size="12" /><span
                  class="column-name"
                  >{{ column.name }}</span
                ><AppIcon
                  v-if="sort === column.name"
                  name="down"
                  :size="12"
                  :class="{ flipped: !descending }" /></span
              ><span :id="`${headerId}-type-${index}`" class="column-type" role="tooltip">{{
                column.dataType
              }}</span>
            </button>
            <span
              class="column-resizer"
              role="separator"
              tabindex="0"
              aria-orientation="vertical"
              :aria-label="`Resize ${column.name} column`"
              :aria-valuemin="minimumColumnWidth"
              :aria-valuemax="maximumColumnWidth"
              :aria-valuenow="widths[index]"
              title="Drag to resize. Arrow keys adjust width."
              @pointerdown="startResize($event, index)"
              @pointermove="moveResize"
              @pointerup="stopResize"
              @pointercancel="stopResize"
              @lostpointercapture="stopResize"
              @keydown="keyResize($event, index)"
            />
          </th>
        </tr>
      </thead>
      <tbody>
        <template v-for="(row, index) in data.rows" :key="index"
          ><tr
            :class="{ 'selected-row': selectedRow === index }"
            :aria-selected="selectedRow === index"
            @click="emit('select', index)"
            @contextmenu.prevent.stop="emit('context', $event, index)"
          >
            <td
              class="row-number"
              :class="{
                'selected-cell': selectedCell?.row === index && selectedCell.column === null,
              }"
              tabindex="0"
              title="Select row"
              @focus="selectedCell = { row: index, column: null }"
              @click="selectedCell = { row: index, column: null }"
              @keydown.enter.prevent="emit('select', index)"
            >
              {{ data.page * data.pageSize + index + 1 }}
            </td>
            <td
              v-for="column in data.columns"
              :key="column.name"
              :class="{
                'null-cell': row[column.name] === null,
                'boolean-cell': typeof row[column.name] === 'boolean',
                'selected-cell': selectedCell?.row === index && selectedCell.column === column.name,
              }"
              :title="display(row[column.name])"
              tabindex="0"
              @focus="selectedCell = { row: index, column: column.name }"
              @click="selectedCell = { row: index, column: column.name }"
              @contextmenu.prevent.stop="emit('context', $event, index, column.name)"
              @dblclick.stop="emit('edit', index, column.name)"
              @keydown.enter.prevent="emit('edit', index, column.name)"
            >
              <span
                v-if="isSpatialType(column.dataType) && typeof row[column.name] === 'string'"
                class="geometry-cell"
              >
                <span class="geometry-cell-preview">{{ display(row[column.name]) }}</span>
                <button
                  type="button"
                  class="icon-button geometry-cell-map"
                  title="View on map"
                  :aria-label="`View ${column.name} on map, row ${data.page * data.pageSize + index + 1}`"
                  @click.stop="geometry = { value: String(row[column.name]), column: column.name }"
                  @dblclick.stop
                  @keydown.enter.stop
                >
                  <AppIcon name="map" :size="18" />
                </button>
              </span>
              <span
                v-else-if="typeof row[column.name] === 'boolean'"
                class="boolean-value"
                :class="{ truthy: row[column.name] }"
                ><span class="tiny-dot" />{{ row[column.name] }}</span
              ><span v-else>{{ display(row[column.name]) }}</span>
            </td>
          </tr></template
        >
      </tbody>
    </table>
    <div v-if="!data.rows.length" class="grid-empty">
      <AppIcon name="table" :size="24" />
      <h3>This table is empty</h3>
      <p>Right-click the table in the explorer to view its structure.</p>
    </div>
  </div>
  <GeometryMap
    v-if="geometry"
    :value="geometry.value"
    :column="geometry.column"
    @close="geometry = null"
  />
</template>

<style scoped>
.geometry-cell {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.geometry-cell-preview {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}
.geometry-cell-map {
  flex: none;
  width: 26px;
  height: 26px;
  padding: 4px;
  color: var(--accent);
}
</style>
