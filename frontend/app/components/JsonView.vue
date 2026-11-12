<script setup lang="ts">
import { computed } from 'vue'
import { jsonLanguage } from '@codemirror/lang-json'
import { highlightTree, tagHighlighter, tags } from '@lezer/highlight'
const jsonHighlighter = tagHighlighter([
  { tag: tags.propertyName, class: 'tok-propertyName' },
  { tag: tags.string, class: 'tok-string' },
  { tag: tags.number, class: 'tok-number' },
  { tag: tags.bool, class: 'tok-bool' },
  { tag: tags.null, class: 'tok-null' },
])
const props = defineProps<{ value: unknown }>()
const tokens = computed(() => {
  const text = JSON.stringify(props.value, null, 2)
  const tokens: { text: string; className: string }[] = []
  let position = 0
  highlightTree(jsonLanguage.parser.parse(text), jsonHighlighter, (from, to, className) => {
    if (from > position) tokens.push({ text: text.slice(position, from), className: '' })
    tokens.push({ text: text.slice(from, to), className })
    position = to
  })
  if (position < text.length) tokens.push({ text: text.slice(position), className: '' })
  return tokens
})
</script>
<template>
  <pre
    class="json-view"
  ><code><span v-for="(token, index) in tokens" :key="index" :class="token.className">{{ token.text }}</span></code></pre>
</template>
<style scoped>
.json-view {
  font: 12px/1.7 var(--font-code);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  padding: 12px 16px;
  background: var(--canvas);
  user-select: text;
}
.tok-propertyName {
  color: var(--accent);
}
.tok-string {
  color: var(--green);
}
.tok-number {
  color: var(--amber);
}
.tok-bool,
.tok-null {
  color: var(--danger);
}
</style>
