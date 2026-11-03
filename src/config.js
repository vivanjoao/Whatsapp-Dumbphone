function loadConfig(env = process.env) {
    if (typeof env.PASSWORD !== 'string' || env.PASSWORD.startsWith('REPLACE_') || env.PASSWORD.trim().length < 12 || ['admin123', 'secretpassword'].includes(env.PASSWORD)) throw new Error('PASSWORD must be a unique password of at least 12 characters.');
    if (typeof env.SESSION_SECRET !== 'string' || env.SESSION_SECRET.startsWith('REPLACE_') || env.SESSION_SECRET.length < 32 || env.SESSION_SECRET === 'fallback-secret-key') throw new Error('SESSION_SECRET must contain at least 32 random characters.');
    if (env.ACCESS_TOKEN && !/^[a-f0-9]{32,128}$/i.test(env.ACCESS_TOKEN)) throw new Error('ACCESS_TOKEN must contain 32–128 hexadecimal characters, or be empty.');
    return { password: env.PASSWORD, secret: env.SESSION_SECRET, token: env.ACCESS_TOKEN || '', legacyTokenUrls: env.LEGACY_TOKEN_URLS !== 'false', secureCookies: env.COOKIE_SECURE === 'true', trustProxy: env.TRUST_PROXY ? env.TRUST_PROXY.split(',').map(value => value.trim()) : false };
}
module.exports = { loadConfig };
