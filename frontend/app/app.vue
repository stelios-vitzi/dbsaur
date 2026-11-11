<script setup lang="ts">
import { RouterView, useRoute } from 'vue-router'
import PrivacyControls from './components/PrivacyControls.vue'
import ToastHost from './components/ToastHost.vue'
import DesktopUpdater from './components/DesktopUpdater.vue'
const route = useRoute()
</script>

<template>
  <div class="app-shell">
    <p class="route-announcement" role="status" aria-live="polite">{{ route.meta.title }}</p>
    <RouterView v-slot="{ Component }">
      <KeepAlive include="DatabaseWorkspace">
        <component :is="Component" />
      </KeepAlive>
    </RouterView>
    <PrivacyControls />
    <ToastHost />
    <DesktopUpdater />
  </div>
</template>

<style scoped>
.route-announcement {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
  border: 0;
}
</style>
