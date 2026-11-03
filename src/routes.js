const express = require('express');
const { escapeHtml, equal } = require('./security');
module.exports = function createRoutes({ client, config, isReady }) {
const router = express.Router();
const { checkAuth, loginLimiter, tokenParam, csrfField, checkCsrf } = require('./auth')(config);
const validId = id => typeof id === 'string' && /^(?:[0-9]{1,20}(?:-[0-9]{1,20})?@(c\.us|g\.us|lid))$/.test(id);
const ready = (req, res, next) => isReady() ? next() : res.status(503).send('WhatsApp is not connected. Please try again later.');

router.post('/login', loginLimiter, checkCsrf, (req, res, next) => {
    if (!equal(req.body.pw, config.password)) return res.status(401).send('Invalid password.');
    req.session.regenerate(err => {
        if (err) return next(err);
        req.session.loggedIn = true;
        req.session.save(err => err ? next(err) : res.redirect('/'));
    });
});

router.get('/', checkAuth, ready, async (req, res) => {
    try {
        const chats = await client.getChats();
        const tp = tokenParam(req);

        let html = `<html><head><meta name="viewport" content="width=240, initial-scale=1.0"></head>
        <body style="background:#000;color:#fff;font-family:sans-serif;margin:0;padding:0;">
        <div style="background:#25D366;color:#000;padding:5px;"><b>WhatsApp</b></div>
        <table width="100%" border="0" cellspacing="0" cellpadding="8">`;

        chats.slice(0, 20).forEach(chat => {
            const hasUnread = chat.unreadCount > 0;
            const titleColor = hasUnread ? '#25D366' : '#ffffff';
            const unreadText = hasUnread ? ` <b>(${escapeHtml(chat.unreadCount)})</b>` : '';
            html += `<tr><td style="border-bottom:1px solid #333;">
                <a href="/chat/${escapeHtml(encodeURIComponent(chat.id._serialized))}${tp}" style="color:${titleColor};text-decoration:none;">
                    ${escapeHtml(chat.name || chat.id.user)}${unreadText}
                </a>
            </td></tr>`;
        });

        html += `</table></body></html>`;
        res.send(html);
    } catch (e) { res.status(503).send('Unable to load conversations.'); }
});

router.get('/chat/:id', checkAuth, ready, async (req, res) => {
    try {
        if (!validId(req.params.id)) return res.status(400).send('Invalid conversation.');
        const chat = await client.getChatById(req.params.id);
        if (chat.unreadCount > 0) await chat.sendSeen();
        const tp = tokenParam(req);

        const msgs = await chat.fetchMessages({ limit: 10 });

        let html = `<html><head><meta name="viewport" content="width=240, initial-scale=1.0"></head>
        <body style="background:#eee;color:#000;font-family:sans-serif;margin:0;padding:0;">
        <div style="background:#25D366;color:#000;padding:5px;">
            <a href="/${tp}" style="color:#000;text-decoration:none;"><b>[&lt; Back]</b></a> ${escapeHtml(chat.name)}
        </div>
        <div style="padding:4px;">`;

        if (msgs.length === 0) html += `<i>No messages.</i><br><br>`;

        for (const m of msgs) {
            const align = m.fromMe ? 'right' : 'left';
            const bg = m.fromMe ? '#dcf8c6' : '#ffffff';
            const date = new Date(m.timestamp * 1000);
            const timeStr = date.getHours().toString().padStart(2, '0') + ':' + date.getMinutes().toString().padStart(2, '0');

            let senderHtml = '';
            if (!m.fromMe && chat.isGroup) {
                try {
                    const contact = await m.getContact();
                    const name = contact.name || contact.pushname || contact.number;
                    senderHtml = `<div style="font-size:10px;color:#128C7E;font-weight:bold;margin-bottom:2px;">${escapeHtml(name)}</div>`;
                } catch (_) {
                    senderHtml = `<div style="font-size:10px;color:#128C7E;font-weight:bold;margin-bottom:2px;">Contact</div>`;
                }
            }

            let mediaHtml = '';
            if (m.hasMedia) {
                const types = { image: 'Image', sticker: 'Sticker', video: 'Video', audio: 'Audio', ptt: 'Audio', document: 'Document' };
                mediaHtml = `<div style="font-size:11px;color:#555;font-style:italic;margin-bottom:2px;">[${types[m.type] || 'Media'}]</div>`;
            }

            html += `<div style="text-align:${align};margin-bottom:5px;">
                <div style="background:${bg};border:1px solid #ccc;padding:4px;display:inline-block;text-align:left;max-width:90%;">
                    ${senderHtml}${mediaHtml}
                    <div style="font-size:12px;">${escapeHtml(m.body || '')}</div>
                    <div style="font-size:9px;color:#888;text-align:right;margin-top:3px;">${timeStr}</div>
                </div>
            </div>`;
        }

        const t = req.legacyToken;
        const tokenField = t ? `<input type="hidden" name="token" value="${escapeHtml(t)}">` : '';
        html += `</div><hr>
        <form action="/send" method="post" style="padding:4px;margin:0;">
            <input type="hidden" name="to" value="${escapeHtml(req.params.id)}">
            ${tokenField}${csrfField(req)}
            <input type="text" name="msg" style="width:140px;">
            <input type="submit" value="OK">
        </form><br><br></body></html>`;

        res.send(html);
    } catch (e) { res.status(503).send('Unable to load messages.'); }
});

router.post('/send', checkAuth, checkCsrf, ready, async (req, res) => {
    if (!validId(req.body.to) || typeof req.body.msg !== 'string' || !req.body.msg.trim() || req.body.msg.length > 4096) return res.status(400).send('Invalid recipient or message (maximum 4096 characters).');
    try {
        await client.sendMessage(req.body.to, req.body.msg);
        res.redirect('/chat/' + encodeURIComponent(req.body.to) + tokenParam(req));
    } catch (_) {
        res.status(503).send('Send could not be confirmed. Check the conversation before trying again.');
    }
});
return router;
};
