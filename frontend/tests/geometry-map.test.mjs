import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { validateStyleMin } from '@maplibre/maplibre-gl-style-spec'
import { geometryBounds, geometryMapStyle } from '../app/utils/geometryMap.ts'

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

test('VijiTiles style validates and geometry stays above the basemap when toggled', () => {
  const geometry = { type: 'Point', coordinates: [23, 38] }
  const basemap = JSON.parse(readFileSync(new URL('../public/maps/vijitiles.json', import.meta.url), 'utf8'))
  const plain = geometryMapStyle(geometry)
  const full = geometryMapStyle(geometry, basemap)
  assert.deepEqual(validateStyleMin(full).map(error => error.message), [])
  assert.deepEqual(validateStyleMin(plain).map(error => error.message), [])
  assert.deepEqual(Object.keys(plain.sources), ['offline-grid', 'webdb-geometry'])
  assert.equal(plain.glyphs, undefined)
  assert.equal(full.sources['webdb-geometry'].data.geometry, geometry)
  assert.deepEqual(full.layers.slice(-3).map(layer => layer.id), ['webdb-geometry-fill', 'webdb-geometry-line', 'webdb-geometry-point'])
  for (const source of Object.values(basemap.sources)) {
    assert.ok(source.tiles.every(url => url.startsWith('https://vijimaps.com/tiles/')))
  }
  assert.ok(basemap.glyphs.startsWith('https://vijimaps.com/tiles/font/'))
  assert.ok(basemap.sources['vijitiles-basemap'].attribution.includes('openstreetmap.org/copyright'))
  assert.equal(basemap.sources['webdb-geometry'], undefined)
})
