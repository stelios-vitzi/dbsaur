<script setup lang="ts">
import { watch, onMounted, onBeforeUnmount, useTemplateRef } from 'vue'

import { Compartment, EditorState, Prec } from '@codemirror/state'
import { EditorView, keymap, lineNumbers, drawSelection, tooltips } from '@codemirror/view'
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands'
import { bracketMatching, HighlightStyle, syntaxHighlighting } from '@codemirror/language'
import {
  autocompletion,
  acceptCompletion,
  completionKeymap,
  closeBrackets,
  closeBracketsKeymap,
} from '@codemirror/autocomplete'
import { sql as sqlLanguage } from '@codemirror/lang-sql'
import { providerOptions } from '../providers'
import { tags } from '@lezer/highlight'
import type { TableInfo } from '../../shared/database'
import { databaseCompletion, providerSnippets } from '../utils/sqlCompletion'

const props = defineProps<{
  provider?: string
  modelValue: string
  height: number
  tables: TableInfo[]
  selectedTable: TableInfo | null
}>()
const emit = defineEmits<{ 'update:modelValue': [value: string]; run: [] }>()
const host = useTemplateRef<HTMLDivElement>('host')
const language = new Compartment()
let view: EditorView | undefined
let tooltipLayer: HTMLDivElement | undefined
function languageSupport() {
  const dialect = providerOptions(props.provider ?? 'postgres').dialect
  return [
    sqlLanguage({ dialect, upperCaseKeywords: true }),
    dialect.language.data.of({
      autocomplete: databaseCompletion(props.tables, props.selectedTable, props.provider),
    }),
    dialect.language.data.of({ autocomplete: providerSnippets(props.provider) }),
  ]
}
onMounted(() => {
  tooltipLayer = document.createElement('div')
  tooltipLayer.className = 'sql-tooltip-layer'
  document.body.appendChild(tooltipLayer)
  view = new EditorView({
    parent: host.value!,
    state: EditorState.create({
      doc: props.modelValue,
      extensions: [
        lineNumbers(),
        history(),
        drawSelection(),
        bracketMatching(),
        closeBrackets(),
        autocompletion(),
        // The overlay owns stacking; CodeMirror keeps tooltips anchored on scroll.
        tooltips({ parent: tooltipLayer, position: 'fixed' }),
        language.of(languageSupport()),
        Prec.highest(
          keymap.of([
            {
              key: 'Mod-Enter',
              run: () => {
                emit('run')
                return true
              },
            },
            { key: 'Tab', run: acceptCompletion },
          ]),
        ),
        keymap.of([
          ...completionKeymap,
          ...closeBracketsKeymap,
          ...defaultKeymap,
          ...historyKeymap,
          indentWithTab,
        ]),
        EditorState.tabSize.of(2),
        EditorView.contentAttributes.of({
          id: 'sql-editor',
          'aria-label': 'SQL query',
          spellcheck: 'false',
          autocapitalize: 'off',
          autocomplete: 'off',
        }),
        EditorView.theme({
          '&.cm-editor': {
            height: '100%',
            backgroundColor: 'var(--query-surface)',
            color: 'var(--text)',
          },
          '&.cm-focused': { outline: 'none', boxShadow: 'inset 2px 0 var(--accent)' },
          '.cm-scroller': {
            overflow: 'auto',
            fontFamily: 'var(--font-code)',
            fontSize: 'var(--text-control)',
            lineHeight: '20px',
          },
          '.cm-content': { padding: '12px 0', caretColor: 'var(--text)' },
          '.cm-line': { padding: '0 24px 0 0' },
          '.cm-gutters': {
            minWidth: '48px',
            border: 'none',
            backgroundColor: 'var(--query-surface)',
            color: 'var(--muted)',
          },
          '.cm-lineNumbers .cm-gutterElement': { padding: '0 12px 0 8px' },
          '.cm-cursor': { borderLeftColor: 'var(--text)' },
          '&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection': {
            backgroundColor: 'var(--raised)',
          },
          '.cm-tooltip': {
            backgroundColor: 'var(--sidebar)',
            color: 'var(--text)',
            border: '1px solid var(--raised)',
          },
          '.cm-tooltip-autocomplete ul li[aria-selected]': {
            backgroundColor: 'var(--accent-soft)',
            color: 'var(--text)',
          },
          '.cm-completionDetail': { color: 'var(--muted)', marginLeft: '12px' },
          '.cm-matchingBracket': {
            backgroundColor: 'var(--raised)',
            outline: '1px solid var(--muted)',
          },
        }),
        syntaxHighlighting(
          HighlightStyle.define([
            { tag: tags.keyword, color: '#a994ef' },
            { tag: [tags.string, tags.special(tags.string)], color: '#97c797' },
            { tag: [tags.number, tags.bool, tags.null], color: '#e5b17d' },
            { tag: tags.comment, color: '#8a9694', fontStyle: 'italic' },
            { tag: [tags.name, tags.typeName], color: '#87c7da' },
            { tag: [tags.operator, tags.punctuation], color: '#c1cbc7' },
          ]),
        ),
        EditorState.transactionFilter.of((transaction) =>
          transaction.docChanged && transaction.newDoc.length > 12_000 ? [] : transaction,
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
    if (view && view.state.doc.toString() !== value) {
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: value } })
    }
  },
)
watch(
  () => [props.tables, props.selectedTable, props.provider],
  () => {
    view?.dispatch({ effects: language.reconfigure(languageSupport()) })
  },
)
onBeforeUnmount(() => {
  view?.destroy()
  tooltipLayer?.remove()
})
defineExpose({ focus: () => view?.focus() })
</script>

<template>
  <div ref="host" class="sql-editor-host" :style="{ height: `${height}px` }" />
</template>
