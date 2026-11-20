const KOI = [
    '......wwo...',
    't..oowwoooo.',
    'ttoowwoooeoo',
    't..oowwoooo.',
    '......oo....',
];
const BUBBLE = ['.b.', 'b.b', '.b.'];

const MOODS = [
    { words: /\b(bug|fix|error|fail|crash)/i, fish: '#d64545', speed: 95, water: ['#cfe3ea', '#9cc3d1'] },
    { words: /\btest/i, fish: '#e3a62f', speed: 55, water: ['#d6eef0', '#a9d6dc'] },
];
const CALM = { fish: '#ee8a3c', speed: 30, water: ['#dff3f3', '#b3dfe4'] };

function pond(text) {
    const mood = MOODS.find(m => m.words.test(text)) ?? CALM;
    const words = text.trim().split(/\s+/).filter(Boolean).length;
    return {
        name: 'Koi pond',
        appearance: 'light',
        backdrop: mood.water,
        colors: { 'page': mood.water[1], 'canvas': mood.water[1] },
        scene: {
            sprites: [
                { art: KOI, colors: { o: mood.fish, w: '#fbf7f0', e: '#1d2b33', t: mood.fish + 'aa' }, pixel: 5, count: Math.min(10, 2 + Math.floor(words / 3)), speed: mood.speed },
                { art: BUBBLE, colors: { b: '#ffffffaa' }, pixel: 3, count: 8, speed: 8 },
            ],
        },
    };
}

export function register(on) {
    on('start', $ => $.theme.set(pond('')));
    on('prompt', ($, e) => $.theme.set(pond(e.text)));
}
