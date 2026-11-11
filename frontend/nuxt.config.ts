export default defineNuxtConfig({
  ssr: false,
  nitro: { prerender: { crawlLinks: false, routes: ['/'] } },
  compatibilityDate: '2026-10-16',
  devtools: { enabled: false },
  telemetry: false,
  css: ['maplibre-gl/dist/maplibre-gl.css', '~/assets/main.css'],
  typescript: { strict: true },
  app: {
    head: {
      title: 'DBsaur — a quiet place for your data',
      meta: [
        {
          name: 'description',
          content: 'Browse PostgreSQL, MySQL, MariaDB, and SQL Server databases.',
        },
      ],
    },
  },
})
