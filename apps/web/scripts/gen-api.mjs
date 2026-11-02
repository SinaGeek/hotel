// Fetch the server's OpenAPI document and write it to disk for tooling
// (todo F2). Usage:
//   pnpm gen:api                       # http://localhost:8080
//   HMS_API_URL=https://hms.example pnpm gen:api
//
// The TypeScript types in src/types/generated/api.ts are hand-shaped to match
// these schemas; `crates/hms-tests/tests/api_contract.rs` fails the build when
// the two drift, so this script is for inspection and for downstream clients
// (Postman, code generators) rather than for regenerating api.ts blindly.
import { writeFileSync, mkdirSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const base = (process.env.HMS_API_URL || 'http://localhost:8080').replace(/\/+$/, '')
const url = `${base}/api/v1/openapi.json`

const res = await fetch(url)
if (!res.ok) {
  console.error(`GET ${url} -> HTTP ${res.status}`)
  process.exit(1)
}
const doc = await res.json()

const outDir = resolve(here, '../src/types/generated')
mkdirSync(outDir, { recursive: true })
const outFile = resolve(outDir, 'openapi.json')
writeFileSync(outFile, `${JSON.stringify(doc, null, 2)}\n`, 'utf8')

const schemas = Object.keys(doc.components?.schemas ?? {}).length
const paths = Object.keys(doc.paths ?? {}).length
console.log(`wrote ${outFile} (${schemas} schemas, ${paths} paths)`)
