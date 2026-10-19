'use strict';

/**
 * تثبيت سجلّ مضيّف Native Messaging لمتصفحات Chromium و Firefox.
 * سجلّ المضيف يربط اسم المضيف com.zyzoom.download_manager بالملف التنفيذي.
 */

const fs = require('fs');
const fsp = require('fs/promises');
const os = require('os');
const path = require('path');

const HOST_NAME = 'com.zyzoom.download_manager';

function hostManifestPaths() {
  const home = os.homedir();
  const file = `${HOST_NAME}.json`;
  return {
    chrome: path.join(home, '.config', 'google-chrome', 'NativeMessagingHosts', file),
    chromium: path.join(home, '.config', 'chromium', 'NativeMessagingHosts', file),
    edge: path.join(home, '.config', 'microsoft-edge', 'NativeMessagingHosts', file),
    brave: path.join(home, '.config', 'BraveSoftware', 'Brave-Browser', 'NativeMessagingHosts', file),
    firefox: path.join(home, '.mozilla', 'native-messaging-hosts', file),
  };
}

async function writeHostManifests(options = {}) {
  const hostPath = options.hostPath;
  const extensionId = options.extensionId || '';
  if (!hostPath) return { written: [], error: 'missing-host-path' };

  const targets = hostManifestPaths();
  const written = [];

  const chromiumManifest = {
    name: HOST_NAME,
    description: 'Zyzoom Download Manager — جسر التقاط الوسائط',
    path: hostPath,
    type: 'stdio',
  };

  for (const key of ['chrome', 'chromium', 'edge', 'brave']) {
    const target = targets[key];
    try {
      await fsp.mkdir(path.dirname(target), { recursive: true });
      await fsp.writeFile(target, `${JSON.stringify(chromiumManifest, null, 2)}\n`);
      await fsp.chmod(target, 0o644).catch(() => {});
      written.push(target);
    } catch (err) {
      /* نتجاهل الفشل في هذا المتصفح ونكمل الباقي */
    }
  }

  // Firefox يتطلب قائمة allowed_extensions تحتوي معرّف الامتداد
  try {
    await fsp.mkdir(path.dirname(targets.firefox), { recursive: true });
    const firefoxManifest = {
      ...chromiumManifest,
      allowed_extensions: extensionId ? [extensionId] : [],
    };
    await fsp.writeFile(targets.firefox, `${JSON.stringify(firefoxManifest, null, 2)}\n`);
    await fsp.chmod(targets.firefox, 0o644).catch(() => {});
    written.push(targets.firefox);
  } catch (err) {
    /* تجاهل */
  }

  return { written, targets };
}

module.exports = { HOST_NAME, hostManifestPaths, writeHostManifests };
