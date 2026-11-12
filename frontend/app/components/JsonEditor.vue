<script setup lang="ts">
import { onMounted, onBeforeUnmount, watch, useTemplateRef } from 'vue'
import { Compartment, EditorState } from '@codemirror/state'
import { EditorView, keymap, lineNumbers } from '@codemirror/view'
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands'
import { bracketMatching, HighlightStyle, syntaxHighlighting } from '@codemirror/language'
import { closeBrackets, closeBracketsKeymap } from '@codemirror/autocomplete'
import { json } from '@codemirror/lang-json'
import { tags } from '@lezer/highlight'

const props = withDefaults(
  defineProps<{
    modelValue: string
    label: string
    disabled?: boolean
    readOnly?: boolean
    height?: number
  }>(),
  { height: 100 },
)
const emit = defineEmits<{ 'update:modelValue': [value: string]; run: [] }>()
const host = useTemplateRef<HTMLDivElement>('host')
const editing = new Compartment()
let view: EditorView | undefined
const editingState = () => [
  EditorState.readOnly.of(props.disabled || props.readOnly),
  EditorView.editable.of(!props.disabled && !props.readOnly),
]
onMounted(() => {
  view = new EditorView({
    parent: host.value!,
    state: EditorState.create({
      doc: props.modelValue,
      extensions: [
        json(),
        lineNumbers(),
        history(),
        bracketMatching(),
        closeBrackets(),
        editing.of(editingState()),
        EditorState.tabSize.of(2),
        EditorView.lineWrapping,
        keymap.of([
          {
            key: 'Mod-Enter',
            run: () => {
              if (!props.disabled && !props.readOnly) emit('run')
              return true
            },
          },
          ...closeBracketsKeymap,
          ...defaultKeymap,
          ...historyKeymap,
          indentWithTab,
        ]),
        EditorView.contentAttributes.of({ 'aria-label': props.label, spellcheck: 'false' }),
        EditorView.theme({
          '&': { backgroundColor: 'var(--canvas)', color: 'var(--text)', height: '100%' },
          '&.cm-focused': { outline: '2px solid var(--accent)', outlineOffset: '2px' },
          '.cm-scroller': {
            fontFamily: 'var(--font-code)',
            fontSize: '12px',
            overflow: 'auto',
            lineHeight: '1.7',
          },
          '.cm-content': { caretColor: 'var(--text)' },
          '.cm-gutters': {
            backgroundColor: 'var(--canvas)',
            color: 'var(--muted)',
            border: 'none',
          },
          '.cm-cursor': { borderLeftColor: 'var(--text)' },
          '.cm-selectionBackground, &.cm-focused .cm-selectionBackground': {
            backgroundColor: 'var(--raised)',
          },
        }),
        syntaxHighlighting(
          HighlightStyle.define([
            { tag: tags.propertyName, color: 'var(--accent)' },
            { tag: tags.string, color: 'var(--green)' },
            { tag: tags.number, color: 'var(--amber)' },
            { tag: [tags.bool, tags.null], color: 'var(--danger)' },
          ]),
        ),
        EditorState.transactionFilter.of((transaction) =>
          transaction.docChanged && transaction.newDoc.length > 1024 * 1024 ? [] : transaction,
        ),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) emit('update:modelValue', update.state.doc.toString())
        }),
      ],
    }),
  })
})
watch(
  () => props.modelValue,
  (value) => {
    if (view && view.state.doc.toString() !== value)
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: value } })
  },
)
watch(
  () => [props.disabled, props.readOnly],
  () => view?.dispatch({ effects: editing.reconfigure(editingState()) }),
)
onBeforeUnmount(() => view?.destroy())
</script>

<template>
  <div ref="host" class="json-editor" :style="{ height: `${height}px` }" />
</template>

<style scoped>
.json-editor {
  min-width: 0;
  border: 1px solid var(--raised);
  border-radius: 4px;
  overflow: hidden;
}
</style>
