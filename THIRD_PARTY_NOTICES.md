# Third-party notices

The [MIT license](LICENSE) covers original DBsaur code and documentation.
Dependencies and third-party assets retain their own licenses and notices.
This inventory highlights bundled visual assets; it is not a complete dependency
license inventory. Inspect the resolved npm and Cargo trees and their license
files when redistributing a build, including transitive dependencies.

| Asset | Source | License / notice |
| --- | --- | --- |
| IBM Plex Sans | `@fontsource-variable/ibm-plex-sans` | SIL Open Font License 1.1; Copyright 2019 IBM Corp. |
| IBM Plex Mono | `@fontsource/ibm-plex-mono` | SIL Open Font License 1.1; Copyright 2017 IBM Corp. |
| Google Material Symbols SVG icons | `@material-symbols/svg-400` | Apache License 2.0; unmodified icons imported in `AppIcon.vue` |
| MapLibre map viewer | `maplibre-gl` | BSD 3-Clause; license retained in `frontend/public/licenses/maplibre-gl.txt` |
| Tauri NSIS installer template | Tauri CLI 2.12.4; adapted in `src-tauri/windows/installer.nsi` | MIT; Copyright (c) 2017 - Present Tauri Apps Contributors; full license below |
| VijiTiles Soft Earth basemap style | `maps/vijitiles/web/style.basemap.json` | Bundled snapshot in `frontend/public/maps/vijitiles.json`; vector tiles and fonts served by vijimaps.com; OpenStreetMap data attribution displayed in the map |

Full asset license texts are retained in [frontend/public/licenses](frontend/public/licenses).
Keep them with distributed assets. Additional attribution requirements from
dependencies must also be preserved in distributed bundles and container images.

The Vijitek name and logo identify the original project. The software license
does not grant trademark rights or imply endorsement of forks. Database product
names belong to their respective owners; DBsaur is an independent project and
does not claim affiliation with those vendors.

## Tauri installer template license

MIT License

Copyright (c) 2017 - Present Tauri Apps Contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
