'use strict';

/**
 * اختبارات جسر الالتقاط (Native Messaging → التطبيق).
 * التشغيل: node --test test/
 */

const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const fsp = require('node:fs/promises');
const http = require('node:http');
const os = require('node:os');
const path = require('node:path');
const { spawn } = require('node:child_process');
const { CaptureBridge } = require('../src/main/bridge');

function post(port, pathname, body) {
  return new Promise((resolve, reject) => {
    const payload = Buffer.from(JSON.stringify(body), 'utf8');
    const request = http.request(
      {
        hostname: '127.0.0.1',
        port,
        path: pathname,
        method: 'POST',
        headers: { 'content-type': 'application/json', 'content-length': payload.length },
      },
      (response) => {
        const chunks = [];
        response.on('data', (chunk) => chunks.push(chunk));
        response.on('end', () => {
          const text = Buffer.concat(chunks).toString('utf8');
          try {
            resolve({ status: response.statusCode, body: JSON.parse(text) });
          } catch (err) {
            resolve({ status: response.statusCode, body: text });
          }
        });
      },
    );
    request.on('error', reject);
    request.end(payload);
  });
}

function get(port, pathname) {
  return new Promise((resolve, reject) => {
    const request = http.request(
      { hostname: '127.0.0.1', port, path: pathname, method: 'GET' },
      (response) => {
        const chunks = [];
        response.on('data', (chunk) => chunks.push(chunk));
        response.on('end', () => {
          const text = Buffer.concat(chunks).toString('utf8');
          try {
            resolve({ status: response.statusCode, body: JSON.parse(text) });
          } catch (err) {
            resolve({ status: response.statusCode, body: text });
          }
        });
      },
    );
    request.on('error', reject);
    request.end();
  });
}

test('يستقبل العناصر من مضيّف محلي ويتجاهل الطلبات غير المصرّح بها', async () => {
  const dir = await fsp.mkdtemp(path.join(os.tmpdir(), 'zyzoom-bridge-'));
  const received = [];
  const bridge = new CaptureBridge({
    port: 0,
    configPath: path.join(dir, 'bridge.json'),
    onCapture: (items) => {
      received.push(...items);
      return items.length;
    },
  });

  // منفذ عشوائي
  await new Promise((resolve, reject) => {
    bridge.server = http.createServer();
    bridge.server.once('error', reject);
    bridge.server.listen(0, '127.0.0.1', resolve);
  });
  bridge.port = bridge.server.address().port;
  bridge.server.close();
  await bridge.start();

  const ok = await post(bridge.port, '/capture', {
    token: bridge.token,
    items: [{ url: 'https://example.com/a.mp4', kind: 'network' }],
  });
  assert.equal(ok.status, 200);
  assert.equal(ok.body.accepted, 1);
  assert.equal(received.length, 1);
  assert.equal(received[0].url, 'https://example.com/a.mp4');

  const denied = await post(bridge.port, '/capture', {
    token: 'wrong-token',
    items: [{ url: 'https://example.com/b.mp4' }],
  });
  assert.equal(denied.status, 403);
  assert.equal(received.length, 1, 'لم تُقبل العناصر غير المصرّح بها');

  const ping = await get(bridge.port, `/ping?token=${bridge.token}`);
  assert.equal(ping.status, 200);
  assert.equal(ping.body.app, 'Zyzoom Download Manager');

  const config = JSON.parse(await fsp.readFile(bridge.configPath, 'utf8'));
  assert.equal(config.token, bridge.token);
  assert.equal(config.port, bridge.port);

  bridge.stop();
  await fsp.rm(dir, { recursive: true, force: true });
});

test('مضيف Native Messaging يمرّر الرسائل إلى الجسر', async () => {
  const dir = await fsp.mkdtemp(path.join(os.tmpdir(), 'zyzoom-host-'));
  const homeConfig = path.join(
    os.homedir(),
    '.config',
    'zyzoom',
    'bridge.json',
  );
  const previous = fs.existsSync(homeConfig) ? await fsp.readFile(homeConfig, 'utf8') : null;

  const bridge = new CaptureBridge({
    port: 0,
    configPath: homeConfig,
    onCapture: (items) => items.length,
  });
  await new Promise((resolve, reject) => {
    bridge.server = http.createServer();
    bridge.server.once('error', reject);
    bridge.server.listen(0, '127.0.0.1', resolve);
  });
  bridge.port = bridge.server.address().port;
  bridge.server.close();
  await bridge.start();

  const hostScript = path.join(__dirname, '..', 'native-host', 'zyzoom_native_host.js');
  const child = spawn(process.execPath, [hostScript], { stdio: ['pipe', 'pipe', 'pipe'] });

  const message = Buffer.from(
    JSON.stringify({
      type: 'capture',
      source: 'zyzoom-capture',
      items: [{ url: 'https://example.com/movie.mkv', kind: 'network' }],
    }),
    'utf8',
  );
  const header = Buffer.alloc(4);
  header.writeUInt32LE(message.length, 0);
  child.stdin.write(Buffer.concat([header, message]));

  const reply = await new Promise((resolve, reject) => {
    let buffer = Buffer.alloc(0);
    const timer = setTimeout(() => reject(new Error('no reply from native host')), 8000);
    child.stdout.on('data', (chunk) => {
      buffer = Buffer.concat([buffer, chunk]);
      if (buffer.length < 4) return;
      const length = buffer.readUInt32LE(0);
      if (buffer.length < 4 + length) return;
      clearTimeout(timer);
      resolve(JSON.parse(buffer.subarray(4, 4 + length).toString('utf8')));
    });
    child.on('error', reject);
  });

  assert.equal(reply.ok, true);
  assert.equal(reply.accepted, 1);

  child.kill();
  bridge.stop();
  if (previous === null) await fsp.unlink(homeConfig).catch(() => {});
  else await fsp.writeFile(homeConfig, previous);
  await fsp.rm(dir, { recursive: true, force: true });
});
