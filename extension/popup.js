'use strict';

function render(ok, message) {
  document.getElementById('dot').classList.toggle('ok', ok);
  document.getElementById('status').textContent = message;
}

async function refresh() {
  try {
    const stored = await chrome.storage.local.get(['lastError', 'lastCaptureAt']);
    const count = stored.lastCaptureAt ? 1 : 0;
    document.getElementById('count').textContent = String(count);
    if (stored.lastError) {
      render(false, stored.lastError);
      return;
    }
  } catch (err) {
    /* تجاهل */
  }
  render(true, 'الامتداد يعمل — ابدأ التصفّح');
}

document.getElementById('ping').addEventListener('click', async () => {
  render(false, 'جارٍ الفحص…');
  try {
    const response = await chrome.runtime.sendMessage({ type: 'zyzoom-ping' });
    render(Boolean(response && response.ok), 'تم إرسال إشارة إلى التطبيق');
  } catch (err) {
    render(false, 'تعذّر الاتصال بالتطبيق');
  }
});

refresh();
