<script setup lang="ts">
import { computed, useId } from 'vue'

import type { DatabaseSetting } from '../../shared/database'
const props = defineProps<{ setting: DatabaseSetting; value: string | null; disabled: boolean }>()
const emit = defineEmits<{ 'update:value': [value: string | null] }>()
const id = useId()
const label = computed(() => props.setting.name.replaceAll('_', ' '))
const effective = computed(() => props.setting.setting ?? '')
const booleanValue = computed(() =>
  ['on', 'true', '1'].includes(props.value ?? effective.value) ? 'on' : 'off',
)
const reason = computed(() => {
  if (props.setting.setting === '[redacted]')
    return 'Sensitive setting. Manage through your database administrator.'
  if (props.setting.context === 'postmaster') return 'Server configuration · restart required'
  if (props.setting.context === 'sighup') return 'Server configuration · reload required'
  if (props.setting.context === 'internal') return 'Reported by PostgreSQL · cannot be changed'
  return 'Requires additional privileges or cannot be configured for a database.'
})
</script>
<template>
  <div class="setting-field">
    <div class="setting-field-heading">
      <label :for="id">{{ label }}</label><span v-if="setting.unit" class="setting-unit">{{
        setting.unit
      }}</span>
    </div>
    <p :id="`${id}-help`">{{ setting.short_desc }}</p>
    <template v-if="setting.editable">
      <label class="setting-override"
        ><input
          type="checkbox"
          :checked="value !== null"
          :disabled="disabled"
          @change="
            emit('update:value', ($event.target as HTMLInputElement).checked ? effective : null)
          "
        />Override for this database</label
      >
      <select
        v-if="setting.vartype === 'bool'"
        :id="id"
        :value="booleanValue"
        :disabled="disabled || value === null"
        :aria-describedby="`${id}-help`"
        @change="emit('update:value', ($event.target as HTMLSelectElement).value)"
      >
        <option value="on">Enabled</option>
        <option value="off">Disabled</option>
      </select>
      <select
        v-else-if="setting.enumvals"
        :id="id"
        :value="value ?? effective"
        :disabled="disabled || value === null"
        :aria-describedby="`${id}-help`"
        @change="emit('update:value', ($event.target as HTMLSelectElement).value)"
      >
        <option v-for="option in setting.enumvals" :key="option" :value="option">
          {{ option }}
        </option>
      </select>
      <input
        v-else
        :id="id"
        type="text"
        :value="value ?? effective"
        :disabled="disabled || value === null"
        :aria-describedby="`${id}-help`"
        :maxlength="4096"
        @input="emit('update:value', ($event.target as HTMLInputElement).value)"
      />
      <small
        >Effective: {{ effective }} {{ setting.unit || ''
        }}<template v-if="setting.min_val !== null">
          · Range {{ setting.min_val }}–{{ setting.max_val }}</template
        ></small
      >
    </template>
    <template v-else
      ><input :id="id" :value="effective" disabled :aria-describedby="`${id}-help`" /><small>{{
        reason
      }}</small></template
    >
    <code>{{ setting.name }}</code>
  </div>
</template>
