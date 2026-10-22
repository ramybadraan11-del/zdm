//! ZDM Extension Popup
//! Developed by Ramy Badraan

document.addEventListener('DOMContentLoaded', () => {
  const toggle = document.getElementById('captureToggle');
  const statusIndicator = document.getElementById('statusIndicator');
  const openBtn = document.getElementById('openZdmBtn');

  // Load toggle state
  chrome.storage.local.get({ captureDownloads: true }, (items) => {
    toggle.checked = items.captureDownloads;
  });

  toggle.addEventListener('change', () => {
    chrome.storage.local.set({ captureDownloads: toggle.checked });
  });

  // Check connection to ZDM Desktop local bridge
  fetch('http://127.0.0.1:46800', { mode: 'no-cors' })
    .then(() => {
      statusIndicator.textContent = 'متصل بالتطبيق ✓';
      statusIndicator.style.color = '#10B981';
    })
    .catch(() => {
      statusIndicator.textContent = 'التطبيق مغلق';
      statusIndicator.style.color = '#F59E0B';
    });

  openBtn.addEventListener('click', () => {
    // Ping bridge to focus or open ZDM
    fetch('http://127.0.0.1:46800', {
      method: 'POST',
      body: JSON.stringify({ type: 'focus_window' }),
      mode: 'no-cors',
    }).catch(() => {});
  });
});
