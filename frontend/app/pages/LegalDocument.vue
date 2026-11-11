<script setup lang="ts">
import { computed } from 'vue'
import AppIcon from '../components/AppIcon.vue'

import { RouterLink } from 'vue-router'
import { legalDocuments, privacyConfig } from '../utils/privacy'
const props = defineProps<{ documentKey: 'privacy' | 'terms' }>()
const document = computed(() => legalDocuments[props.documentKey])
</script>

<template>
  <div class="legal-page">
    <header class="app-header">
      <RouterLink to="/" class="brand" aria-label="DBsaur home"
        ><span class="brand-icon"><AppIcon name="database" :size="20" /></span>DB<span
          class="brand-db"
          >saur</span
        ></RouterLink
      >
      <RouterLink to="/" class="button secondary small">Back to DBsaur</RouterLink>
    </header>
    <main class="legal-content">
      <h1>{{ document.title }}</h1>
      <p class="legal-date">Last updated: {{ privacyConfig.updated }}</p>
      <p class="legal-introduction">{{ document.introduction }}</p>
      <section v-for="[heading, body] in document.sections" :key="heading">
        <h2>{{ heading }}</h2>
        <p>{{ body }}</p>
      </section>
      <p>
        Contact: <a :href="`mailto:${privacyConfig.email}`">{{ privacyConfig.email }}</a>
      </p>
      <p>
        <a href="https://github.com/SteliosDaVitzi/web-db-tool">Source code and MIT license</a>
      </p>
      <p v-if="documentKey === 'privacy'" class="legal-sources">
        Learn more:
        <a
          href="https://www.edpb.europa.eu/sme/be-compliant/respect-individuals-rights_en"
          target="_blank"
          rel="noopener noreferrer"
          >GDPR rights (EDPB)</a
        >
        and
        <a
          href="https://europa.eu/youreurope/business/growing/digitalising/online-privacy/index_en.htm"
          target="_blank"
          rel="noopener noreferrer"
          >cookies and online privacy (Your Europe)</a
        >.
      </p>
    </main>
  </div>
</template>
