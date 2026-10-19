'use strict';

/**
 * اختبارات نواة التنزيل: تقسيم الأجزاء، الاستئناف، الإيقاف المؤقت.
 * التشغيل: node --test test/
 */

const test = require('node:test');
const assert = require('node:assert');
const http = require('node:http');
const fs = require('node:fs');
const fsp = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');
const { SegmentDownload } = require('../src/main/downloader');

function makePayload(size) {
  return Buffer.from(
    Array.from({ length: size }, (_, i) => (i * 7 + (i >> 8) * 13) % 251),
  );
}

function startServer(payload, { slow = false, ranges = true } = {}) {
  const server = http.createServer((req, res) => {
    if (req.method === 'HEAD') {
      res.writeHead(200, {
        'content-length': payload.length,
        'accept-ranges': ranges ? 'bytes' : 'none',
      });
      res.end();
      return;
    }
    const range = req.headers.range;
    if (ranges && range) {
      const match = /bytes=(\d+)-(\d*)/.exec(range);
      const start = Number(match[1]);
      const end = match[2] ? Number(match[2]) : payload.length - 1;
      res.writeHead(206, {
        'content-range': `bytes ${start}-${end}/${payload.length}`,
        'content-length': end - start + 1,
        'accept-ranges': 'bytes',
      });
      const slice = payload.subarray(start, end + 1);
      if (slow) {
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
      res.end(slice);
      return;
    }
    res.writeHead(200, {
      'content-length': payload.length,
      'accept-ranges': ranges ? 'bytes' : 'none',
    });
    if (slow) {
      let offset = 0;
      const chunk = 16 * 1024;
      const timer = setInterval(() => {
        if (offset >= payload.length) {
          clearInterval(timer);
          res.end();
          return;
        }
        res.write(payload.subarray(offset, offset + chunk));
        offset += chunk;
      }, 40);
      res.on('close', () => clearInterval(timer));
      return;
    }
    res.end(payload);
  });
  return new Promise((resolve) => {
    server.listen(0, '127.0.0.1', () => resolve({ server, port: server.address().port }));
  });
}

async function waitForStatus(download, status, timeoutMs = 20000) {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    if (download.status === status) return true;
    await new Promise((r) => setTimeout(r, 50));
  }
  return false;
}

test('ينزّل ملفًا كاملًا باستخدام عدة أجزاء', async () => {
  const payload = makePayload(512 * 1024);
  const { server, port } = await startServer(payload);
  const dir = await fsp.mkdtemp(path.join(os.tmpdir(), 'zyzoom-'));

  const download = new SegmentDownload({
    url: `http://127.0.0.1:${port}/file.bin`,
    dir,
    name: 'file.bin',
    connections: 8,
  });

  download.start();
  assert.ok(await waitForStatus(download, 'completed'), 'اكتمل التنزيل');
  assert.equal(download.segments.length, 8, 'تم تقسيم الملف إلى 8 أجزاء');
  assert.equal(download.totalSize, payload.length, 'الحجم الكلي صحيح');

  const written = await fsp.readFile(path.join(dir, 'file.bin'));
  assert.ok(written.equals(payload), 'محتوى الملف مطابق تمامًا');

  await fsp.rm(dir, { recursive: true, force: true });
  server.close();
});

test('يتعامل مع خادم لا يدعم Range بجزء واحد', async () => {
  const payload = makePayload(64 * 1024);
  const { server, port } = await startServer(payload, { ranges: false });
  const dir = await fsp.mkdtemp(path.join(os.tmpdir(), 'zyzoom-'));

  const download = new SegmentDownload({
    url: `http://127.0.0.1:${port}/file.bin`,
    dir,
    name: 'norange.bin',
    connections: 8,
  });

  download.start();
  assert.ok(await waitForStatus(download, 'completed'));
  assert.equal(download.segments.length, 1, 'جزء واحد فقط');
  const written = await fsp.readFile(path.join(dir, 'norange.bin'));
  assert.ok(written.equals(payload));

  await fsp.rm(dir, { recursive: true, force: true });
  server.close();
});

test('يوقف مؤقتًا ثم يكمل من حيث توقف', async () => {
  const payload = makePayload(4 * 1024 * 1024);
  const { server, port } = await startServer(payload, { slow: true });
  const dir = await fsp.mkdtemp(path.join(os.tmpdir(), 'zyzoom-'));

  const download = new SegmentDownload({
    url: `http://127.0.0.1:${port}/slow.bin`,
    dir,
    name: 'slow.bin',
    connections: 4,
  });

  download.start();
  await new Promise((r) => setTimeout(r, 700));
  await download.pause();
  assert.equal(download.status, 'paused');
  assert.ok(download.downloaded > 0, 'تم تنزيل جزء قبل الإيقاف');
  assert.ok(fs.existsSync(path.join(dir, 'slow.bin.part')), 'ملف .part موجود');
  assert.ok(fs.existsSync(path.join(dir, 'slow.bin.zyzoom.json')), 'ملف الحالة موجود');

  const partial = download.downloaded;
  await new Promise((r) => setTimeout(r, 300));
  assert.ok(
    download.downloaded - partial <= 64 * 1024,
    'لا اكتمال إضافي كبير بعد الإيقاف',
  );

  download.start();
  assert.ok(await waitForStatus(download, 'completed'), 'اكتمل بعد الاستئناف');
  const written = await fsp.readFile(path.join(dir, 'slow.bin'));
  assert.ok(written.equals(payload), 'الملف مكتمل وصحيح بعد الاستئناف');
  assert.ok(!fs.existsSync(path.join(dir, 'slow.bin.zyzoom.json')), 'حُذف ملف الحالة');

  await fsp.rm(dir, { recursive: true, force: true });
  server.close();
});

test('يبلّغ عن الفشل عند خطأ HTTP', async () => {
  const server = http.createServer((req, res) => {
    res.writeHead(404);
    res.end('nope');
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const port = server.address().port;
  const dir = await fsp.mkdtemp(path.join(os.tmpdir(), 'zyzoom-'));

  const download = new SegmentDownload({
    url: `http://127.0.0.1:${port}/missing.bin`,
    dir,
    name: 'missing.bin',
    connections: 2,
  });

  download.start();
  assert.ok(await waitForStatus(download, 'failed'), 'سُجّل الفشل');
  assert.ok(download.error, 'رسالة الخطأ محفوظة');

  await fsp.rm(dir, { recursive: true, force: true });
  server.close();
});
