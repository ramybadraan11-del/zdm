#!/usr/bin/env node
/**
 * Build the Tauri updater's latest.json for a release.
 *
 * Usage: node tools/make-latest-json.mjs <version> <urlBase> <appimage> <windowsExe>
 *
 * <appimage> and <windowsExe> must already be signed with
 * `tauri signer sign <file>`, which writes <file>.sig next to them. <urlBase> is
 * the releases download base, e.g.
 *   https://github.com/owner/zdm-releases/releases/download/v1.3.0
 * Notes are read from NOTES env var or default to the tag name.
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { basename } from 'node:path';

const [version, urlBase, appimage, windowsExe] = process.argv.slice(2);
if (!version || !urlBase || !appimage || !windowsExe) {
  console.error('usage: make-latest-json.mjs <version> <urlBase> <appimage> <windowsExe>');
  process.exit(2);
}

const readSig = (file) => {
  try {
    return readFileSync(file + '.sig', 'utf8').trim();
  } catch {
    console.error(`missing signature: ${file}.sig — run \`tauri signer sign ${file}\` first`);
    process.exit(1);
  }
};

const appimageName = basename(appimage);
const windowsName = basename(windowsExe);

const manifest = {
  version,
  notes: process.env.RELEASE_NOTES ?? version,
  pub_date: new Date().toISOString(),
  platforms: {
    'linux-x86_64': {
      signature: readSig(appimage),
      url: `${urlBase}/${appimageName}`,
    },
    'windows-x86_64': {
      signature: readSig(windowsExe),
      url: `${urlBase}/${windowsName}`,
    },
  },
};

writeFileSync('latest.json', JSON.stringify(manifest, null, 2));
console.log('latest.json written:', Object.keys(manifest.platforms).join(', '));
