# Maintained dependency patches

These local packages replace vulnerable upstream releases in the frontend build
tree. Their original licenses and copyright notices remain in each package.

| Package | Upstream base | Local version | Advisory | Maintained fix |
| --- | --- | --- | --- | --- |
| `braces` | 3.0.3 | 3.0.4 | [GHSA-vfj7-8cjw-p6xm](https://github.com/advisories/GHSA-vfj7-8cjw-p6xm) | Bounds parser nesting and recursive AST walks at 100 levels; rejects cyclic parent chains in expansion. |

The `braces` hardening follows the reviewed changes proposed in
[micromatch/braces#72](https://github.com/micromatch/braces/pull/72), with depth
checks retained across parse, compile, expand, and stringify. This patch stays
local until upstream publishes a fixed release.

The previous local `node-forge` patch has been replaced by upstream 1.4.3.
Its malformed DigestInfo regression test remains alongside the braces tests in
`frontend/tests/dependency-hardening.test.mjs`. Local file dependencies are copied
at install time (`install-links=true`) so moved checkouts do not retain stale links.
