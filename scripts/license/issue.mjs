#!/usr/bin/env node
// Issue a Lyncost for Windows license key (LYNC3-...).
//
//   node scripts/license/issue.mjs --name "Asha Rao"                 # real key (private key in ~/.config/lyncost-signing)
//   node scripts/license/issue.mjs --name "QA" --key scripts/license/test-private.pem   # test key (QA builds only)
//   node scripts/license/issue.mjs --name "QA" --id 0a0b0c0d0e0f --issued 2026-10-25
//
// Format (must match src-tauri/src/license.rs):
//   payload = [version=1][id: 6 bytes][issued: u16 days since 2026-01-01, big endian][edition=1][name length][name UTF-8]
//   signature = Ed25519(private key, "LYNC3" || payload)
//   key = "LYNC3-" + base32(payload || signature), RFC 4648 alphabet, no padding, in groups of 6.
import { createPrivateKey, randomBytes, sign } from "node:crypto"
import { readFileSync } from "node:fs"
import { homedir } from "node:os"
import { join } from "node:path"

const args = Object.fromEntries(
  process.argv.slice(2).reduce((acc, a, i, all) => (a.startsWith("--") ? [...acc, [a.slice(2), all[i + 1]]] : acc), []),
)
const name = (args.name ?? "").trim().replace(/\s+/g, " ")
const nameBytes = Buffer.from(name, "utf8")
if (!name || nameBytes.length > 40) {
  console.error("Give the buyer's name with --name (1 to 40 bytes).")
  process.exit(1)
}
const keyPath = args.key ?? join(homedir(), ".config/lyncost-signing/license-ed25519-private.pem")
const id = args.id ? Buffer.from(args.id, "hex") : randomBytes(6)
if (id.length !== 6) {
  console.error("--id must be 12 hex characters.")
  process.exit(1)
}
const epoch = Date.UTC(2026, 0, 1)
const issuedAt = args.issued ? Date.parse(`${args.issued}T00:00:00Z`) : Date.now()
const days = Math.floor((issuedAt - epoch) / 86_400_000)

const payload = Buffer.concat([Buffer.from([1]), id, Buffer.from([days >> 8, days & 0xff, 1, nameBytes.length]), nameBytes])
const signature = sign(null, Buffer.concat([Buffer.from("LYNC3"), payload]), createPrivateKey(readFileSync(keyPath)))

const ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"
function base32(buf) {
  let bits = 0
  let value = 0
  let out = ""
  for (const byte of buf) {
    value = (value << 8) | byte
    bits += 8
    while (bits >= 5) {
      out += ALPHABET[(value >>> (bits - 5)) & 31]
      bits -= 5
    }
  }
  if (bits > 0) out += ALPHABET[(value << (5 - bits)) & 31]
  return out
}

const body = base32(Buffer.concat([payload, signature]))
console.log(`LYNC3-${body.match(/.{1,6}/g).join("-")}`)
