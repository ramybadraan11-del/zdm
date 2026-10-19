'use strict';

const { app, BrowserWindow, ipcMain, shell, nativeTheme } = require('electron');
const path = require('path');
const { DownloadEngine } = require('./engine');
const { CaptureBridge } = require('./bridge');
const { writeHostManifests, hostManifestPaths } = require('./host-manifest');

const isDev = process.argv.includes('--dev');
const isSelfTest = process.argv.includes('--selftest');
const engine = new DownloadEngine();

let mainWindow = null;
let snifferWindow = null;
let bridge = null;

function rendererUrl(file) {
  return `file://${path.join(__dirname, '..', 'renderer', file)}`;
}

function createMainWindow() {
  mainWindow = new BrowserWindow({
    width: 1280,
    height: 820,
    minWidth: 1000,
    minHeight: 640,
    show: false,
    frame: false,
    backgroundColor: '#070d19',
    title: 'Zyzoom Download Manager',
    icon: path.join(__dirname, '..', 'renderer', 'assets', 'logo.png'),
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false,
    },
  });

  mainWindow.loadURL(rendererUrl('index.html'));
  mainWindow.once('ready-to-show', () => mainWindow.show());

  if (isSelfTest) {
    mainWindow.webContents.on('console-message', (_e, level, message, line, sourceId) => {
      const prefix = level >= 2 ? 'RENDERER-ERROR' : 'RENDERER';
      console.log(`[${prefix}] ${message} (${sourceId}:${line})`);
    });
    mainWindow.webContents.on('did-fail-load', (_e, code, desc) => {
      console.log(`[LOAD-ERROR] ${code} ${desc}`);
    });
    setTimeout(async () => {
      const shotPath = process.env.ZYZOOM_SELFTEST_SHOT;
      const probe = process.env.ZYZOOM_SELFTEST_JS;
      if (probe) {
        try {
          const result = await mainWindow.webContents.executeJavaScript(probe, true);
          console.log('[SELFTEST-JS]', JSON.stringify(result));
        } catch (err) {
          console.log(`[SELFTEST-JS] failed: ${err.message}`);
        }
        await new Promise((r) => setTimeout(r, 400));
      }
      if (process.env.ZYZOOM_SELFTEST_CAPTURE) {
        engine.addCapture({
          url: 'https://cdn.example.com/media/movie-1080p.mp4',
          kind: 'network',
          mime: 'video/mp4',
          title: 'مقطع تجريبي بدقة 1080p',
          pageUrl: 'https://example.com/watch/1',
        });
        engine.addCapture({
          url: 'https://cdn.example.com/media/track.m4a',
          kind: 'network',
          mime: 'audio/mp4',
          title: 'مسار صوتي',
          pageUrl: 'https://example.com/watch/1',
        });
        await mainWindow.webContents.executeJavaScript(
          'document.querySelector(\'[data-tab="browser-list"]\').click()',
          true,
        );
        await new Promise((r) => setTimeout(r, 600));
      }
      if (shotPath) {
        try {
          const image = await mainWindow.webContents.capturePage();
          require('fs').writeFileSync(shotPath, image.toPNG());
          console.log(`[SELFTEST] screenshot: ${shotPath}`);
        } catch (err) {
          console.log(`[SELFTEST] screenshot failed: ${err.message}`);
        }
      }
      const shot2 = process.env.ZYZOOM_SELFTEST_SHOT2;
      if (shot2) {
        createSnifferWindow();
        setTimeout(async () => {
          try {
            const image = await snifferWindow.webContents.capturePage();
            require('fs').writeFileSync(shot2, image.toPNG());
            console.log(`[SELFTEST] screenshot2: ${shot2}`);
          } catch (err) {
            console.log(`[SELFTEST] screenshot2 failed: ${err.message}`);
          }
          app.exit(0);
        }, 2500);
        return;
      }
      console.log('[SELFTEST] windows:', BrowserWindow.getAllWindows().length);
      app.exit(0);
    }, 4000);
  }

  if (isDev) mainWindow.webContents.openDevTools({ mode: 'detach' });

  mainWindow.on('closed', () => {
    mainWindow = null;
  });
}

function createSnifferWindow() {
  if (snifferWindow && !snifferWindow.isDestroyed()) {
    snifferWindow.focus();
    return;
  }
  snifferWindow = new BrowserWindow({
    width: 1180,
    height: 820,
    minWidth: 900,
    minHeight: 620,
    show: false,
    frame: false,
    backgroundColor: '#070d19',
    title: 'Zyzoom Media Sniffer',
    icon: path.join(__dirname, '..', 'renderer', 'assets', 'logo.png'),
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false,
    },
  });

  snifferWindow.loadURL(rendererUrl('sniffer.html'));
  snifferWindow.once('ready-to-show', () => snifferWindow.show());
  snifferWindow.on('closed', () => {
    snifferWindow = null;
  });
}

function send(channel, payload) {
  const targets = [mainWindow, snifferWindow].filter((win) => win && !win.isDestroyed());
  for (const win of targets) win.webContents.send(channel, payload);
}

app.whenReady().then(() => {
  nativeTheme.themeSource = 'dark';
  engine.on('changed', (list) => send('zyzoom:downloads:changed', list));
  engine.on('captures', (list) => send('zyzoom:captures:changed', list));
  engine.start();

  bridge = new CaptureBridge({
    onCapture: (items) => {
      let accepted = 0;
      for (const item of items) accepted += engine.addCapture(item);
      return accepted;
    },
  });
  bridge.start().catch((err) => console.log('[bridge] failed:', err.message));

  ipcMain.handle('zyzoom:downloads:list', () => engine.list());
  ipcMain.handle('zyzoom:downloads:add', (_event, payload) => engine.add(payload || {}));
  ipcMain.handle('zyzoom:downloads:remove', (_event, id) => {
    engine.remove(id);
    return true;
  });
  ipcMain.handle('zyzoom:downloads:action', (_event, { id, action }) => {
    engine.setStatus(id, action);
    return true;
  });
  ipcMain.handle('zyzoom:downloads:bulk-action', (_event, { ids, action }) => {
    for (const id of ids || []) engine.setStatus(id, action);
    return true;
  });
  ipcMain.handle('zyzoom:sniffer:open', () => {
    createSnifferWindow();
    return true;
  });
  ipcMain.handle('zyzoom:settings:update', (_event, options) => {
    engine.setOptions(options || {});
    return true;
  });
  ipcMain.handle('zyzoom:captures:list', () => engine.listCaptures());
  ipcMain.handle('zyzoom:captures:clear', () => {
    engine.clearCaptures();
    return true;
  });
  ipcMain.handle('zyzoom:captures:download', (_event, id) => {
    const capture = engine.listCaptures().find((item) => item.id === id);
    if (!capture) return null;
    let name = 'media.bin';
    try {
      const base = new URL(capture.url).pathname.split('/').pop();
      if (base) name = decodeURIComponent(base);
    } catch (err) {
      /* تجاهل */
    }
    const record = engine.add({
      url: capture.url,
      name,
      description: capture.title || capture.pageUrl || '',
      browser: 'Zyzoom Capture',
      type: capture.kind === 'audio' ? 'صوت' : 'فيديو',
    });
    engine.update(capture.id, { status: 'queued' });
    return record;
  });
  ipcMain.handle('zyzoom:bridge:info', () => ({
    host: '127.0.0.1',
    port: bridge ? bridge.port : null,
    configPath: bridge ? bridge.configPath : null,
    manifests: hostManifestPaths(),
    nativeHostScript: path.join(__dirname, '..', '..', 'native-host', 'zyzoom_native_host.js'),
  }));
  ipcMain.handle('zyzoom:bridge:install-host', async (_event, extensionId) => {
    const hostPath = path.join(__dirname, '..', '..', 'native-host', 'zyzoom_native_host.js');
    try {
      await require('fs').promises.chmod(hostPath, 0o755);
    } catch (err) {
      /* تجاهل */
    }
    const result = await writeHostManifests({ hostPath, extensionId: extensionId || '' });
    return result;
  });
  ipcMain.handle('zyzoom:window:control', (_event, action) => {
    const win = BrowserWindow.getFocusedWindow() || mainWindow;
    if (!win) return false;
    if (action === 'minimize') win.minimize();
    else if (action === 'maximize') {
      if (win.isMaximized()) win.unmaximize();
      else win.maximize();
    } else if (action === 'close') win.close();
    else if (action === 'maximized') return win.isMaximized();
    return true;
  });
  ipcMain.handle('zyzoom:window:state', () => {
    const win = BrowserWindow.getFocusedWindow() || mainWindow;
    return win ? win.isMaximized() : false;
  });
  ipcMain.handle('zyzoom:shell:open', (_event, url) => {
    if (typeof url === 'string' && /^(https?|file):\/\//i.test(url)) shell.openExternal(url);
    return true;
  });

  createMainWindow();

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createMainWindow();
  });
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit();
});

app.on('before-quit', () => {
  engine.stop();
  if (bridge) bridge.stop();
});
