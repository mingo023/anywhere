// A working copy of the desktop window for the hero, drawn at the app's 1440×900pt size and scaled to fit.
// Icons are inlined from packages/desktop/crates/theme/assets so the page also works from file://.

const L = (d) => `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${d}</svg>`;
const F = (d) => `<svg viewBox="0 0 24 24" fill="currentColor"><path d="${d}"/></svg>`;

const ICONS = {
  bell: L('<path d="M10.268 21a2 2 0 0 0 3.464 0"/><path d="M3.262 15.326A1 1 0 0 0 4 17h16a1 1 0 0 0 .74-1.673C19.41 13.956 18 12.499 18 8A6 6 0 0 0 6 8c0 4.499-1.411 5.956-2.738 7.326"/>'),
  'sidebar-collapse': L('<rect width="18" height="18" x="3" y="3" rx="2"/><path d="M9 3v18"/><path d="m16 15-3-3 3-3"/>'),
  search: L('<path d="m21 21-4.34-4.34"/><circle cx="11" cy="11" r="8"/>'),
  bolt: L('<path d="M15.914 4a1.5 1.5 0 00-2.474-1.561l-9 9A1.5 1.5 0 005.5 14h4.002a.5.5 0 01.471.666L8.086 20a1.5 1.5 0 002.475 1.56l9-9A1.5 1.5 0 0018.5 10h-3.997a.5.5 0 01-.472-.667z"/>'),
  plus: L('<path d="M5 12h14"/><path d="M12 5v14"/>'),
  laptop: L('<path d="M18 5a2 2 0 0 1 2 2v8.526a2 2 0 0 0 .212.897l1.068 2.127a1 1 0 0 1-.9 1.45H3.62a1 1 0 0 1-.9-1.45l1.068-2.127A2 2 0 0 0 4 15.526V7a2 2 0 0 1 2-2z"/><path d="M20.054 15.987H3.946"/>'),
  worktree: L('<circle cx="12" cy="18" r="3"/><circle cx="6" cy="6" r="3"/><circle cx="18" cy="6" r="3"/><path d="M18 9v2c0 .6-.4 1-1 1H7c-.6 0-1-.4-1-1V9"/><path d="M12 12v3"/>'),
  'pull-request': L('<circle cx="18" cy="18" r="3"/><circle cx="6" cy="6" r="3"/><path d="M13 6h3a2 2 0 0 1 2 2v7"/><line x1="6" x2="6" y1="9" y2="21"/>'),
  merge: L('<circle cx="18" cy="18" r="3"/><circle cx="6" cy="6" r="3"/><path d="M6 21V9a9 9 0 0 0 9 9"/>'),
  check: L('<path d="M20 6 9 17l-5-5"/>'),
  'x-bold': L('<path stroke-width="3" d="M18 6 6 18"/><path stroke-width="3" d="m6 6 12 12"/>'),
  branch: L('<path d="M15 6a9 9 0 0 0-9 9V3"/><circle cx="18" cy="6" r="3"/><circle cx="6" cy="18" r="3"/>'),
  claude: F('m4.7144 15.9555 4.7174-2.6471.079-.2307-.079-.1275h-.2307l-.7893-.0486-2.6956-.0729-2.3375-.0971-2.2646-.1214-.5707-.1215-.5343-.7042.0546-.3522.4797-.3218.686.0608 1.5179.1032 2.2767.1578 1.6514.0972 2.4468.255h.3886l.0546-.1579-.1336-.0971-.1032-.0972L6.973 9.8356l-2.55-1.6879-1.3356-.9714-.7225-.4918-.3643-.4614-.1578-1.0078.6557-.7225.8803.0607.2246.0607.8925.686 1.9064 1.4754 2.4893 1.8336.3643.3035.1457-.1032.0182-.0728-.164-.2733-1.3539-2.4467-1.445-2.4893-.6435-1.032-.17-.6194c-.0607-.255-.1032-.4674-.1032-.7285L6.287.1335 6.6997 0l.9957.1336.419.3642.6192 1.4147 1.0018 2.2282 1.5543 3.0296.4553.8985.2429.8318.091.255h.1579v-.1457l.1275-1.706.2368-2.0947.2307-2.6957.0789-.7589.3764-.9107.7468-.4918.5828.2793.4797.686-.0668.4433-.2853 1.8517-.5586 2.9021-.3643 1.9429h.2125l.2429-.2429.9835-1.3053 1.6514-2.0643.7286-.8196.85-.9046.5464-.4311h1.0321l.759 1.1293-.34 1.1657-1.0625 1.3478-.8804 1.1414-1.2628 1.7-.7893 1.36.0729.1093.1882-.0183 2.8535-.607 1.5421-.2794 1.8396-.3157.8318.3886.091.3946-.3278.8075-1.967.4857-2.3072.4614-3.4364.8136-.0425.0304.0486.0607 1.5482.1457.6618.0364h1.621l3.0175.2247.7892.522.4736.6376-.079.4857-1.2142.6193-1.6393-.3886-3.825-.9107-1.3113-.3279h-.1822v.1093l1.0929 1.0686 2.0035 1.8092 2.5075 2.3314.1275.5768-.3218.4554-.34-.0486-2.2039-1.6575-.85-.7468-1.9246-1.621h-.1275v.17l.4432.6496 2.3436 3.5214.1214 1.0807-.17.3521-.6071.2125-.6679-.1214-1.3721-1.9246L14.38 17.959l-1.1414-1.9428-.1397.079-.674 7.2552-.3156.3703-.7286.2793-.6071-.4614-.3218-.7468.3218-1.4753.3886-1.9246.3157-1.53.2853-1.9004.17-.6314-.0121-.0425-.1397.0182-1.4328 1.9672-2.1796 2.9446-1.7243 1.8456-.4128.164-.7164-.3704.0667-.6618.4008-.5889 2.386-3.0357 1.4389-1.882.929-1.0868-.0062-.1579h-.0546l-6.3385 4.1164-1.1293.1457-.4857-.4554.0608-.7467.2307-.2429 1.9064-1.3114Z'),
  openai: F('M22.2819 9.8211a5.9847 5.9847 0 0 0-.5157-4.9108 6.0462 6.0462 0 0 0-6.5098-2.9A6.0651 6.0651 0 0 0 4.9807 4.1818a5.9847 5.9847 0 0 0-3.9977 2.9 6.0462 6.0462 0 0 0 .7427 7.0966 5.98 5.98 0 0 0 .511 4.9107 6.051 6.051 0 0 0 6.5146 2.9001A5.9847 5.9847 0 0 0 13.2599 24a6.0557 6.0557 0 0 0 5.7718-4.2058 5.9894 5.9894 0 0 0 3.9977-2.9001 6.0557 6.0557 0 0 0-.7475-7.0729zm-9.022 12.6081a4.4755 4.4755 0 0 1-2.8764-1.0408l.1419-.0804 4.7783-2.7582a.7948.7948 0 0 0 .3927-.6813v-6.7369l2.02 1.1686a.071.071 0 0 1 .038.052v5.5826a4.504 4.504 0 0 1-4.4945 4.4944zm-9.6607-4.1254a4.4708 4.4708 0 0 1-.5346-3.0137l.142.0852 4.783 2.7582a.7712.7712 0 0 0 .7806 0l5.8428-3.3685v2.3324a.0804.0804 0 0 1-.0332.0615L9.74 19.9502a4.4992 4.4992 0 0 1-6.1408-1.6464zM2.3408 7.8956a4.485 4.485 0 0 1 2.3655-1.9728V11.6a.7664.7664 0 0 0 .3879.6765l5.8144 3.3543-2.0201 1.1685a.0757.0757 0 0 1-.071 0l-4.8303-2.7865A4.504 4.504 0 0 1 2.3408 7.872zm16.5963 3.8558L13.1038 8.364 15.1192 7.2a.0757.0757 0 0 1 .071 0l4.8303 2.7913a4.4944 4.4944 0 0 1-.6765 8.1042v-5.6772a.79.79 0 0 0-.407-.667zm2.0107-3.0231l-.142-.0852-4.7735-2.7818a.7759.7759 0 0 0-.7854 0L9.409 9.2297V6.8974a.0662.0662 0 0 1 .0284-.0615l4.8303-2.7866a4.4992 4.4992 0 0 1 6.6802 4.66zM8.3065 12.863l-2.02-1.1638a.0804.0804 0 0 1-.038-.0567V6.0742a4.4992 4.4992 0 0 1 7.3757-3.4537l-.142.0805L8.704 5.459a.7948.7948 0 0 0-.3927.6813zm1.0976-2.3654l2.602-1.4998 2.6069 1.4998v2.9994l-2.5974 1.4997-2.6067-1.4997Z'),
  sparkle: L('<path d="M11.017 2.814a1 1 0 0 1 1.966 0l1.051 5.558a2 2 0 0 0 1.594 1.594l5.558 1.051a1 1 0 0 1 0 1.966l-5.558 1.051a2 2 0 0 0-1.594 1.594l-1.051 5.558a1 1 0 0 1-1.966 0l-1.051-5.558a2 2 0 0 0-1.594-1.594l-5.558-1.051a1 1 0 0 1 0-1.966l5.558-1.051a2 2 0 0 0 1.594-1.594z"/>'),
  shield: L('<path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z"/>'),
  'arrow-up': L('<path d="m5 12 7-7 7 7"/><path d="M12 19V5"/>'),
  prompt: L('<path d="M12 19h8"/><path d="m4 17 6-6-6-6"/>'),
  'split-right': L('<rect width="18" height="18" x="3" y="3" rx="2"/><path d="M12 3v18"/>'),
  'split-down': L('<rect width="18" height="18" x="3" y="3" rx="2"/><path d="M3 12h18"/>'),
  more: L('<circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/><circle cx="5" cy="12" r="1"/>'),
  sessions: L('<path d="M2.992 16.342a2 2 0 0 1 .094 1.167l-1.065 3.29a1 1 0 0 0 1.236 1.168l3.413-.998a2 2 0 0 1 1.099.092 10 10 0 1 0-4.777-4.719"/><path d="M8 12h.01"/><path d="M12 12h.01"/><path d="M16 12h.01"/>'),
  explorer: L('<path d="M15 2h-4a2 2 0 0 0-2 2v11a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2V8"/><path d="M16.706 2.706A2.4 2.4 0 0 0 15 2v5a1 1 0 0 0 1 1h5a2.4 2.4 0 0 0-.706-1.706z"/><path d="M5 7a2 2 0 0 0-2 2v11a2 2 0 0 0 2 2h8a2 2 0 0 0 1.732-1"/>'),
  'chevron-right': L('<path d="m9 18 6-6-6-6"/>'),
  'chevron-down': L('<path d="m6 9 6 6 6-6"/>'),
};

const FOLDER = 'm6.922 3.768-.644-.536A1 1 0 0 0 5.638 3H2a1 1 0 0 0-1 1v8a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1V5a1 1 0 0 0-1-1H7.562a1 1 0 0 1-.64-.232';
const FOLDER_OPEN = 'M14.483 6H4.721a1 1 0 0 0-.949.684L2 12V5h12a1 1 0 0 0-1-1H7.562a1 1 0 0 1-.64-.232l-.644-.536A1 1 0 0 0 5.638 3H2a1 1 0 0 0-1 1v8a1 1 0 0 0 1 1h11l2.403-5.606A1 1 0 0 0 14.483 6';
const FOLDERS = {
  components: ['#c0ca33', '#f0f4c3', 'M6 10h4v4H6zm5 0h4v4h-4zM6 5h4v4H6zm4.172 2L13 4.172 15.829 7 13 9.829z'],
  hooks: ['#7e57c2', '#d1c4e9', 'M12.5 5A1.5 1.5 0 0 0 11 6.5a1.5 1.5 0 0 0 1 1.41V12a1 1 0 0 1-2 0v-1h1L9 9v3a2 2 0 0 0 4 0V7.91a1.5 1.5 0 0 0 1-1.41A1.5 1.5 0 0 0 12.5 5m0 1a.5.5 0 1 1 0 1 .5.5 0 0 1 0-1'],
  src: ['#4caf50', '#c8e6c9', 'M9.225 15a.5.5 0 0 1-.12-.014.57.568 0 0 1-.414-.661l1.549-7.872a.566.565 0 0 1 .254-.372.53.53 0 0 1 .4-.067.57.57 0 0 1 .415.662l-1.552 7.872a.56.56 0 0 1-.253.371.53.53 0 0 1-.28.081m3.105-1h-.038a.54.54 0 0 1-.382-.206.583.582 0 0 1 .057-.774l2.664-2.483-2.653-2.312a.583.582 0 0 1-.08-.772.54.54 0 0 1 .377-.218.53.53 0 0 1 .406.129l3.126 2.727a.579.578 0 0 1 .002.862l-3.114 2.904a.536.535 0 0 1-.365.144zm-4.661 0a.536.535 0 0 1-.365-.146L4.186 10.95a.58.58 0 0 1-.005-.846l.01-.01 3.128-2.726a.516.515 0 0 1 .4-.13.54.54 0 0 1 .38.218.583.582 0 0 1-.08.773l-2.65 2.31 2.663 2.482a.579.578 0 0 1 .056.774.536.535 0 0 1-.381.206z'],
};
const folder = (name, open) => {
  const [color, motiveColor, motive] = FOLDERS[name];
  return `<svg viewBox="0 0 16 16"><path fill="${color}" d="${open ? FOLDER_OPEN : FOLDER}"/><path fill="${motiveColor}" d="${motive}"/></svg>`;
};
const FILES = {
  ts: '<svg viewBox="0 0 16 16"><path fill="#0288d1" d="M2 2v12h12V2zm4 6h3v1H8v4H7V9H6zm5 0h2v1h-2v1h1a1.003 1.003 0 0 1 1 1v1a1.003 1.003 0 0 1-1 1h-2v-1h2v-1h-1a1.003 1.003 0 0 1-1-1V9a1.003 1.003 0 0 1 1-1"/></svg>',
  tsx: '<svg viewBox="0 0 32 32" fill="#0288d1"><path d="M16 12c7.444 0 12 2.59 12 4s-4.556 4-12 4-12-2.59-12-4 4.556-4 12-4m0-2c-7.732 0-14 2.686-14 6s6.268 6 14 6 14-2.686 14-6-6.268-6-14-6"/><path d="M16 14a2 2 0 1 0 2 2 2 2 0 0 0-2-2"/><path d="M10.458 5.507c2.017 0 5.937 3.177 9.006 8.493 3.722 6.447 3.757 11.687 2.536 12.392a.9.9 0 0 1-.457.1c-2.017 0-5.938-3.176-9.007-8.492C8.814 11.553 8.779 6.313 10 5.608a.9.9 0 0 1 .458-.1m-.001-2A2.87 2.87 0 0 0 9 3.875C6.13 5.532 6.938 12.304 10.804 19c3.284 5.69 7.72 9.493 10.74 9.493A2.87 2.87 0 0 0 23 28.124c2.87-1.656 2.062-8.428-1.804-15.124-3.284-5.69-7.72-9.493-10.74-9.493Z"/><path d="M21.543 5.507a.9.9 0 0 1 .457.1c1.221.706 1.186 5.946-2.536 12.393-3.07 5.316-6.99 8.493-9.007 8.493a.9.9 0 0 1-.457-.1C8.779 25.686 8.814 20.446 12.536 14c3.07-5.316 6.99-8.493 9.007-8.493m0-2c-3.02 0-7.455 3.804-10.74 9.493C6.939 19.696 6.13 26.468 9 28.124a2.87 2.87 0 0 0 1.457.369c3.02 0 7.455-3.804 10.74-9.493C25.061 12.304 25.87 5.532 23 3.876a2.87 2.87 0 0 0-1.457-.369"/></svg>',
  json: '<svg viewBox="0 -960 960 960"><path fill="#f9a825" d="M560-160v-80h120q17 0 28.5-11.5T720-280v-80q0-38 22-69t58-44v-14q-36-13-58-44t-22-69v-80q0-17-11.5-28.5T680-720H560v-80h120q50 0 85 35t35 85v80q0 17 11.5 28.5T840-560h40v160h-40q-17 0-28.5 11.5T800-360v80q0 50-35 85t-85 35zm-280 0q-50 0-85-35t-35-85v-80q0-17-11.5-28.5T120-400H80v-160h40q17 0 28.5-11.5T160-600v-80q0-50 35-85t85-35h120v80H280q-17 0-28.5 11.5T240-680v80q0 38-22 69t-58 44v14q36 13 58 44t22 69v80q0 17 11.5 28.5T280-240h120v80z"/></svg>',
  md: '<svg viewBox="0 0 16 16"><path fill="#42a5f5" d="M8 1C4.136 1 1 4.136 1 8s3.136 7 7 7 7-3.136 7-7-3.136-7-7-7m1 11H7V7.5h2zm0-6H7V4h2z"/></svg>',
};

const sized = (svg, s) => svg.replace('<svg', `<svg width="${s}" height="${s}"`);
const ic = (name, s = 16) => sized(ICONS[name], s);
const fileIcon = (name) => sized(FILES[name.split('.').pop()], 16);
const esc = (s) => s.replace(/[&<>"]/g, (c) => `&#${c.charCodeAt(0)};`);

const AGENTS = { claude: ['claude', 'Claude Code'], codex: ['openai', 'Codex'] };

// Demo data only: never the names of real projects.
const PROJECTS = [
  { name: 'orbit-web', emoji: '🚀', open: true, rows: [
    { label: 'Local', local: true },
    { label: 'Add dark mode to settings', status: 'working',
      log: [['you', 'Add a dark mode toggle to the settings screen'], ['say', "I'll add a theme setting and wire it to the existing tokens."], ['tool', 'Read', 'src/settings.ts']],
      script: [['tool', 'Read', 'src/theme/tokens.ts'], ['tool', 'Write', 'hooks/useTheme.ts'], ['out', 'Wrote 24 lines'], ['tool', 'Update', 'src/settings.ts'], ['out', 'Updated with 18 additions and 2 removals'], ['tool', 'Bash', 'pnpm test settings'], ['out', '12 passed'], ['say', 'Done. Settings now offers Light, Dark and System, and remembers the choice.']] },
    { label: 'Fix flaky checkout test', status: 'needs',
      log: [['you', 'The checkout test fails about one run in five. Find out why.'], ['tool', 'Read', 'tests/checkout.spec.ts'], ['say', 'It waits a fixed 500ms for the cart total. I want to run it 20 times to confirm.']],
      ask: { cmd: 'pnpm test checkout --repeat 20', desc: 'Run the checkout spec 20 times', rule: 'pnpm test',
        after: [['tool', 'Bash', 'pnpm test checkout --repeat 20'], ['err', '4 failed, 16 passed'], ['tool', 'Update', 'tests/checkout.spec.ts'], ['out', 'Wait for the total to render instead of sleeping'], ['tool', 'Bash', 'pnpm test checkout --repeat 20'], ['out', '20 passed'], ['say', 'Fixed. The test now waits for the cart total instead of a timer.']] } },
    { label: 'Migrate to React 19', pr: 'open',
      log: [['you', 'Upgrade us to React 19'], ['tool', 'Bash', 'pnpm up react@19 react-dom@19'], ['tool', 'Update', 'src/app.tsx'], ['out', 'Replaced ReactDOM.render with createRoot'], ['tool', 'Bash', 'gh pr create --fill'], ['say', 'Opened PR #214: Migrate to React 19.']] },
  ] },
  { name: 'fox-api', emoji: '🦊', open: true, rows: [
    { label: 'Local', local: true },
    { label: 'Rate-limit the upload route', status: 'done',
      log: [['you', 'Rate-limit POST /uploads to 10 a minute per user'], ['tool', 'Update', 'src/routes/uploads.ts'], ['out', 'Updated with 21 additions'], ['tool', 'Bash', 'pnpm test uploads'], ['out', '9 passed'], ['say', 'Done. Over the limit now returns 429 with Retry-After.']] },
    { label: 'Cache invoice queries', status: 'failed', agent: 'codex',
      log: [['you', 'Cache the invoice list queries'], ['tool', 'Update', 'src/invoices/list.ts'], ['tool', 'Bash', 'pnpm test invoices'], ['err', '2 failed: cached totals are stale after a refund']] },
    { label: 'Bump the Postgres driver', pr: 'merged',
      log: [['you', 'Bump pg to the latest minor'], ['tool', 'Bash', 'pnpm up pg'], ['tool', 'Bash', 'pnpm test'], ['out', '131 passed'], ['say', 'PR #88 is merged.']] },
  ] },
  { name: 'garden-mobile', emoji: '🌱', open: true, rows: [
    { label: 'Local', local: true },
    { label: 'Crash when resuming a download', status: 'needs',
      log: [['you', 'The app crashes when a paused download resumes'], ['tool', 'Read', 'src/downloads/queue.ts'], ['say', 'The resume handler reads a task that was already freed. I want to reproduce it with the queue tests.']],
      ask: { cmd: 'pnpm test queue --runInBand', desc: 'Run the download queue tests', rule: 'pnpm test',
        after: [['tool', 'Bash', 'pnpm test queue --runInBand'], ['err', '1 failed: resume after cancel'], ['tool', 'Update', 'src/downloads/queue.ts'], ['out', 'Drop the task from the queue before freeing it'], ['tool', 'Bash', 'pnpm test queue --runInBand'], ['out', '17 passed'], ['say', 'Fixed. Resuming a cancelled download no longer touches the freed task.']] } },
    { label: 'Haptics on long press',
      log: [['you', 'Add a light haptic when you long-press a plant'], ['tool', 'Update', 'components/PlantCard.tsx'], ['say', 'Added. Want the same on the watering button?']] },
  ] },
];

const STARTERS = {
  'Review my uncommitted changes': [['tool', 'Bash', 'git diff --stat'], ['out', '3 files changed, 48 insertions(+), 6 deletions(-)'], ['tool', 'Read', 'src/settings.ts'], ['tool', 'Read', 'components/Sheet.tsx'], ['say', 'Looks good to commit. One note: Sheet.tsx drops its focus trap when it closes early.']],
  'Fix the failing tests': [['tool', 'Bash', 'pnpm test'], ['err', '2 failed, 46 passed'], ['tool', 'Update', 'src/store.test.ts'], ['out', 'Updated with 4 additions and 4 removals'], ['tool', 'Bash', 'pnpm test'], ['out', '48 passed'], ['say', 'Fixed. The snapshots expected the old date format.']],
  'Summarise what this branch changes': [['tool', 'Bash', 'git log main..HEAD --oneline'], ['out', '4 commits'], ['tool', 'Bash', 'git diff main --stat'], ['out', '3 files changed, 48 insertions(+), 6 deletions(-)'], ['say', 'This branch adds a dark theme, a toggle for it in Settings, and remembers the choice.']],
  'Write the commit message': [['tool', 'Bash', 'git diff --staged'], ['out', '3 files changed'], ['say', 'Add dark mode to settings'], ['say', 'Settings offers Light, Dark and System. The choice is saved and applied before first paint.']],
};
const GENERIC = [['say', "I'll look around the repo first."], ['tool', 'Search', '3 patterns'], ['out', 'Found 7 files'], ['tool', 'Read', 'src/app.tsx'], ['tool', 'Update', 'src/app.tsx'], ['out', 'Updated with 12 additions and 3 removals'], ['tool', 'Bash', 'pnpm test'], ['out', '48 passed'], ['say', 'Done. Tests pass.']];

const TREE = [
  { name: 'components', kids: [['Button.tsx'], ['Sheet.tsx', 'M']] },
  { name: 'hooks', kids: [['useTheme.ts', 'A']] },
  { name: 'src', kids: [['app.tsx'], ['settings.ts', 'M'], ['store.ts']] },
  ['package.json'],
  ['README.md'],
];
const CHANGES = [['src/settings.ts', 'M'], ['hooks/useTheme.ts', 'A'], ['components/Sheet.tsx', 'M']];
const URGENCY = ['needs', 'failed', 'done', 'working', undefined];

let ids = 0;
const adopt = (r, p) => Object.assign(r, { id: ids++, p, agent: r.agent ?? 'claude', log: r.log ?? [] });
PROJECTS.forEach((p) => p.rows.forEach((r) => adopt(r, p)));
const all = () => PROJECTS.flatMap((p) => p.rows);
const needing = () => all().filter((r) => r.status === 'needs');

let sel = PROJECTS[0].rows[0];
let agent = 'claude';
let panel = 'explorer';
let draft = '';
const opened = new Set(['src']);

function mark(r) {
  if (r.status === 'needs') return '<i class="need"></i>';
  if (r.status === 'working') return '<i class="spin"></i>';
  if (r.status === 'done') return `<span class="ok">${ic('check', 11)}</span>`;
  if (r.status === 'failed') return `<span class="fail">${ic('x-bold', 11)}</span>`;
  if (r.pr === 'open') return `<span class="pr">${ic('pull-request', 13)}</span>`;
  if (r.pr === 'merged') return `<span class="merged">${ic('merge', 13)}</span>`;
  return ic(r.local ? 'laptop' : 'worktree', 13);
}

const row = (r, trail = '') =>
  `<button class="row wt${r === sel ? ' on' : ''}" data-act="sel" data-v="${r.id}"><span class="glyph">${mark(r)}</span><span class="name">${esc(r.label)}</span>${trail}</button>`;

const tileBadge = (p) =>
  p.rows.some((r) => r.status === 'needs') ? '<i class="alert"></i>'
  : p.rows.some((r) => r.status === 'working') ? '<i class="busy"><i class="spin"></i></i>'
  : '';

function side() {
  const n = needing().length;
  $('.side').innerHTML = `
    <div class="side-head">
      <button class="ib" data-act="bell" aria-label="Needs you">${ic('bell')}${n ? `<b class="badge">${n}</b>` : ''}</button>
      <button class="ib" aria-label="Hide sidebar">${ic('sidebar-collapse')}</button>
    </div>
    <div class="field">${ic('search', 15)}<span>Search</span><kbd>⌘K</kbd></div>
    <div class="row"><span class="glyph">${ic('bolt', 15)}</span><span class="name">Automations</span></div>
    <div class="label">Projects<button class="ib" aria-label="Add project">${ic('plus', 14)}</button></div>
    ${PROJECTS.map((p, i) => `
      <button class="row proj" data-act="fold" data-v="${i}">
        <span class="slot"><span class="tile">${p.emoji}${tileBadge(p)}</span><span class="chev">${ic(p.open ? 'chevron-down' : 'chevron-right', 12)}</span></span>
        <span class="name">${p.name}</span>
      </button>
      ${p.open ? p.rows.map((r) => row(r)).join('') : ''}`).join('')}`;
}

function home() {
  const [icon, name] = AGENTS[agent];
  return `<div class="home">
    <div class="title">What should ${name} work on?</div>
    <div class="branch">${ic('branch', 13)}main · ${CHANGES.length} changed files</div>
    <div class="composer${draft.trim() ? '' : ' empty'}">
      <textarea rows="2" placeholder="Describe a task or paste an error" aria-label="Task">${esc(draft)}</textarea>
      <div class="foot">
        <button class="chip" data-act="agent"><span class="${agent}">${ic(icon, 13)}</span><b>${name}</b>${ic('chevron-down', 12)}</button>
        <button class="chip">${ic('sparkle', 13)}Default model${ic('chevron-down', 12)}</button>
        <button class="chip">${ic('shield', 13)}Agent's settings${ic('chevron-down', 12)}</button>
        <span class="sp"></span><kbd>⌘↵</kbd>
        <button class="send" data-act="send" aria-label="Send">${ic('arrow-up', 15)}</button>
      </div>
    </div>
    <div class="pills">${Object.keys(STARTERS).map((t) => `<button class="pill" data-act="pill">${t}</button>`).join('')}</div>
    <div class="or">or open<button class="link">${ic('prompt', 13)}Terminal<kbd>⌘T</kbd></button></div>
  </div>`;
}

const line = ([kind, a, b]) =>
  kind === 'you' ? `<div class="l you">&gt; ${esc(a)}</div>`
  : kind === 'tool' ? `<div class="l"><i class="ok">●</i><span><b>${a}</b>(${esc(b)})</span></div>`
  : kind === 'out' ? `<div class="l out">⎿  ${esc(a)}</div>`
  : kind === 'err' ? `<div class="l out err">⎿  ${esc(a)}</div>`
  : `<div class="l"><i>●</i><span>${esc(a)}</span></div>`;

const ask = (s) => `<div class="ask">
  <div class="ask-h">Bash command</div>
  <div class="ask-cmd">${s.ask.cmd}</div>
  <div class="ask-d">${s.ask.desc}</div>
  <div class="ask-q">Do you want to proceed?</div>
  <button data-act="ans" data-v="yes">1. Yes</button>
  <button data-act="ans" data-v="yes">2. Yes, and don't ask again for ${s.ask.rule} commands</button>
  <button data-act="ans" data-v="no">3. No, and tell ${AGENTS[s.agent][1]} what to do differently</button>
</div>`;

const term = (s) => `<div class="term">${s.log.map(line).join('')}${
  s.status === 'needs' ? ask(s)
  : s.status === 'working' ? '<div class="l think"><i class="spin"></i><span>Working… <em>esc to interrupt</em></span></div>'
  : '<div class="input">&gt; <i class="caret"></i></div>'}</div>`;

function main() {
  const [icon, name] = AGENTS[sel.agent];
  $('.tabs').innerHTML = sel.local ? '' : `<div class="tab"><span class="${sel.agent}">${ic(icon, 13)}</span>${name}<span class="glyph">${mark(sel)}</span></div>`;
  const body = $('.body');
  body.innerHTML = sel.local ? home() : term(sel);
  body.scrollTop = body.scrollHeight;
}

const dir = ({ name, kids }) => {
  const open = opened.has(name);
  return `<button class="tr" data-act="dir" data-v="${name}"><span class="cv">${ic(open ? 'chevron-down' : 'chevron-right', 12)}</span>${sized(folder(name, open), 16)}<span class="n">${name}</span></button>
    ${open ? kids.map((k) => file(k, 1)).join('') : ''}`;
};
const file = ([name, git], depth = 0, path = '') =>
  `<div class="tr ${git ?? ''}" style="--d:${depth}"><span class="cv"></span>${fileIcon(name)}<span class="n">${name}${path && ` <small>${path}</small>`}</span>${git ? `<b class="git">${git}</b>` : ''}</div>`;

const PANELS = {
  explorer: () => `<div class="head"><span class="t">${sel.p.name}</span><button class="ib" aria-label="New file">${ic('plus', 15)}</button></div>
    <div class="field find">${ic('search', 15)}<span>Go to file…</span><kbd>⌘P</kbd></div>
    <div class="list">${TREE.map((n) => (n.kids ? dir(n) : file(n))).join('')}</div>`,
  changes: () => `<div class="head"><span class="t">Changes</span><span class="stat"><b class="add">+48</b> <b class="del">−6</b></span></div>
    <div class="list">${CHANGES.map(([path, git]) => {
      const name = path.split('/').pop();
      return file([name, git], 0, path.slice(0, -name.length - 1));
    }).join('')}</div>`,
  sessions: () => `<div class="head"><span class="t">Sessions</span></div>
    <div class="list">${all().filter((r) => !r.local)
      .sort((a, b) => URGENCY.indexOf(a.status) - URGENCY.indexOf(b.status))
      .map((r) => row(r, `<span class="from">${r.p.emoji}</span>`)).join('')}</div>`,
};

function right() {
  $('.panel').innerHTML = PANELS[panel]();
  const n = needing().length;
  $('.rail').innerHTML = [['sessions', 'sessions'], ['explorer', 'explorer'], ['changes', 'branch']].map(([v, icon]) =>
    `<button class="${panel === v ? 'on' : ''}" data-act="rail" data-v="${v}" aria-label="${v}">${ic(icon, 17)}${
      v === 'sessions' && n ? '<i class="dot"></i>' : v === 'changes' ? `<b class="count">${CHANGES.length}</b>` : ''}</button>`).join('');
}

function draw() {
  side();
  main();
  right();
}

function refresh(s) {
  side();
  right();
  if (s === sel) main();
}

function play(s, lines, end, ms) {
  s.status = 'working';
  lines.forEach((l, i) => setTimeout(() => {
    s.log.push(l);
    if (i === lines.length - 1) s.status = end;
    refresh(s);
  }, ms * (i + 1)));
  refresh(s);
}

function send(text) {
  text = text.trim();
  if (!text) return;
  const s = adopt({ label: text, agent, log: [['you', text]] }, sel.p);
  sel.p.rows.splice(1, 0, s);
  sel.p.open = true;
  sel = s;
  draft = '';
  play(s, STARTERS[text] ?? GENERIC, 'done', 900);
}

function answer(s, v) {
  if (v === 'yes') return play(s, s.ask.after, 'done', 1000);
  s.status = undefined;
  s.log.push(['say', "Okay, I won't run it. What should I do instead?"]);
  draw();
}

const win = document.querySelector('.hero .window');
const app = document.createElement('div');
const $ = (q) => app.querySelector(q);
app.className = 'app';
app.innerHTML = `<div class="side"></div>
  <div class="pane">
    <div class="bar">
      <div class="tabs"></div>
      <button class="ib" data-act="home" aria-label="New session">${ic('plus', 15)}</button>
      <div class="groups">
        <span class="group"><button aria-label="Split right">${ic('split-right')}</button><button aria-label="Split down">${ic('split-down')}</button></span>
        <span class="group"><button aria-label="More">${ic('more')}</button></span>
      </div>
    </div>
    <div class="body"></div>
  </div>
  <div class="panel"></div>
  <div class="rail"></div>`;

app.addEventListener('click', (e) => {
  const t = e.target.closest('[data-act]');
  if (!t) return;
  const v = t.dataset.v;
  switch (t.dataset.act) {
    case 'sel': sel = all().find((r) => r.id === +v); break;
    case 'fold': PROJECTS[v].open = !PROJECTS[v].open; break;
    case 'bell': sel = needing()[0] ?? sel; break;
    case 'home': sel = sel.p.rows[0]; break;
    case 'agent': agent = agent === 'claude' ? 'codex' : 'claude'; break;
    case 'rail': panel = v; break;
    case 'dir': opened.has(v) ? opened.delete(v) : opened.add(v); break;
    case 'pill': return send(t.textContent);
    case 'send': return send(draft);
    case 'ans': return answer(sel, v);
  }
  draw();
});
app.addEventListener('input', (e) => {
  draft = e.target.value;
  $('.composer').classList.toggle('empty', !draft.trim());
});
app.addEventListener('keydown', (e) => {
  if (e.target.matches('textarea') && e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault();
    send(draft);
  }
});

new ResizeObserver(([e]) => win.style.setProperty('--s', e.contentRect.width / 1440)).observe(win);
win.append(app);
win.classList.add('live');
draw();
const busy = PROJECTS[0].rows[1];
play(busy, busy.script, 'done', 4000);
