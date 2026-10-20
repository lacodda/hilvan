// Puts the checker - hilvan's core built to WebAssembly - where the app
// imports it from: src/check.wasm, which is built, never committed.
//
//   node scripts/check-wasm.mjs
//
// Built from crates/check with cargo, the way the server is: the app checks a
// typed turn with the code the server grades it with. Where cargo is not at
// hand - the image's Node stage - the module is built in a stage of its own
// and named by HILVAN_CHECK_WASM, and this copies it.
import { execFileSync } from 'node:child_process'
import { copyFileSync, existsSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

const root = fileURLToPath(new URL('../..', import.meta.url))
const target = fileURLToPath(new URL('../src/check.wasm', import.meta.url))
const built = fileURLToPath(new URL('../../target/wasm32-unknown-unknown/wasm/hilvan_check.wasm', import.meta.url))

const given = process.env.HILVAN_CHECK_WASM
if (given) {
  if (!existsSync(given)) {
    console.error(`HILVAN_CHECK_WASM names ${given}, which is not there`)
    process.exit(1)
  }
  copyFileSync(given, target)
  console.log(`check.wasm copied from ${given}`)
} else {
  execFileSync('cargo', ['build', '-p', 'hilvan-check', '--target', 'wasm32-unknown-unknown', '--profile', 'wasm'], {
    cwd: root,
    stdio: 'inherit',
  })
  copyFileSync(built, target)
  console.log('check.wasm built from crates/check')
}
