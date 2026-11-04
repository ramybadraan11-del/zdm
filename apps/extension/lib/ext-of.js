/**
 * Extension extraction, shared between background.js (via importScripts) and
 * the node test-runner. Kept dependency-free on purpose.
 *
 * @param {string} value filename or full URL
 * @returns {string} lowercase extension without a dot, '' when absent/invalid
 */
var globalScope = typeof self !== 'undefined' ? self : globalThis;

globalScope.extOf = function extOf(value) {
  if (!value) return '';
  var path = '';
  try {
    path = new URL(value, 'http://localhost').pathname || '';
  } catch (e) {
    path = String(value).split(/[?#]/)[0];
  }
  var last = '';
  try {
    last = decodeURIComponent(path.split('/').filter(Boolean).pop() || '');
  } catch (e) {
    last = path.split('/').filter(Boolean).pop() || '';
  }
  var dot = last.lastIndexOf('.');
  if (dot < 0) return '';
  return last.slice(dot + 1).toLowerCase().replace(/[^a-z0-9+]/g, '');
};
