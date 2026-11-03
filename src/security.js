const crypto = require('node:crypto');
const escapeHtml = value => String(value ?? '').replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);
const equal = (a, b) => typeof a === 'string' && typeof b === 'string' && a.length > 0 && Buffer.byteLength(a) === Buffer.byteLength(b) && crypto.timingSafeEqual(Buffer.from(a), Buffer.from(b));
function csrf(secret, context, value) {
    const sign = data => crypto.createHmac('sha256', secret).update(JSON.stringify([context, data])).digest('hex');
    if (value === undefined) {
        const data = `${Date.now()}.${crypto.randomBytes(16).toString('hex')}`;
        return `${data}.${sign(data)}`;
    }
    if (typeof value !== 'string' || !/^\d{13}\.[a-f0-9]{32}\.[a-f0-9]{64}$/.test(value)) return false;
    const [time, nonce, signature] = value.split('.');
    const age = Date.now() - Number(time);
    return age >= 0 && age <= 3600000 && equal(signature, sign(`${time}.${nonce}`));
}
module.exports = { escapeHtml, equal, csrf };
