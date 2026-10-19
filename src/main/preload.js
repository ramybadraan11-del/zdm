'use strict';

const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('zyzoom', {
  downloads: {
    list: () => ipcRenderer.invoke('zyzoom:downloads:list'),
    add: (payload) => ipcRenderer.invoke('zyzoom:downloads:add', payload),
    remove: (id) => ipcRenderer.invoke('zyzoom:downloads:remove', id),
    action: (id, action) => ipcRenderer.invoke('zyzoom:downloads:action', { id, action }),
    bulkAction: (ids, action) => ipcRenderer.invoke('zyzoom:downloads:bulk-action', { ids, action }),
    onChanged: (callback) => {
      const listener = (_event, list) => callback(list);
      ipcRenderer.on('zyzoom:downloads:changed', listener);
      return () => ipcRenderer.removeListener('zyzoom:downloads:changed', listener);
    },
  },
  sniffer: {
    open: () => ipcRenderer.invoke('zyzoom:sniffer:open'),
  },
  settings: {
    update: (options) => ipcRenderer.invoke('zyzoom:settings:update', options),
  },
  captures: {
    list: () => ipcRenderer.invoke('zyzoom:captures:list'),
    clear: () => ipcRenderer.invoke('zyzoom:captures:clear'),
    download: (id) => ipcRenderer.invoke('zyzoom:captures:download', id),
    onChanged: (callback) => {
      const listener = (_event, list) => callback(list);
      ipcRenderer.on('zyzoom:captures:changed', listener);
      return () => ipcRenderer.removeListener('zyzoom:captures:changed', listener);
    },
  },
  bridge: {
    info: () => ipcRenderer.invoke('zyzoom:bridge:info'),
    installHost: (extensionId) => ipcRenderer.invoke('zyzoom:bridge:install-host', extensionId),
  },
  window: {
    control: (action) => ipcRenderer.invoke('zyzoom:window:control', action),
    isMaximized: () => ipcRenderer.invoke('zyzoom:window:state'),
  },
  shell: {
    openExternal: (url) => ipcRenderer.invoke('zyzoom:shell:open', url),
  },
});
