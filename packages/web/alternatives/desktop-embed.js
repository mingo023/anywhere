// Turns `<div class="aw-desktop" data-screen="…">` into the landing page's desktop window. demo/desktop.js mounts
// the app into it and scales it to the frame's width, so this must run before demo/desktop.js.
(() => {
const host = document.querySelector('.aw-desktop');
if (!host) return;
const screen = host.dataset.screen || 'sessions';
host.innerHTML = `<div class="desktop-frame" role="region" aria-label="Interactive preview of the Anywhere desktop app" tabindex="0">
  <figure class="window desktop" data-screen="${screen}" aria-label="Anywhere desktop app"><span class="lights" aria-hidden="true"><i></i><i></i><i></i></span></figure>
</div>`;
})();
