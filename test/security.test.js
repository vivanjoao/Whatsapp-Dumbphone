const { test } = require('node:test');
const assert = require('node:assert/strict');
const request = require('supertest');
const { createApp } = require('../src/app');
const { loadConfig } = require('../src/config');
const { csrf } = require('../src/security');
const config = { password: 'unique-test-password', secret: 'a'.repeat(64), token: 'b'.repeat(32), legacyTokenUrls: true };
const id = '123@c.us';
const field = html => html.match(/name="csrf" value="([^"]+)"/)[1];
function fixture(overrides = {}, settings = {}) {
    const sends = [];
    const chat = { id: { _serialized: id }, name: '<script>alert(1)</script>', unreadCount: 1, isGroup: true, sendSeen: async () => {}, fetchMessages: async () => [{ body: '<img src=x onerror=alert(1)>', fromMe: false, timestamp: 1, getContact: async () => ({ name: '<b>sender</b>' }) }] };
    const client = { getChats: async () => [chat], getChatById: async () => chat, sendMessage: async (...args) => sends.push(args), ...overrides };
    return { app: createApp({ client, config: { ...config, ...settings }, isReady: () => true }), sends, chat };
}
test('configuration rejects missing and default credentials', () => {
    for (const env of [{}, { PASSWORD: 'admin123', SESSION_SECRET: config.secret }, { PASSWORD: config.password, SESSION_SECRET: 'short' }, { PASSWORD: config.password, SESSION_SECRET: config.secret, ACCESS_TOKEN: 'bad' }]) assert.throws(() => loadConfig(env));
    assert.equal(loadConfig({ PASSWORD: config.password, SESSION_SECRET: config.secret }).token, '');
});
test('unauthenticated and malformed tokens cannot read or send', async () => {
    const { app, sends } = fixture();
    for (const query of ['', '?token=', '?token=wrong', '?token[]=x']) assert.equal((await request(app).get('/' + query)).status, 401);
    assert.equal((await request(app).post('/send').send({ to: id, msg: 'x' })).status, 401);
    assert.equal(sends.length, 0);
    assert.equal((await request(fixture({}, { token: '' }).app).get('/?token=' + config.token)).status, 401);
});
test('cookie login protects against CSRF, regenerates session and authenticates', async () => {
    const { app } = fixture();
    const agent = request.agent(app);
    const page = await agent.get('/');
    const oldCookie = page.headers['set-cookie'][0].split(';')[0];
    assert.equal((await agent.post('/login').type('form').send({ pw: config.password })).status, 403);
    assert.equal((await agent.post('/login').type('form').send({ pw: 'wrong', csrf: field(page.text) })).status, 401);
    const login = await agent.post('/login').type('form').send({ pw: config.password, csrf: field(page.text) });
    assert.equal(login.status, 302);
    assert.notEqual(login.headers['set-cookie'][0].split(';')[0], oldCookie);
    assert.equal((await agent.get('/')).status, 200);
});
test('HTML content and group senders are escaped; security headers applied', async () => {
    const { app } = fixture();
    for (const path of ['/', '/chat/' + id]) {
        const res = await request(app).get(path).query({ token: config.token });
        assert.equal(res.status, 200);
        assert.doesNotMatch(res.text, /<script>|<img|<b>sender/);
        assert.match(res.text, /&lt;/);
        assert.equal(res.headers['referrer-policy'], 'no-referrer');
        assert.equal(res.headers['cache-control'], 'no-store');
    }
});
test('cookieless bookmark can submit signed form exactly once per request', async () => {
    const { app, sends } = fixture();
    const page = await request(app).get('/chat/' + id).query({ token: config.token });
    const data = { token: config.token, csrf: field(page.text), to: id, msg: 'hello' };
    assert.equal((await request(app).post('/send').type('form').send({ ...data, csrf: 'invalid' })).status, 403);
    for (const invalid of [{ to: '//evil.example' }, { msg: '' }, { msg: 'a'.repeat(4097) }]) assert.equal((await request(app).post('/send').type('form').send({ ...data, ...invalid })).status, 400);
    assert.equal((await request(app).post('/send').type('form').send(`token=${config.token}&csrf=${data.csrf}&to=123%40c.us&to=456%40c.us&msg=hello`)).status, 400);
    const result = await request(app).post('/send').type('form').send(data);
    assert.equal(result.status, 302);
    assert.equal(result.headers.location, '/chat/123%40c.us?token=' + config.token);
    assert.deepEqual(sends, [[id, 'hello']]);
});
test('CSRF expires and is bound to context', () => {
    const value = csrf(config.secret, 'first');
    assert.equal(csrf(config.secret, 'second', value), false);
    assert.equal(csrf(config.secret, 'first', value), true);
    const now = Date.now;
    try { Date.now = () => now() + 3600001; assert.equal(csrf(config.secret, 'first', value), false); } finally { Date.now = now; }
});
test('provider errors are controlled and never expose internal data or retry sends', async () => {
    let calls = 0;
    const { app, chat } = fixture({ sendMessage: async () => { calls++; throw Error('private-secret'); } });
    const page = await request(app).get('/chat/' + id).query({ token: config.token });
    const res = await request(app).post('/send').type('form').send({ token: config.token, csrf: field(page.text), to: id, msg: 'hello' });
    assert.equal(res.status, 503);
    assert.equal(calls, 1);
    assert.doesNotMatch(res.text, /private-secret/);
    chat.fetchMessages = async () => { throw Error('private-secret'); };
    const history = await request(app).get('/chat/' + id).query({ token: config.token });
    assert.equal(history.status, 503);
    assert.doesNotMatch(history.text, /No messages|private-secret/);
});
test('disconnected gateway rejects requests without touching provider', async () => {
    const app = createApp({ client: {}, config, isReady: () => false });
    assert.equal((await request(app).get('/').query({ token: config.token })).status, 503);
});
test('cookie mode removes token from links and invalidates old session cookies', async () => {
    const { app } = fixture({}, { legacyTokenUrls: false });
    const agent = request.agent(app);
    const exchange = await agent.get('/').query({ token: config.token });
    assert.equal(exchange.status, 302);
    assert.equal(exchange.headers.location, '/');
    const page = await agent.get('/');
    assert.equal(page.status, 200);
    assert.doesNotMatch(page.text, /token=/);
    // A changed credential invalidates the old session signature.
    const changed = fixture({}, { token: 'c'.repeat(32), legacyTokenUrls: false }).app;
    assert.equal((await request(changed).get('/').set('Cookie', exchange.headers['set-cookie'][0].split(';')[0])).status, 401);
});
test('login attempts are rate limited', async () => {
    const { app } = fixture();
    const agent = request.agent(app);
    const page = await agent.get('/');
    for (let i = 0; i < 5; i++) assert.equal((await agent.post('/login').type('form').send({ pw: 'wrong', csrf: field(page.text) })).status, 401);
    assert.equal((await agent.post('/login').type('form').send({ pw: 'wrong', csrf: field(page.text) })).status, 429);
});
test('encrypted file sessions survive app restart, expire and are revoked on credential rotation', async () => {
    const fs = require('node:fs');
    const os = require('node:os');
    const path = require('node:path');
    const session = require('express-session');
    const FileStore = require('session-file-store')(session);
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'gateway-test-'));
    const options = { path: dir, secret: config.secret, reapInterval: -1, retries: 0, logFn: () => {} };
    const settings = { ...config, legacyTokenUrls: false };
    const build = (store, cfg = settings) => createApp({ client: { getChats: async () => [] }, config: cfg, isReady: () => true, store });
    try {
        const first = build(new FileStore(options));
        const exchange = await request(first).get('/').query({ token: config.token });
        const cookie = exchange.headers['set-cookie'][0].split(';')[0];
        const files = fs.readdirSync(dir);
        assert.equal(files.length, 1);
        assert.doesNotMatch(fs.readFileSync(path.join(dir, files[0]), 'utf8'), /tokenAuth|bbbbbbbb/);
        const secondStore = new FileStore(options);
        assert.equal((await request(build(secondStore)).get('/').set('Cookie', cookie)).status, 200);
        assert.equal((await request(build(secondStore, { ...settings, token: '' })).get('/').set('Cookie', cookie)).status, 401);
        const sessionId = files[0].replace(/\.json$/, '');
        const expired = { cookie: { originalMaxAge: 1, expires: new Date(0) }, tokenAuth: config.token, __lastAccess: 0 };
        await new Promise((resolve, reject) => secondStore.set(sessionId, expired, err => err ? reject(err) : resolve()));
        assert.equal((await request(build(secondStore)).get('/').set('Cookie', cookie)).status, 401);
    } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});
