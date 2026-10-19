'use strict';

/**
 * جسر الالتقاط: خادم HTTP محلي على 127.0.0.1 يستقبل الوسائط التي يكتشفها
 * امتداد المتصفح عبر مضيّف Native Messaging.
 *
 * يُحمى برمز سرّي (token) يُخزَّن في ملف إعدادات داخل مجلد بيانات التطبيق،
 * ولا يستقبل أي طلب قادم من خارج الجهاز.
 */

const http = require('http');
const fs = require('fs');
const fsp = require('fs/promises');
const os = require('os');
const path = require('path');
const crypto = require('crypto');

const DEFAULT_PORT = 46800;

function configDir() {
  if (process.platform === 'win32') {
    return path.join(os.homedir(), 'AppData', 'Roaming', 'ZyzoomDownloadManager');
  }
  if (process.platform === 'darwin') {
    return path.join(os.homedir(), 'Library', 'Application Support', 'ZyzoomDownloadManager');
  }
  return path.join(os.homedir(), '.config', 'zyzoom');
}

class CaptureBridge {
  constructor(options = {}) {
    this.port = options.port || DEFAULT_PORT;
    this.token = options.token || crypto.randomBytes(24).toString('hex');
    this.onCapture = options.onCapture || (() => {});
    this.onStatus = options.onStatus || (() => {});
    this.server = null;
    this.configPath = options.configPath || path.join(configDir(), 'bridge.json');
  }

  async writeConfig(extra = {}) {
    await fsp.mkdir(path.dirname(this.configPath), { recursive: true });
    const payload = {
      host: '127.0.0.1',
      port: this.port,
      token: this.token,
      ...extra,
    };
    await fsp.writeFile(this.configPath, JSON.stringify(payload, null, 2), { mode: 0o600 });
    return this.configPath;
  }

  async readConfig() {
    try {
      return JSON.parse(await fsp.readFile(this.configPath, 'utf8'));
    } catch (err) {
      return null;
    }
  }

  async start() {
    this.server = http.createServer((req, res) => {
      const url = new URL(req.url, `http://${req.headers.host}`);
      res.setHeader('Access-Control-Allow-Origin', 'null');
      res.setHeader('Cache-Control', 'no-store');

      if (req.method === 'OPTIONS') {
        res.writeHead(204);
        res.end();
        return;
      }

      if (url.pathname === '/ping') {
        if (url.searchParams.get('token') !== this.token) {
          res.writeHead(403, { 'content-type': 'application/json' });
          res.end(JSON.stringify({ ok: false, error: 'unauthorized' }));
          return;
        }
        res.writeHead(200, { 'content-type': 'application/json' });
        res.end(
          JSON.stringify({
            ok: true,
            app: 'Zyzoom Download Manager',
            version: '1.0.0',
            port: this.port,
          }),
        );
        return;
      }

      if (url.pathname === '/capture' && req.method === 'POST') {
        let body = '';
        req.on('data', (chunk) => {
          body += chunk;
          if (body.length > 1024 * 512) req.destroy();
        });
        req.on('end', () => {
          let payload = null;
          try {
            payload = JSON.parse(body || '{}');
          } catch (err) {
            res.writeHead(400, { 'content-type': 'application/json' });
            res.end(JSON.stringify({ ok: false, error: 'invalid-json' }));
            return;
          }
          if (payload.token !== this.token) {
            res.writeHead(403, { 'content-type': 'application/json' });
            res.end(JSON.stringify({ ok: false, error: 'unauthorized' }));
            return;
          }
          const items = Array.isArray(payload.items) ? payload.items : [];
          const accepted = this.onCapture(items, payload) || 0;
          this.onStatus({ total: items.length, accepted });
          res.writeHead(200, { 'content-type': 'application/json' });
          res.end(JSON.stringify({ ok: true, accepted }));
        });
        return;
      }

      res.writeHead(404, { 'content-type': 'application/json' });
      res.end(JSON.stringify({ ok: false, error: 'not-found' }));
    });

    await new Promise((resolve, reject) => {
      this.server.once('error', reject);
      this.server.listen(this.port, '127.0.0.1', resolve);
    });

    this.server.unref();
    await this.writeConfig();
    return this.configPath;
  }

  stop() {
    if (this.server) {
      this.server.close();
      this.server = null;
    }
  }
}

module.exports = { CaptureBridge, configDir, DEFAULT_PORT };
