'use strict';
document.documentElement.classList.add('js');
const navToggle = document.querySelector('.nav-toggle');
navToggle.addEventListener('click', () => {
  const open = navToggle.getAttribute('aria-expanded') !== 'true';
  navToggle.setAttribute('aria-expanded', String(open));
  navToggle.closest('aside').classList.toggle('nav-open', open);
});
async function copyText(text, status) {
  try {
    if (!navigator.clipboard || !window.isSecureContext) throw new Error('Clipboard unavailable');
    await navigator.clipboard.writeText(text);
    status.textContent = 'Copied to clipboard.';
  } catch (_) {
    status.textContent = 'Copy unavailable. Select the text and copy it manually.';
  }
}
document.querySelectorAll('pre').forEach((pre) => {
  const code = pre.querySelector('code');
  if (!code) return;
  const button = document.createElement('button');
  button.type = 'button';
  button.textContent = 'Copy code';
  const status = document.createElement('span');
  status.setAttribute('role', 'status');
  status.setAttribute('aria-live', 'polite');
  button.addEventListener('click', () => copyText(code.textContent, status));
  pre.prepend(button, status);
});
const pageButton = document.querySelector('.copy-page');
pageButton.addEventListener('click', async () => {
  const status = document.querySelector('.copy-status');
  try {
    const response = await fetch(pageButton.dataset.raw);
    if (!response.ok) throw new Error('Fetch failed');
    await copyText(await response.text(), status);
  } catch (_) {
    status.textContent = 'Open View Markdown to select and copy the text.';
  }
});
