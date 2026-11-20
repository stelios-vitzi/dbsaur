import type { Geometry } from 'geojson'
import type { AddProtocolAction, StyleSpecification } from 'maplibre-gl'
import vijitilesStyle from '../../assets/maps/vijitiles.json' with { type: 'json' }

export type GeometryBounds = [[number, number], [number, number]]

export const basemapProtocol = 'dbsaur-map'
const basemapServers: ReadonlyMap<string, string> = new Map([
  ['vijitiles', 'https://vijimaps.com/tiles/'],
  ['osm', 'https://tile.openstreetmap.org/'],
])

const openStreetMapStyle: StyleSpecification = {
  version: 8,
  sources: {
    osm: {
      type: 'raster',
      tiles: [`${basemapProtocol}://osm/{z}/{x}/{y}.png`],
      tileSize: 256,
      maxzoom: 19,
      attribution:
        '© <a href="https://www.openstreetmap.org/copyright" target="_blank" rel="noopener noreferrer">OpenStreetMap</a> contributors',
    },
  },
  layers: [{ id: 'osm', type: 'raster', source: 'osm' }],
}

// MapLibre suppresses tile HTTP 404s. Report missing or stalled resources as
// errors so the preview can switch providers.
export const fetchBasemapResource: AddProtocolAction = async ({ url }, { signal }) => {
  const resource = new URL(url)
  const server = basemapServers.get(resource.hostname)
  if (!server) throw new Error('Unknown basemap provider')
  const response = await fetch(server + resource.pathname.slice(1), {
    credentials: 'omit',
    signal: AbortSignal.any([signal, AbortSignal.timeout(8000)]),
  })
  if (!response.ok) throw new Error(`Basemap request failed (HTTP ${response.status})`)
  const data = await response.arrayBuffer()
  if (data.byteLength === 0) throw new Error('Empty basemap response')
  return {
    data,
    cacheControl: response.headers.get('Cache-Control') ?? undefined,
    expires: response.headers.get('Expires') ?? undefined,
  }
}

export function mapBackgrounds(geometry: Geometry): StyleSpecification[] {
  return [
    geometryMapStyle(geometry, vijitilesStyle as unknown as StyleSpecification),
    geometryMapStyle(geometry, openStreetMapStyle),
    geometryMapStyle(geometry),
  ]
}

export function geometryBounds(geometry: Geometry): GeometryBounds | null {
  let west = Infinity
  let south = Infinity
  let east = -Infinity
  let north = -Infinity
  function coordinates(value: unknown) {
    if (!Array.isArray(value)) return
    if (typeof value[0] === 'number' && typeof value[1] === 'number') {
      const [x, y] = value as [number, number]
      if (!Number.isFinite(x) || !Number.isFinite(y)) return
      west = Math.min(west, x)
      east = Math.max(east, x)
      south = Math.min(south, Math.max(-85.051129, Math.min(85.051129, y)))
      north = Math.max(north, Math.max(-85.051129, Math.min(85.051129, y)))
    } else value.forEach(coordinates)
  }
  function visit(value: Geometry) {
    if (value.type === 'GeometryCollection') value.geometries.forEach(visit)
    else coordinates(value.coordinates)
  }
  visit(geometry)
  return west === Infinity
    ? null
    : [
        [west, south],
        [east, north],
      ]
}

export function geometryMapStyle(
  geometry: Geometry,
  basemap?: StyleSpecification,
): StyleSpecification {
  return {
    version: 8,
    ...basemap,
    sources: {
      ...(!basemap
        ? {
            'offline-grid': {
              type: 'geojson' as const,
              data: {
                type: 'FeatureCollection' as const,
                features: [
                  ...Array.from({ length: 35 }, (_, i) => [
                    [(i - 17) * 10, -85],
                    [(i - 17) * 10, 85],
                  ]),
                  ...Array.from({ length: 17 }, (_, i) => [
                    [-180, (i - 8) * 10],
                    [180, (i - 8) * 10],
                  ]),
                ].map((coordinates) => ({
                  type: 'Feature' as const,
                  properties: {},
                  geometry: { type: 'LineString' as const, coordinates },
                })),
              },
            },
          }
        : {}),
      ...basemap?.sources,
      'webdb-geometry': { type: 'geojson', data: { type: 'Feature', properties: {}, geometry } },
    },
    layers: [
      ...(basemap?.layers ?? [
        { id: 'background', type: 'background' as const, paint: { 'background-color': '#edf1ef' } },
        {
          id: 'offline-grid',
          type: 'line' as const,
          source: 'offline-grid',
          paint: { 'line-color': '#b7c5c0', 'line-width': 1 },
        },
      ]),
      {
        id: 'webdb-geometry-fill',
        type: 'fill',
        source: 'webdb-geometry',
        filter: ['==', '$type', 'Polygon'],
        paint: { 'fill-color': '#187a65', 'fill-opacity': 0.2 },
      },
      {
        id: 'webdb-geometry-line',
        type: 'line',
        source: 'webdb-geometry',
        filter: ['!=', '$type', 'Point'],
        paint: { 'line-color': '#187a65', 'line-width': 3 },
      },
      {
        id: 'webdb-geometry-point',
        type: 'circle',
        source: 'webdb-geometry',
        filter: ['==', '$type', 'Point'],
        paint: {
          'circle-radius': 7,
          'circle-color': '#187a65',
          'circle-opacity': 0.7,
          'circle-stroke-color': '#187a65',
          'circle-stroke-width': 2,
        },
      },
    ],
  }
}
