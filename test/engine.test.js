'use strict';

/**
 * اختبارات تكامل محرك التنزيلات مع المُنزِّل الحقيقي.
 * التشغيل: node --test test/
 */

const test = require('node:test');
const assert = require('node:assert');
const http = require('node:http');
const fsp = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');
const { DownloadEngine } = require('../src/main/engine');

function makePayload(size) {
  return Buffer.from(Array.from({ length: size }, (_, i) => i % 251));
}

async function startServer(payload, { slow = false } = {}) {
  const server = http.createServer((req, res) => {
    if (req.method === 'HEAD') {
      res.writeHead(200, {
        'content-length': payload.length,
        'accept-ranges': 'bytes',
      });
      res.end();
      return;
    }
    const match = /bytes=(\d+)-(\d*)/.exec(req.headers.range || '');
    if (match) {
      const start = Number(match[1]);
      const end = match[2] ? Number(match[2]) : payload.length - 1;
      res.writeHead(206, {
        'content-range': `bytes ${start}-${end}/${payload.length}`,
        'content-length': end - start + 1,
        'accept-ranges': 'bytes',
      });
      if (slow) {
        const slice = payload.subarray(start, end + 1);
        let offset = 0;
        const chunk = 16 * 1024;
        const timer = setInterval(() => {
          if (offset >= slice.length) {
            clearInterval(timer);
            res.end();
            return;
          }
          res.write(slice.subarray(offset, offset + chunk));
          offset += chunk;
        }, 40);
        res.on('close', () => clearInterval(timer));
        return;
      }
      res.end(payload.subarray(start, end + 1));
      return;
    }
    res.writeHead(200, {
      'content-length': payload.length,
      'accept-ranges': 'bytes',
    });
    res.end(payload);
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  return { server, port: server.address().port };
}

async function waitFor(engine, predicate, timeoutMs = 15000) {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    const list = engine.list();
    if (predicate(list)) return list;
    await new Promise((r) => setTimeout(r, 60));
  }
  return engine.list();
}

test('المحرك يضيف رابطًا وينزّله فعليًا إلى مجلد الحفظ', async () => {
  const payload = makePayload(700 * 1024);
  const { server, port } = await startServer(payload);
  const dir = await fsp.mkdtemp(path.join(os.tmpdir(), 'zyzoom-engine-'));

  const engine = new DownloadEngine({ dir, maxConnections: 8 });
  const record = engine.add({
    url: `http://127.0.0.1:${port}/asset.bin`,
    name: 'asset.bin',
    description: 'اختبار تكامل',
  });

  assert.ok(record.id, 'أُنشئ السجل');
  assert.equal(record.status, 'downloading', 'بدأ التنزيل فورًا');

  const list = await waitFor(engine, (items) => {
    const item = items.find((i) => i.id === record.id);
    return item && item.status === 'completed';
  });

  const item = list.find((i) => i.id === record.id);
  assert.equal(item.status, 'completed', 'اكتمل التنزيل');
  assert.equal(item.size, payload.length, 'الحجم صحيح');
  assert.equal(item.percent, 100, 'النسبة 100%');
  assert.ok(item.real, 'معلمة real مضبوطة');

  const written = await fsp.readFile(path.join(dir, 'asset.bin'));
  assert.ok(written.equals(payload), 'الملف مطابق تمامًا');
  assert.ok(item.finishedAtText && item.finishedAtText !== '--', 'وقت الانتهاء مسجّل');

  engine.stop();
  await fsp.rm(dir, { recursive: true, force: true });
  server.close();
});

test('المحرك يوقف المهمة مؤقتًا ويستأنفها', async () => {
  const payload = makePayload(4 * 1024 * 1024);
  const { server, port } = await startServer(payload, { slow: true });
  const dir = await fsp.mkdtemp(path.join(os.tmpdir(), 'zyzoom-engine-'));

  const engine = new DownloadEngine({ dir, maxConnections: 4 });
  const record = engine.add({ url: `http://127.0.0.1:${port}/big.bin`, name: 'big.bin' });

  await new Promise((r) => setTimeout(r, 150));
  engine.setStatus(record.id, 'pause');
  const paused = engine.list().find((i) => i.id === record.id);
  assert.equal(paused.status, 'paused', 'الحالة متوقفة مؤقتًا');

  engine.setStatus(record.id, 'start');
  const list = await waitFor(engine, (items) => {
    const item = items.find((i) => i.id === record.id);
    return item && item.status === 'completed';
  });
  const item = list.find((i) => i.id === record.id);
  assert.equal(item.status, 'completed', `اكتمل بعد الاستئناف (${item.error || 'بلا خطأ'})`);

  const written = await fsp.readFile(path.join(dir, 'big.bin'));
  assert.ok(written.equals(payload), 'الملف مكتمل بعد الاستئناف');

  engine.stop();
  await fsp.rm(dir, { recursive: true, force: true });
  server.close();
});

test('المحرك يجمع اللقطات من المتصفح ويزيلها', async () => {
  const engine = new DownloadEngine();
  assert.equal(engine.listCaptures().length, 0);

  const added = engine.addCapture({
    url: 'https://example.com/movie.mp4',
    kind: 'network',
    title: 'مقطع',
    pageUrl: 'https://example.com/watch',
  });
  assert.equal(added, 1);
  assert.equal(engine.addCapture({ url: 'https://example.com/movie.mp4' }), 0, 'لا تكرار');

  const captures = engine.listCaptures();
  assert.equal(captures.length, 1);
  assert.equal(captures[0].kind, 'network');

  engine.clearCaptures();
  assert.equal(engine.listCaptures().length, 0);
});
