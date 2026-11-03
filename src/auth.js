const rateLimit = require('express-rate-limit');
const { equal, csrf, escapeHtml } = require('./security');
module.exports = function createAuth(config) {
    const loginLimiter = rateLimit({ windowMs: 15 * 60 * 1000, max: 5, message: 'Too many login attempts. Please try again later.' });
    const tokenParam = req => req.legacyToken ? `?token=${encodeURIComponent(req.legacyToken)}` : '';
    const context = req => req.legacyToken ? `token:${req.legacyToken}` : `session:${req.sessionID}`;
    const csrfField = req => {
        if (!req.legacyToken) req.session.formIssued = true;
        return `<input type="hidden" name="csrf" value="${escapeHtml(csrf(config.secret, context(req)))}">`;
    };
    const checkCsrf = (req, res, next) => typeof req.body.csrf === 'string' && csrf(config.secret, context(req), req.body.csrf) ? next() : res.status(403).send('Form expired or invalid. Reload the page.');
    const checkAuth = (req, res, next) => {
        const token = req.query.token || req.body.token;
        if (config.token && equal(token, config.token)) {
            if (config.legacyTokenUrls) {
                req.legacyToken = token;
                return next();
            }
            // Exchange the bookmark credential for a fresh cookie session.
            return req.session.regenerate(err => {
                if (err) return next(err);
                req.session.tokenAuth = config.token;
                req.session.save(err => err ? next(err) : req.method === 'GET' ? res.redirect(req.path) : next());
            });
        }
        if (req.session.loggedIn === true || (config.token && equal(req.session.tokenAuth, config.token))) return next();
        res.status(401).send(`<html><head><meta name="viewport" content="width=240, initial-scale=1.0"></head><body><h3>Login</h3><form action="/login" method="post">${csrfField(req)}<input type="password" name="pw"><input type="submit" value="login"></form></body></html>`);
    };
    return { checkAuth, loginLimiter, tokenParam, csrfField, checkCsrf };
};
