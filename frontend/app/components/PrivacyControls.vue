<script setup lang="ts">
import { onMounted, useTemplateRef } from 'vue'
import { useToast } from '../composables/useToast'
import AppIcon from './AppIcon.vue'

import { RouterLink } from 'vue-router'
import { privacyConfig, validPrivacyChoice } from '../utils/privacy'
const dialog = useTemplateRef<HTMLDialogElement>('dialog')
const toast = useToast()
let trigger: HTMLElement | null = null
function open(event?: MouseEvent) {
  trigger = event?.currentTarget instanceof HTMLElement ? event.currentTarget : null
  dialog.value?.showModal()
}
function acknowledge() {
  try {
    localStorage.setItem(
      privacyConfig.storageKey,
      JSON.stringify({
        version: privacyConfig.version,
        expires: Date.now() + privacyConfig.retentionDays * 86400000,
      }),
    )
  } catch {
    toast.show('Local app storage is unavailable. This notice may appear again next time.', 'info')
  }
  dialog.value?.close()
}
function clearChoice() {
  try {
    localStorage.removeItem(privacyConfig.storageKey)
    toast.show('Saved privacy notice preference removed.')
  } catch {
    toast.show(
      'Could not clear the notice preference. Check local app storage permissions.',
      'error',
    )
  }
}
onMounted(() => {
  try {
    if (validPrivacyChoice(localStorage.getItem(privacyConfig.storageKey))) return
    localStorage.removeItem(privacyConfig.storageKey)
  } catch {
    /* The notice remains usable when storage is blocked. */
  }
  // Legal pages remain directly readable without a blocking first-visit notice.
  if (location.pathname === '/') open()
})
</script>

<template>
  <footer class="app-footer">
    <span class="footer-credit"
      >DBsaur by
      <a class="footer-attribution" href="https://vijitek.com">
        <span class="attribution-logo" aria-hidden="true">
          <img
            src="../assets/images/vijitek-logo.webp"
            alt=""
            width="1280"
            height="1280"
            decoding="async"
          />
        </span>
        <span>Vijitek</span>
      </a>
    </span>
    <nav aria-label="Legal and privacy">
      <RouterLink to="/privacy">Privacy policy</RouterLink>
      <RouterLink to="/terms">Terms of use</RouterLink>
      <button type="button" @click="open">Privacy settings</button>
    </nav>
  </footer>
  <dialog
    ref="dialog"
    class="privacy-dialog"
    aria-labelledby="privacy-title"
    aria-describedby="privacy-description"
    @close="trigger?.focus()"
  >
    <span class="privacy-emblem"><AppIcon name="lock" :size="24" /></span>
    <h2 id="privacy-title">Your privacy in DBsaur</h2>
    <p id="privacy-description">
      DBsaur connects from its local desktop process to the database you choose. Connection details,
      queries, and results stay between your computer and that database.
    </p>
    <h3>Necessary storage only</h3>
    <p>
      We use a local session cookie to keep you connected. Queries are saved in the app only when
      you choose to save them. Saved connections are encrypted in your local application data
      directory.
    </p>
    <p>
      No analytics, advertising, or tracking services are included. There are no optional tracking
      cookies to accept.
    </p>
    <p>
      Closing this notice does not grant consent or waive your GDPR rights. “Got it” remembers this
      notice for 180 days in this app.
    </p>
    <div class="privacy-policy-links">
      <RouterLink to="/privacy" @click="dialog?.close()"
        >Privacy policy &amp; data rights</RouterLink
      >
      <RouterLink to="/terms" @click="dialog?.close()">Terms of use</RouterLink>
    </div>
    <div class="privacy-actions">
      <button class="text-button" type="button" @click="clearChoice">
        Clear saved notice choice
      </button>
      <button class="button" type="button" autofocus @click="acknowledge">Got it</button>
    </div>
  </dialog>
</template>
