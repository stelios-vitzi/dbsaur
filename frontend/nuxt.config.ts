import process from 'node:process'

const desktop = process.env.DBSAUR_DESKTOP === '1'

export default defineNuxtConfig({
  ssr: !desktop,
  ...(desktop ? { nitro: { prerender: { crawlLinks: false, routes: ['/'] } } } : {}),
  compatibilityDate: '2026-10-16',
  devtools: { enabled: false },
  telemetry: false,
  devServer: { port: 34616 },
  css: ['maplibre-gl/dist/maplibre-gl.css', '~/assets/main.css'],
  typescript: { strict: true },
  runtimeConfig: { apiBase: 'http://127.0.0.1:34617' },
  routeRules: {
    '/**': {
      headers: {
        'X-Content-Type-Options': 'nosniff',
        'X-Frame-Options': 'DENY',
        'Referrer-Policy': 'no-referrer',
        'Content-Security-Policy': "frame-ancestors 'none'; object-src 'none'; base-uri 'self'",
      },
    },
  },
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
