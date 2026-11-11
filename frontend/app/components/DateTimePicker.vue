<script setup lang="ts">
import { computed, reactive, ref, watch, useTemplateRef } from 'vue'
import AppIcon from './AppIcon.vue'
import {
  daysInMonth,
  formatTemporal,
  parseTemporal,
  utcCalendarDate,
  validTemporal,
  type TemporalType,
  type TemporalParts,
} from '../utils/temporal'
const props = defineProps<{ modelValue: string; type: TemporalType; disabled?: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [value: string]; validity: [valid: boolean] }>()
const now = new Date()
const initial = parseTemporal(props.modelValue, props.type)
const parts = reactive<TemporalParts>(
  initial ?? {
    year: now.getFullYear(),
    month: now.getMonth() + 1,
    day: now.getDate(),
    hour: 0,
    minute: 0,
    second: 0,
    fraction: '',
    offset: props.type.zoned ? 'Z' : '',
    separator: 'T',
    seconds: true,
  },
)
const viewYear = ref(parts.year)
const viewMonth = ref(parts.month)
const root = useTemplateRef<HTMLElement>('root')
const monthNames = Array.from({ length: 12 }, (_, index) =>
  new Intl.DateTimeFormat(undefined, { month: 'long', timeZone: 'UTC' }).format(
    utcCalendarDate(2000, index + 1, 1),
  ),
)
const weekdays = ['Mo', 'Tu', 'We', 'Th', 'Fr', 'Sa', 'Su']
const calendar = computed(() => {
  if (!daysInMonth(viewYear.value, viewMonth.value)) return []
  const start = (utcCalendarDate(viewYear.value, viewMonth.value, 1).getUTCDay() + 6) % 7
  return Array.from({ length: 42 }, (_, index) => {
    const value = utcCalendarDate(viewYear.value, viewMonth.value, index - start + 1)
    return {
      year: value.getUTCFullYear(),
      month: value.getUTCMonth() + 1,
      day: value.getUTCDate(),
      label: value.toLocaleDateString(undefined, { timeZone: 'UTC', dateStyle: 'full' }),
    }
  })
})
const valid = computed(
  () => !!parseTemporal(props.modelValue, props.type) && validTemporal(parts, props.type),
)
watch(valid, (value) => emit('validity', value), { immediate: true })
watch(
  () => props.modelValue,
  (value) => {
    const parsed = parseTemporal(value, props.type)
    if (parsed) Object.assign(parts, parsed)
  },
)
function update() {
  if (validTemporal(parts, props.type)) emit('update:modelValue', formatTemporal(parts, props.type))
  else emit('validity', false)
}
function selectDate(date: { year: number; month: number; day: number }) {
  Object.assign(parts, date)
  viewYear.value = date.year
  viewMonth.value = date.month
  update()
}
function moveMonth(amount: number) {
  const date = utcCalendarDate(viewYear.value, viewMonth.value + amount, 1)
  if (date.getUTCFullYear() < 1 || date.getUTCFullYear() > 9999) return
  viewYear.value = date.getUTCFullYear()
  viewMonth.value = date.getUTCMonth() + 1
}
function calendarKey(event: KeyboardEvent, date: { year: number; month: number; day: number }) {
  const delta = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -7, ArrowDown: 7 }[event.key]
  if (delta === undefined) return
  event.preventDefault()
  const next = utcCalendarDate(date.year, date.month, date.day + delta)
  selectDate({ year: next.getUTCFullYear(), month: next.getUTCMonth() + 1, day: next.getUTCDate() })
  requestAnimationFrame(focus)
}
function selected(date: { year: number; month: number; day: number }) {
  return date.year === parts.year && date.month === parts.month && date.day === parts.day
}
function focus() {
  ;(
    root.value?.querySelector('[data-selected="true"]') ?? root.value?.querySelector('input')
  )?.scrollIntoView({ block: 'nearest' })
  ;(
    root.value?.querySelector<HTMLElement>('[data-selected="true"]') ??
    root.value?.querySelector('input')
  )?.focus()
}
defineExpose({ focus })
</script>

<template>
  <div ref="root" class="date-time-picker">
    <section v-if="type.kind !== 'time'" class="date-calendar" aria-label="Choose a date">
      <div class="date-calendar-heading">
        <button
          type="button"
          class="icon-button date-previous"
          aria-label="Previous month"
          :disabled="disabled"
          @click="moveMonth(-1)"
        >
          <AppIcon name="chevron" :size="18" />
        </button>
        <select v-model.number="viewMonth" aria-label="Month" :disabled="disabled">
          <option v-for="(month, index) in monthNames" :key="month" :value="index + 1">
            {{ month }}
          </option>
        </select>
        <input
          v-model.number="viewYear"
          type="number"
          min="1"
          max="9999"
          aria-label="Year"
          :disabled="disabled"
        />
        <button
          type="button"
          class="icon-button"
          aria-label="Next month"
          :disabled="disabled"
          @click="moveMonth(1)"
        >
          <AppIcon name="chevron" :size="18" />
        </button>
      </div>
      <div class="date-weekdays" aria-hidden="true">
        <span v-for="day in weekdays" :key="day">{{ day }}</span>
      </div>
      <div class="date-days" role="grid" aria-label="Calendar days">
        <button
          v-for="date in calendar"
          :key="date.year + '-' + date.month + '-' + date.day"
          type="button"
          role="gridcell"
          :aria-label="date.label"
          :aria-selected="selected(date)"
          :data-selected="selected(date)"
          :tabindex="
            selected(date) ||
            (!calendar.some(selected) && date.day === 1 && date.month === viewMonth)
              ? 0
              : -1
          "
          :class="{ outside: date.month !== viewMonth, selected: selected(date) }"
          :disabled="disabled || date.year < 1 || date.year > 9999"
          @click="selectDate(date)"
          @keydown="calendarKey($event, date)"
        >
          {{ date.day }}
        </button>
      </div>
      <button
        type="button"
        class="text-button date-today"
        :disabled="disabled"
        @click="
          selectDate({ year: now.getFullYear(), month: now.getMonth() + 1, day: now.getDate() })
        "
      >
        Today
      </button>
    </section>
    <section v-if="type.kind !== 'date'" class="date-time-fields" aria-label="Choose a time">
      <div class="date-time-clock">
        <label
          >Hour<input
            v-model.number="parts.hour"
            type="number"
            min="0"
            max="23"
            :disabled="disabled"
            @input="update"
        /></label>
        <span>:</span
        ><label
          >Minute<input
            v-model.number="parts.minute"
            type="number"
            min="0"
            max="59"
            :disabled="disabled"
            @input="update"
        /></label>
        <span>:</span
        ><label
          >Second<input
            v-model.number="parts.second"
            type="number"
            min="0"
            max="59"
            :disabled="disabled"
            @input="update"
        /></label>
      </div>
      <div class="date-time-precision">
        <label
          >Fractional seconds<input
            v-model="parts.fraction"
            inputmode="numeric"
            maxlength="9"
            placeholder="000000"
            :disabled="disabled"
            @input="update" /></label
        ><label v-if="type.zoned || parts.offset"
          >UTC offset<input
            v-model="parts.offset"
            placeholder="Z or +02:00"
            maxlength="6"
            :disabled="disabled"
            @input="update"
        /></label>
      </div>
      <p v-if="type.zoned || parts.offset" class="date-time-note">
        The UTC offset is preserved. No timezone conversion is applied.
      </p>
    </section>
    <p v-if="!valid" class="date-time-invalid" role="status">
      Choose a valid date and time, or edit the text value.
    </p>
  </div>
</template>

<style scoped>
.date-time-picker {
  display: grid;
  gap: 14px;
  padding: 14px;
  border: 1px solid var(--raised);
  border-radius: 6px;
  background: var(--canvas);
  font-size: var(--text-sm);
}
.date-calendar-heading {
  display: flex;
  gap: 7px;
  align-items: center;
  margin-bottom: 12px;
}
.date-calendar-heading select {
  flex: 1;
  min-width: 0;
}
.date-calendar-heading > input {
  width: 75px;
}
.date-previous svg {
  transform: rotate(180deg);
}
.date-weekdays,
.date-days {
  display: grid;
  grid-template-columns: repeat(7, minmax(0, 1fr));
  gap: 3px;
  text-align: center;
}
.date-weekdays {
  color: var(--muted);
  margin-bottom: 5px;
  font-size: var(--text-xs);
}
.date-days button {
  min-width: 0;
  min-height: 30px;
  border-radius: 4px;
  color: var(--secondary);
  font-size: inherit;
}
.date-days button:hover:not(:disabled) {
  background: var(--surface-hover);
  color: var(--text);
}
.date-days button.outside {
  color: var(--muted);
  opacity: 0.55;
}
.date-days button.selected {
  background: var(--accent-soft);
  color: var(--accent);
  box-shadow: inset 0 0 0 1px var(--accent);
  opacity: 1;
}
.date-today {
  margin-top: 6px;
  font-size: inherit;
  color: var(--accent);
}
.date-time-fields {
  padding-top: 12px;
  border-top: 1px solid var(--raised);
}
.date-time-fields:first-child {
  padding-top: 0;
  border: 0;
}
.date-time-clock {
  display: grid;
  grid-template-columns: 1fr auto 1fr auto 1fr;
  align-items: end;
  gap: 7px;
}
.date-time-clock > span {
  padding-bottom: 7px;
  color: var(--muted);
}
.date-time-picker label {
  display: grid;
  gap: 5px;
  min-width: 0;
  color: var(--muted);
  font-size: var(--text-xs);
}
.date-time-picker input,
.date-time-picker select {
  min-width: 0;
  width: 100%;
  height: 32px;
  padding: 5px 7px;
  border: 1px solid var(--raised);
  border-radius: 4px;
  color: var(--text);
  background: var(--sidebar);
  font-size: var(--text-sm);
}
.date-time-precision {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
  margin-top: 12px;
}
.date-time-note,
.date-time-invalid {
  color: var(--muted);
  margin-top: 9px;
  line-height: 1.5;
  font-size: var(--text-xs);
}
.date-time-invalid {
  color: var(--amber);
  margin-top: 0;
}
</style>
