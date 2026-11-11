<script setup lang="ts">
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
    toast.show('Browser storage is unavailable. This notice may appear again next time.', 'info')
  }
  dialog.value?.close()
}
function clearChoice() {
  try {
    localStorage.removeItem(privacyConfig.storageKey)
    toast.show('Saved privacy notice preference removed.')
  } catch {
    toast.show('Could not clear the notice preference. Check browser storage permissions.', 'error')
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
            src="~/assets/images/vijitek-logo.webp"
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
      <NuxtLink to="/privacy">Privacy policy</NuxtLink>
      <NuxtLink to="/terms">Terms of use</NuxtLink>
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
      DBsaur connects through its backend to the database you choose. Connection details, queries,
      and results pass through that server.
    </p>
    <h3>Necessary storage only</h3>
    <p>
      We use a session cookie to keep you connected. Queries are saved in this browser only when you
      choose to save them. Saved connections, when enabled, are encrypted on the server.
    </p>
    <p>
      No analytics, advertising, or tracking services are included. There are no optional tracking
      cookies to accept.
    </p>
    <p>
      Closing this notice does not grant consent or waive your GDPR rights. “Got it” remembers this
      notice for 180 days on this browser.
    </p>
    <div class="privacy-policy-links">
      <NuxtLink to="/privacy" @click="dialog?.close()">Privacy policy &amp; data rights</NuxtLink>
      <NuxtLink to="/terms" @click="dialog?.close()">Terms of use</NuxtLink>
    </div>
    <div class="privacy-actions">
      <button class="text-button" type="button" @click="clearChoice">
        Clear saved notice choice
      </button>
      <button class="button" type="button" autofocus @click="acknowledge">Got it</button>
    </div>
  </dialog>
</template>
