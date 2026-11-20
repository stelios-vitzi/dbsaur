import assert from 'node:assert/strict'
import { test } from 'node:test'
import { validateStyleMin } from '@maplibre/maplibre-gl-style-spec'
import { fetchBasemapResource, geometryBounds, mapBackgrounds } from '../app/utils/geometryMap.ts'

test('fits every geometry in collections, ignores elevation, and handles empty coordinates', () => {
  assert.deepEqual(geometryBounds({ type: 'GeometryCollection', geometries: [
    { type: 'Point', coordinates: [23, 38, 500] },
    { type: 'MultiPolygon', coordinates: [[[[20, 35], [25, 35], [25, 40], [20, 35]]]] },
  ] }), [[20, 35], [25, 40]])
  assert.equal(geometryBounds({ type: 'Point', coordinates: [] }), null)
  assert.equal(geometryBounds({ type: 'GeometryCollection', geometries: [] }), null)
  assert.equal(geometryBounds({ type: 'Point', coordinates: [NaN, Infinity] }), null)
  assert.deepEqual(geometryBounds({ type: 'Point', coordinates: [0, 90] }), [[0, 85.051129], [0, 85.051129]])
})

test('each basemap validates and keeps the geometry above the background', () => {
  const geometry = { type: 'Point', coordinates: [23, 38] }
  const [vijitiles, osm, offline] = mapBackgrounds(geometry)
  assert.equal(vijitiles.sources['vijitiles-basemap'].type, 'vector')
  assert.equal(osm.sources.osm.type, 'raster')
  assert.match(osm.sources.osm.attribution, /openstreetmap\.org\/copyright/)
  assert.deepEqual(Object.keys(offline.sources), ['offline-grid', 'webdb-geometry'])
  for (const style of [vijitiles, osm, offline]) {
    assert.deepEqual(validateStyleMin(style).map(error => error.message), [])
    assert.equal(style.sources['webdb-geometry'].data.geometry, geometry)
    assert.deepEqual(style.layers.slice(-3).map(layer => layer.id), ['webdb-geometry-fill', 'webdb-geometry-line', 'webdb-geometry-point'])
  }
})

test('basemap loads use fixed provider URLs and report missing tiles for fallback', async (t) => {
  const controller = new AbortController()
  const fetch = t.mock.method(globalThis, 'fetch', async (url, options) => {
    assert.equal(options.credentials, 'omit')
    assert.equal(options.signal.aborted, false)
    if (url === 'https://vijimaps.com/tiles/basemap/3/4/3')
      return new Response(null, { status: 404 })
    assert.equal(url, 'https://tile.openstreetmap.org/3/4/3.png')
    return new Response(new Uint8Array([42]), { headers: { 'Cache-Control': 'max-age=3600' } })
  })
  await assert.rejects(
    fetchBasemapResource({ url: 'dbsaur-map://vijitiles/basemap/3/4/3' }, controller),
    /HTTP 404/,
  )
  const tile = await fetchBasemapResource({ url: 'dbsaur-map://osm/3/4/3.png' }, controller)
  assert.deepEqual(new Uint8Array(tile.data), new Uint8Array([42]))
  assert.equal(tile.cacheControl, 'max-age=3600')
  await assert.rejects(
    fetchBasemapResource({ url: 'dbsaur-map://example.com/3/4/3.png' }, controller),
    /Unknown basemap provider/,
  )
  assert.equal(fetch.mock.callCount(), 2)
})
