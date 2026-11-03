function createWhatsApp() {
    const { Client, LocalAuth } = require('whatsapp-web.js');
    const qrcode = require('qrcode-terminal');
    const client = new Client({
        authStrategy: new LocalAuth({ dataPath: process.env.AUTH_DIR || '.wwebjs_auth' }),
        puppeteer: {
            executablePath: process.env.PUPPETEER_EXECUTABLE_PATH || '/usr/bin/chromium-browser',
            args: ['--no-sandbox', '--disable-setuid-sandbox', '--disable-dev-shm-usage', '--disable-gpu']
        }
    });
    // Chromium owns its profile locks. Never remove them automatically.
    let ready = false;
    client.on('qr', qr => { ready = false; qrcode.generate(qr, { small: true }); });
    client.on('ready', () => { ready = true; console.log('WhatsApp Gateway is ready!'); });
    client.on('disconnected', () => { ready = false; });
    client.on('auth_failure', () => { ready = false; });
    client.on('change_state', state => { if (state !== 'CONNECTED') ready = false; });
    return { client, isReady: () => ready };
}
module.exports = { createWhatsApp };
