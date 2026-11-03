const express = require('express');
const session = require('express-session');
const crypto = require('node:crypto');
const createRoutes = require('./routes');
function createApp({ client, config, isReady = () => false, store }) {
    const app = express();
    app.disable('x-powered-by');
    app.set('trust proxy', config.trustProxy || false);
    app.use((req, res, next) => {
        res.set({ 'Cache-Control': 'no-store', 'Referrer-Policy': 'no-referrer', 'X-Content-Type-Options': 'nosniff', 'X-Frame-Options': 'DENY', 'Content-Security-Policy': "default-src 'none'; style-src 'unsafe-inline'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'" });
        next();
    });
    app.use(express.urlencoded({ extended: false, limit: '16kb', parameterLimit: 10 }));
    // Rotating any credential invalidates old cookie sessions as well.
    const secret = crypto.createHmac('sha256', config.secret).update(JSON.stringify([config.password, config.token])).digest('hex');
    app.use(session({ secret, store, resave: false, saveUninitialized: false, cookie: { httpOnly: true, sameSite: 'lax', secure: config.secureCookies, maxAge: 30 * 24 * 60 * 60 * 1000 } }));
    app.use(createRoutes({ client, config, isReady }));
    app.use((err, req, res, next) => {
        if (res.headersSent) return next(err);
        res.status(err.type === 'entity.too.large' ? 413 : err.status === 400 ? 400 : 500).send('Request could not be completed.');
    });
    return app;
}
module.exports = { createApp };
