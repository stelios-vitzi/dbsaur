import { createRouter, createWebHistory } from 'vue-router'
import DatabaseWorkspace from './components/DatabaseWorkspace.vue'
import LegalDocument from './pages/LegalDocument.vue'
import { legalDocuments } from './utils/privacy'

const appTitle = 'DBsaur — a quiet place for your data'
export const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', component: DatabaseWorkspace, meta: { title: appTitle } },
    ...(['privacy', 'terms'] as const).map((documentKey) => ({
      path: '/' + documentKey,
      component: LegalDocument,
      props: { documentKey },
      meta: { title: legalDocuments[documentKey].title + ' — DBsaur' },
    })),
    { path: '/:pathMatch(.*)*', redirect: '/' },
  ],
  scrollBehavior: () => ({ top: 0 }),
})
router.afterEach((to) => {
  document.title = String(to.meta.title ?? appTitle)
})
