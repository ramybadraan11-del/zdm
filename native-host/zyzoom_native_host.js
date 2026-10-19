#!/usr/bin/env node
'use strict';

/**
 * مضيّف Native Messaging لـ Zyzoom Download Manager.
 *
 * يقرأ رسائل المتصفح بصيغة Chrome Native Messaging (طول 4 بايت + JSON)،
 * ويحوّلها إلى طلبات HTTPLocal إلى جسر الالتقاط داخل التطبيق،
 * ثم يعيد النتيجة إلى المتصفح بنفس الصيغة.
 *
 * التشغيل: يطلقه المتصفح تلقائيًا. لا يُستدعى يدويًا إلا للاختبار.
 */

const fs = require('fs');
const os = require('os');
const path = require('path');
const http = require('http');

function configPath() {
  if (process.platform === 'win32') {
    return path.join(os.homedir(), 'AppData', 'Roaming', 'ZyzoomDownloadManager', 'bridge.json');
  }
  if (process.platform === 'darwin') {
    return path.join(os.homedir(), 'Library', 'Application Support', 'ZyzoomDownloadManager', 'bridge.json');
  }
  return path.join(os.homedir(), '.config', 'zyzoom', 'bridge.json');
}

function readConfig() {
  try {
    return JSON.parse(fs.readFileSync(configPath(), 'utf8'));
  } catch (err) {
    return null;
  }
}

function postJson(target, body) {
  return new Promise((resolve) => {
    const url = new URL(target);
    const payload = Buffer.from(JSON.stringify(body), 'utf8');
    const request = http.request(
      {
        hostname: url.hostname,
        port: url.port || 80,
        path: url.pathname,
        method: 'POST',
        headers: {
          'content-type': 'application/json',
          'content-length': payload.length,
        },
      },
      (response) => {
        const chunks = [];
        response.on('data', (chunk) => chunks.push(chunk));
        response.on('end', () => {
          const text = Buffer.concat(chunks).toString('utf8');
          try {
            resolve(JSON.parse(text));
          } catch (err) {
            resolve({ ok: false, error: 'invalid-response' });
          }
        });
      },
    );
    request.on('error', (err) => resolve({ ok: false, error: err.message }));
    request.end(payload);
  });
}

function writeMessage(message) {
  const body = Buffer.from(JSON.stringify(message), 'utf8');
  const header = Buffer.alloc(4);
  header.writeUInt32LE(body.length, 0);
  process.stdout.write(Buffer.concat([header, body]));
}

function readMessages(onMessage) {
  let buffer = Buffer.alloc(0);
  process.stdin.on('data', (chunk) => {
    buffer = Buffer.concat([buffer, chunk]);
    for (;;) {
      if (buffer.length < 4) return;
      const length = buffer.readUInt32LE(0);
      if (buffer.length < 4 + length) return;
      const body = buffer.subarray(4, 4 + length).toString('utf8');
      buffer = buffer.subarray(4 + length);
      let message = null;
      try {
        message = JSON.parse(body);
      } catch (err) {
        continue;
      }
      onMessage(message);
    }
  });
  process.stdin.on('end', () => process.exit(0));
}

async function handle(message) {
  const config = readConfig();
  if (!config) {
    writeMessage({ ok: false, error: 'zyzoom-app-not-running' });
    return;
  }

  if (message.type === 'ping') {
    writeMessage({ ok: true, pong: true });
    return;
  }

  const items = Array.isArray(message.items) ? message.items : [];
  const response = await postJson(`http://${config.host}:${config.port}/capture`, {
    token: config.token,
    items,
    source: message.source || 'extension',
  });
  writeMessage(response);
}

readMessages((message) => {
  handle(message).catch((err) => writeMessage({ ok: false, error: err.message }));
});
