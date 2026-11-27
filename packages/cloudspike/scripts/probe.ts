const [url, token] = process.argv.slice(2);
const ws = new WebSocket(url);
const timer = setTimeout(() => { console.log("TIMEOUT"); process.exit(1); }, 10000);
ws.onopen = () => ws.send(JSON.stringify({ type: "hello", id: "1", clientId: "spike-probe", protocolVersion: 3, ownerToken: token }));
ws.onmessage = (e) => { const m = JSON.parse(String(e.data)); console.log(m.type, JSON.stringify(m).slice(0, 300)); if (m.type === "hello.ok" || m.type === "error") { clearTimeout(timer); ws.close(); process.exit(m.type === "hello.ok" ? 0 : 2); } };
ws.onerror = (e) => { console.log("ERROR", e.message ?? e); process.exit(3); };
