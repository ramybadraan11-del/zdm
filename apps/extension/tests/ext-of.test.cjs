const test = require('node:test');
const assert = require('node:assert/strict');
require('../lib/ext-of.js');
const extOf = globalThis.extOf;

test('plain filenames', () => {
  assert.equal(extOf('movie.MP4'), 'mp4');
  assert.equal(extOf('archive.tar.gz'), 'gz');
  assert.equal(extOf('/home/u/file.exe'), 'exe');
});

test('urls with query and hash fragments', () => {
  assert.equal(extOf('https://cdn.x.com/a/b/setup.exe?sig=abc&t=1#frag'), 'exe');
  // A query parameter is not a file name: no extension on the URL path.
  assert.equal(extOf('https://x.com/dl?file=Report.PDF'), '');
});

test('urls without an extension get a fallback for directory paths', () => {
  assert.equal(extOf('https://x.com/get/'), '');
  assert.equal(extOf('https://x.com/'), '');
  assert.equal(extOf('https://x.com/path/noext'), '');
});

test('weird dots are sanitised, not trusted', () => {
  assert.equal(extOf('a...mp4'), 'mp4');
  assert.equal(extOf('a.e!x@e'), 'exe');
});

test('missing and junk input', () => {
  assert.equal(extOf(''), '');
  assert.equal(extOf(undefined), '');
  assert.equal(extOf('not a url %broken'), '');
});

test('encoded names', () => {
  assert.equal(extOf('%20My%20Photo.JPEG'), 'jpeg');
});
