# Releasing Lyncost: the A to Z checklist

Every release, Linux and Windows together, goes through this list in order. Tick each box in the
release pull request or in your notes. Steps marked **(CI)** run automatically on GitHub; you only
check that they are green.

The website (wintershogun.com) lives in a separate repository; section J covers it.

## A. Decide

- [ ] Write down what changed, in plain words. This becomes the changelog and release notes.
- [ ] Pick the version: bug or display fix = `1.1.x`, new feature = `1.x.0`, breaking change = `x.0.0`.
- [ ] Work on a branch (`git checkout -b release/X.Y.Z`), never straight on `main`.

## B. Rules while changing code

- [ ] `dist/` is the original prebuilt frontend (its source is lost). Change it only with exact-match
      string patches that assert how many times the text was found, or through `dist/lyncost-polish.css`
      and `dist/lyncost-boot.js`. The same `dist/` ships to Linux and Windows: a text that must differ
      uses `/Windows/i.test(navigator.userAgent)`.
- [ ] Windows must never say "Linux", "Fedora", or "free" (except the free trial).
- [ ] Backend change: run `python3 scripts/check-ipc-contract.py`.
- [ ] License change (`src-tauri/src/license.rs`): the key format must stay identical to
      `scripts/license/issue.mjs` **and** the website issuer (`worker/lib/lyncost-license.ts`), or paid keys
      stop working. Never change the public key.

## C. Test on this laptop (Linux)

Keep the laptop cool: these use `nice` and 2 build jobs.

- [ ] `npm test` (Rust tests + contract check)
- [ ] `cargo test --manifest-path src-tauri/Cargo.toml --features qa,license-gate license`
- [ ] Real app, on a hidden screen (never touches your own Lyncost):
  ```sh
  CARGO_BUILD_JOBS=2 nice -n 19 sh scripts/qa/env.sh build
  sh scripts/qa/env.sh reset && sh scripts/qa/env.sh start
  cd scripts/qa
  python3 license_flows.py          # trial, lock, fake unlock, wrong/forged/real key
  sh env.sh reset && python3 seed.py && python3 flows.py   # every feature (26 checks)
  python3 layout_check.py 135790    # clipped, wrapped, spilled text at 4 window sizes
  cd ../.. && sh scripts/qa/env.sh stop
  ```
- [ ] All pass, 0 JS errors. Layout: only long names (like "Interactive Brokers") may wrap.

## D. Set the version

- [ ] `python3 scripts/set-version.py X.Y.Z` (package.json, Cargo.toml, Linux app-store file, labels)
- [ ] Add the release at the top of `CHANGELOG.md` (date, plain-language bullets).
- [ ] Update `version.json` (version, one-line notes, date, `release_url` ending in `vX.Y.Z`).
      Linux apps read this file to show "Update available".

## E. Full check on GitHub before merging (CI)

- [ ] Push the branch, then start the checks: `gh workflow run build.yml --ref release/X.Y.Z`
- [ ] Every job green:
  - `test`, `audit` (no known vulnerabilities)
  - `build-linux` and `install-test` on Fedora 43/44, Ubuntu 22.04/24.04, Debian 12, Mint 22
  - `build-windows` (installer + signed update file), `smoke-windows` (silent install, start, trial
    recorded, uninstall keeps data)
  - `qa-windows`: license test, 26 feature checks, layout check on real Windows
- [ ] Download the `windows-qa-screenshots` artifact and **look at every page in both themes**.
      Things tests miss: overlapping text, stretched cards, wrong wording, old version numbers.

## F. Release (CI)

- [ ] Merge to `main`, then: `git tag -a vX.Y.Z -m "Lyncost X.Y.Z" && git push origin main vX.Y.Z`
- [ ] Wait for the tag's build: it repeats every check and creates a **draft** release.
- [ ] Draft has 6 files: `.rpm`, `.deb`, `.AppImage`, `_x64-setup.exe`, `SHA256SUMS`, `latest.json`.
- [ ] `latest.json` says version X.Y.Z, its URL ends in `vX.Y.Z/Lyncost_X.Y.Z_x64-setup.exe`, and it has a signature.
- [ ] Publish: `gh release edit vX.Y.Z --title "..." --notes "..." --draft=false --latest`

## G. Check the published release

- [ ] `curl -sL https://github.com/UltraDarshan/lyncost/releases/latest/download/latest.json` shows X.Y.Z
      (Windows apps will offer the update).
- [ ] `version.json` on `main` shows X.Y.Z (Linux apps will offer the update).
- [ ] Every download link answers (rpm, deb, AppImage, setup.exe, SHA256SUMS).
- [ ] Full-screen Windows check of the released installer with a real license key:
      `gh workflow run windows-tour.yml -f tag=vX.Y.Z`, then look at the `windows-tour-screenshots`
      artifact (first launch, activation, every page, 26 feature checks).

## H. Website (wintershogun repository)

- [ ] `src/config/lyncost.ts`: `version`, `releaseDate`, the three Linux file names and sizes,
      `windows.installer` and `windows.size` (sizes from the release page).
- [ ] `content/products/lyncost-windows.md`: `file`, `edition`, `fileSize`.
- [ ] `wsadmin check`, then `wsadmin publish`.
- [ ] Live: the Lyncost page and shop show the new version; "Download free trial" opens the install
      guide and downloads the new installer; Linux download buttons work.

## I. Money path (only when checkout, prices, coupons, or keys changed)

- [ ] Buy once with a real payment (a coupon is fine), see the key on My downloads, recover it with
      "Recover a purchase", activate it in the app, then refund in the Razorpay dashboard.

## J. Afterwards

- [ ] Optional: announce it (blog or news post through `wsadmin new`; facts only).
- [ ] Add a history row to the website's `AGENTS.md` when the website changed.
- [ ] `sh scripts/qa/env.sh stop` (frees the laptop).

## If something goes wrong

- **Broken release:** mark the previous release as latest (`gh release edit vPREVIOUS --latest`),
  point `version.json` back to it, then fix and release `X.Y.Z+1`. Never delete a published release:
  installed apps and download links may point to it.
- **Website broken:** `wsadmin rollback`.

## Keys and secrets (never in git)

| What | Where | Used for |
| --- | --- | --- |
| License signing key (private) | `~/.config/lyncost-signing/license-ed25519-private.pem` + encrypted USB | Making license keys. Lost = no new keys that work |
| Same key, as a secret | Cloudflare Worker `wintershogun`, `LYNCOST_LICENSE_KEY` | The website makes keys after payment |
| Update signing key | `~/.config/lyncost-signing/updater.key` (+ `.password`) + USB | Signing Windows updates |
| Same, as secrets | GitHub `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | CI signs `latest.json` |
| A real license key | GitHub `LYNCOST_TEST_LICENSE` | `windows-tour.yml` activation check |

Release builds must never use `--features qa` (it trusts the public test key). CI fails the build if
the test key is found in `lyncost.exe`.
