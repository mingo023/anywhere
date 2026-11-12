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
  cursor: F('M19.784 7.046L12.385 2.699C12.268 2.631 12.135 2.595 12 2.595C11.865 2.595 11.733 2.631 11.616 2.699L4.215 7.046C4.117 7.104 4.035 7.187 3.979 7.286C3.922 7.386 3.892 7.5 3.892 7.615L3.892 16.383C3.892 16.618 4.015 16.835 4.215 16.953L11.615 21.301C11.732 21.369 11.865 21.405 12 21.405C12.135 21.405 12.267 21.369 12.384 21.301L19.785 16.953C19.883 16.895 19.965 16.812 20.021 16.712C20.078 16.612 20.108 16.498 20.108 16.383L20.108 7.616C20.108 7.5 20.078 7.387 20.021 7.287C19.964 7.187 19.883 7.104 19.785 7.047L19.784 7.046ZM19.319 7.968L12.175 20.559C12.127 20.644 12 20.609 12 20.511L12 12.266C11.999 12.185 11.978 12.106 11.939 12.035C11.899 11.965 11.841 11.907 11.772 11.866L4.755 7.743C4.673 7.695 4.707 7.565 4.803 7.565L19.091 7.565C19.295 7.565 19.421 7.789 19.319 7.968Z'),
  grok: F('M9.077 15.114L16.676 9.498C17.048 9.222 17.581 9.33 17.758 9.757C18.692 12.013 18.275 14.723 16.416 16.584C14.558 18.445 11.971 18.853 9.607 17.924L7.025 19.121C10.729 21.655 15.226 21.028 18.037 18.213C20.266 15.981 20.957 12.938 20.311 10.195L20.317 10.201C19.381 6.17 20.547 4.559 22.936 1.265C22.993 1.187 23.05 1.109 23.106 1.029L19.962 4.176L19.962 4.167L9.075 15.116M7.509 16.479C4.85 13.936 5.309 10.001 7.577 7.732C9.254 6.053 12.002 5.368 14.401 6.375L16.978 5.184C16.514 4.848 15.919 4.487 15.236 4.233C12.151 2.962 8.457 3.594 5.948 6.104C3.536 8.519 2.777 12.233 4.08 15.403C5.053 17.771 3.458 19.447 1.851 21.138C1.281 21.737 0.71 22.337 0.249 22.971L7.507 16.481'),
  opencode: F('M15.827 6.357L8.303 6.357L8.303 17.643L15.827 17.643L15.827 6.357ZM19.589 21.405L4.541 21.405L4.541 2.595L19.589 2.595L19.589 21.405Z'),
  pi: F('M3.243 3.243L16.865 3.243L16.865 12L12.324 12L12.324 16.378L7.784 16.378L7.784 20.757L3.243 20.757L3.243 3.243ZM7.784 7.622L7.784 12L12.324 12L12.324 7.622L7.784 7.622ZM16.865 12.324L20.757 12.324L20.757 20.757L16.865 20.757L16.865 12.324Z'),
  omp: F('M3.75 5.25L20.25 5.25L20.25 8.625L16.125 8.625L16.125 21L12.75 21L12.75 8.625L9.375 8.625L9.375 16.875L6 16.875L6 8.625L3.75 8.625Z'),
  fx: F('M11.242 2.595C11.881 2.595 12.629 2.766 13.069 2.965L13.318 3.079L12.713 4.845L12.424 4.738C12.088 4.614 11.731 4.503 11.315 4.503C10.891 4.503 10.627 4.595 10.42 4.807C10.197 5.038 10.004 5.45 9.835 6.189L9.703 6.804L11.698 6.804L13.962 6.816L13.994 6.816L14.012 6.842L16.186 10.038L18.345 6.816L21.257 6.816L17.734 11.863L21.405 17.057L18.657 17.057L18.639 17.033L11.634 7.484L11.418 8.561L9.333 8.561L7.356 17.958C7.143 18.985 6.813 19.846 6.28 20.454C5.736 21.074 5.004 21.405 4.055 21.405C3.553 21.405 3.139 21.328 2.796 21.207L2.595 21.136L2.595 19.163L2.674 19.189L2.992 19.295C3.311 19.402 3.581 19.479 3.91 19.479C4.078 19.479 4.219 19.446 4.343 19.38C4.468 19.314 4.579 19.213 4.68 19.073C4.895 18.777 5.066 18.311 5.209 17.652L7.123 8.561L5.447 8.561L5.675 7.358L5.708 7.347L7.492 6.758L7.653 6.054C7.939 4.794 8.331 3.916 8.927 3.358C9.534 2.789 10.306 2.595 11.242 2.595ZM15.599 14.561L13.919 17.118L10.838 17.118L14.031 12.501L15.599 14.561Z'),
  hermes: F('M9.85 11.57L8.99 11.53L8.92 11.4L8.75 11.35L9.41 11.44L9.79 11.4L10.17 11.21L10.3 10.93L10.02 10.77L9.94 10.93L9.98 11.07L9.84 11.24L9.66 11.36L9.42 11.34L9.18 11.09L9.11 10.87L9.15 10.6L8.83 10.94L8.68 10.87L8.77 10.5L8.61 10.49L8.79 10.25L9.04 10.13L9.55 10.1L10.05 10.33L9.43 10.19L9.84 10.29L10.55 10.71L10.75 10.69L10.74 10.78L10.87 10.8L10.63 10.94L10.68 11L10.37 11.24L10.39 11.3L10.25 11.33L10.3 11.42L10.12 11.42L10.12 11.51L9.94 11.49L9.85 11.57ZM7.27 12.6L7.58 12.8L7.47 12.63L7.27 12.6ZM19.94 18.36L19.34 19.1L18.29 19.76L18.15 20.02L18.16 20.36L18.07 20.44L17.96 20.36L17.69 19.56L17.23 18.96L16.68 18.58L15.93 18.42L16.74 18.82L17.41 19.39L17.8 19.98L17.89 20.39L17.82 20.47L17.71 20.43L17.18 19.6L16.54 18.95L16.12 18.74L15.46 18.64L14.77 18.71L14.38 18.89L13.34 19.88L12.97 20.31L12.83 20.62L12.54 20.63L12.63 20.11L12.86 19.63L12.93 19.71L13.42 19.08L14.24 18.49L13.54 18.72L12.85 19.16L12.38 19.8L11.95 20.63L11.57 20.64L11.54 20.58L13.26 18.65L11.03 19.93L11.02 19.72L10.13 20.64L9.29 20.64L9.33 20.41L9.02 20.64L9.14 20.4L8.64 20.64L6.73 20.64L7.28 19.89L7.59 18.93L7.3 19.3L6.9 19.57L6.45 19.64L6.03 19.57L5.55 19.2L5.36 18.65L4.48 18.25L4.19 18.01L3.93 17.64L3.73 17.03L3.72 16.41L3.91 17.03L4.2 17.45L4.62 17.74L5.19 17.9L5.62 17.82L6.02 17.55L6.34 17.13L6.54 16.59L6.15 17.06L5.77 17.37L5.36 17.56L4.96 17.61L4.54 17.5L4.2 17.26L3.96 16.92L3.82 16.46L3.88 15.63L4.13 15.13L4.37 14.91L4.19 15.56L4.32 16.24L4.48 16.46L4.72 16.61L5.09 16.64L5.31 16.55L5.64 16.03L5.87 14.92L6.22 11.41L5.52 10.71L5.68 11.08L6.03 11.49L5.54 11.35L5.04 11.01L4.59 10.39L4.39 9.62L4.39 8.94L4.52 8.22L4.76 7.52L5.15 6.73L5.59 6.1L6.2 5.52L7 4.92L7.93 4.38L9.67 3.7L10.88 3.44L11.81 3.35L12.56 3.39L13.22 3.52L14.02 3.77L14.74 4.11L15.88 4.99L16.4 5.6L16.74 6.14L17.11 6.93L17.62 8.36L18.23 11.27L19.28 15.59L19.34 16.69L19.22 17.63L18.9 18.42L18.42 19.1L18.72 18.9L19.11 18.45L19.72 17.31L19.84 16.37L19.6 15.41L20.01 15.93L20.24 16.72L20.22 17.62L19.94 18.36ZM8.59 6.78L8.72 7.2L9.38 7.24L9.55 7.37L9.37 6.66L8.63 6.62L8.59 6.78ZM9.17 9.03L8.75 8.1L8.98 8.98L9.26 9.61L9.33 9.67L9.51 9.63ZM7.74 6.95L7.34 6.79L7.2 7.23L7.34 7.4L8.24 7.23L8.35 7.37L8.39 7.24L8.49 7.41L8.46 6.36L8.24 6.91L7.94 7.01L7.86 6.92L7.74 6.95ZM5.13 7.66L5.49 6.89L5.96 6.18L5.16 7L5.02 7.31L5.11 7.49L4.96 7.95L5.13 7.66ZM6.38 7.04L6.05 7.06L6.23 6.69L6.08 6.86L5.94 6.83L6.02 6.59L5.87 6.78L5.6 6.81L5.32 7.37L5.27 7.55L5.6 7.56L5.52 7.86L5.81 7.37L5.79 7.57L5.87 7.76L6.05 7.65L6.07 7.8ZM6.51 7.47L6.85 7.29L6.91 7.64L7.51 6.1L6.78 7.1L6.49 7.25L6.65 6.77L6.27 7.54L6.42 7.59L6.51 7.47ZM6.66 10.88L6.96 10.85L7.35 11.07L7.24 10.88L6.71 10.79L6.9 10.73L7.2 10.8L6.81 10.52L6.81 10.43L6.42 10.57L6.54 10.86L6.66 10.88ZM6.8 11.73L6.92 11.83L7.03 11.75L6.81 11.13L6.72 11.22L6.8 11.73ZM6.71 11.99L6.7 12.07L6.77 11.98ZM7.27 17.33L6.97 17.99L6.62 18.42L6.23 18.62L5.61 18.67L5.78 18.88L6.02 18.97L6.34 18.95L6.63 18.79L7.05 18.19L7.27 17.33ZM9.4 15.91L9.86 15.85L10.08 15.93L10.54 16.43L10.92 17.04L11.32 16.7L11.29 15.41L11.89 11.74L11.91 10.17L11.26 10.01L11.15 9.74L11.12 9.98L11.04 10.01L9.82 10.02L9.57 9.76L8.28 10.34L8.5 9.93L7.85 10.04L7.51 9.76L7.17 9.33L7.48 10.1L6.85 10.28L7.01 10.4L6.92 10.48L7.32 10.83L7.42 11.24L7.23 11.73L6.76 12.2L6.75 12.39L7.23 13.04L7.16 13.44L7.29 13.62L7.67 13.65L8.25 13.95L8.44 13.96L8.49 14.04L8.05 14.23L8.14 14.12L8.08 14.09L7.62 14.07L7.5 14.32L7.5 14.1L7.42 14.11L7.32 14.22L7.38 14.36L7.83 14.38L7.57 14.51L7.56 14.62L7.39 14.51L7.23 14.31L7.2 13.96L7.01 13.6L7.13 13.07L6.65 12.48L6.61 13.15L6.77 13.88L7.46 15.27L7.86 15.88L8.32 16.02ZM7.68 13.73L7.48 13.66L7.4 13.69L7.43 13.75L7.68 13.73ZM10.51 6.58L10.49 6.66L10.41 6.51L10.32 6.51L10.29 6.65L10.09 6.83L10.01 6.73L9.6 6.92L9.61 7.17L10.17 7.45L10.43 7.3L10.52 7.47L10.53 7.17L10.8 7.58L10.83 7.17L11.02 7.54L11.01 7.22L11.2 7.31L11.17 7.18L10.83 6.44L10.42 5.89L10.51 6.58ZM11.07 6.21L10.94 6.38L11.08 6.65L11.44 6.69L11.55 6.81L11.61 6.64L11.79 6.62L11.88 6.68L11.85 6.5L11.95 6.49L11.9 6.23L11.98 5.97L12.51 6.75L12.8 6.82L12.68 6.49L12.45 6.49L11.89 5.65L11.56 5.87L11.32 5.61L11.37 5.91L11.1 5.98L11.07 6.21ZM12.63 15.66L13.28 15.25L13.17 14.05L12.63 15.66ZM10.32 19.81L10.03 20.06L9.63 20.63ZM12.79 18.4L11.83 17.72L11.49 17.14L10.63 17.94L10.04 18.09L10.81 19.48L11.98 18.95L12.79 18.4ZM13.48 16.3L13.36 15.78L12.58 16.27L12.52 16.42L12.69 16.74L12.99 16.78L13.44 16.49L13.48 16.3ZM13.51 8.23L13.37 8.15L13.42 7.88L14.15 7.76L14.24 8.04L14.05 8.22L14.09 8.43L14.19 8.54L14.75 8.77L14.13 8.4L14.12 8.26L14.3 8.14L14.32 8.03L13.89 6.56L13.31 5.47L12.51 4.53L11.73 4L10.87 3.75L9.99 3.75L8.85 4.08L9.63 4.03L10.41 4.16L10.99 4.41L11.66 4.9L12.37 5.69L12.87 6.66L13.29 8.21L13.56 8.49L13.27 8.92L13.57 8.58L13.61 8.3ZM14.83 10.93L14.57 10.72L14.27 10.67L13.9 10.72L13.65 10.9L13.82 10.95L14.27 10.86L14.65 10.97L14.83 10.93ZM15.48 6.79L15.67 6.65L16.01 7.26L15.79 6.65L16.22 7.2L16.07 6.69L16.1 6.61L16.18 6.65L16.18 6.5L16.41 6.81L16.25 6.35L16.36 6.43L16.35 5.78L16.68 6.16L16.06 5.37L16.16 5.68L16.13 6.1L15.94 6.02L15.6 5.67L15.78 6.08L15.55 6.06L15.41 6.17L15.46 6.35L15.27 6.56L14.68 6.44L14.47 6.15L14.47 6.56L14.02 6.55L14.08 6.7L14.41 6.75L14.76 7.07L14.78 6.9L15.03 7.25L14.74 6.57L15.48 6.79ZM17.01 13.77L16.63 12.05L15.41 8.19L16.7 12.5L16.98 13.77ZM18.51 15.4L18.34 14.72L18.04 14.02L18.16 14.84L18.16 15.57L18.03 16.54L17.73 17.48L18.09 17.12L18.38 16.58L18.52 16.01L18.51 15.4Z'),
  antigravity: F('M19.051 19.9C20.042 20.643 21.529 20.148 20.166 18.785C16.078 14.82 16.945 3.918 11.865 3.918C6.786 3.918 7.653 14.82 3.564 18.785C2.078 20.272 3.688 20.643 4.679 19.9C8.52 17.298 8.272 12.714 11.865 12.714C15.458 12.714 15.21 17.298 19.051 19.9Z'),
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

// The agent chip cycles through these in key order.
const AGENTS = {
  claude: ['claude', 'Claude Code'], codex: ['openai', 'Codex'], cursor: ['cursor', 'Cursor'], grok: ['grok', 'Grok Build'],
  opencode: ['opencode', 'OpenCode'], pi: ['pi', 'Pi'], omp: ['omp', 'omp'], fx: ['fx', 'fx'], hermes: ['hermes', 'Hermes Agent'],
  antigravity: ['antigravity', 'Antigravity'],
};

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
    case 'agent': {
      const order = Object.keys(AGENTS);
      agent = order[(order.indexOf(agent) + 1) % order.length];
      break;
    }
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
