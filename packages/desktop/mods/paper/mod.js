const HUE = 38;

const hsl = (s, l) => {
    const k = n => (n + HUE / 30) % 12;
    const a = (s / 100) * Math.min(l / 100, 1 - l / 100);
    const f = n => l / 100 - a * Math.max(-1, Math.min(k(n) - 3, Math.min(9 - k(n), 1)));
    return '#' + [0, 8, 4].map(n => Math.round(f(n) * 255).toString(16).padStart(2, '0')).join('');
};

export function register(on) {
    on('start', $ => {
        $.theme.set({
            name: 'Paper',
            appearance: 'light',
            accent: '#2f5d8c',
            backdrop: [hsl(45, 93), hsl(40, 86)],
            bubble: { radius: 4 },
            colors: {
                'window': hsl(40, 92),
                'window.solid': hsl(40, 92),
                'sidebar': hsl(38, 88),
                'page': hsl(40, 86),
                'canvas': hsl(40, 86),
                'chat.bubble': hsl(50, 97),
                'text': hsl(25, 14),
                'text.body': hsl(22, 22),
                'text.2': hsl(18, 32),
            },
        });
    });
}
