require('dotenv').config();
const { loadConfig } = require('./src/config');
const { createApp } = require('./src/app');
const { createWhatsApp } = require('./src/whatsapp');
async function main() {
    const config = loadConfig();
    const { client, isReady } = createWhatsApp();
    const session = require('express-session');
    const FileStore = require('session-file-store')(session);
    const path = require('node:path');
    const fs = require('node:fs');
    const sessionPath = path.join(process.env.AUTH_DIR || '.wwebjs_auth', 'http-sessions');
    fs.mkdirSync(sessionPath, { recursive: true, mode: 0o700 });
    const store = new FileStore({ path: sessionPath, ttl: 30 * 24 * 60 * 60, secret: config.secret, logFn: () => {} });
    const app = createApp({ client, config, isReady, store });
    const server = app.listen(3000, '0.0.0.0');
    let stopping = false;
    const stop = async () => {
        if (stopping) return;
        stopping = true;
        const timer = setTimeout(() => process.exit(1), 10000);
        timer.unref();
        await Promise.all([new Promise(resolve => server.close(resolve)), client.destroy()]);
        clearTimeout(timer);
        clearInterval(store.options.reapIntervalObject);
    };
    process.once('SIGTERM', () => stop().catch(() => { process.exitCode = 1; }));
    process.once('SIGINT', () => stop().catch(() => { process.exitCode = 1; }));
    try { await client.initialize(); } catch (_) {
        console.error('WhatsApp startup failed. Check the profile and configuration; locks were preserved.');
        await stop();
        process.exitCode = 1;
    }
}
if (require.main === module) main().catch(() => { console.error('Startup failed. Check required configuration.'); process.exitCode = 1; });
module.exports = { main };
