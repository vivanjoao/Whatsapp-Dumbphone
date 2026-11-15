use crate::domain::{Chat, Conversation};
pub fn escape(value: &str) -> String {
    value
        .chars()
        .fold(String::with_capacity(value.len()), |mut out, c| {
            out.push_str(match c {
                '&' => "&amp;",
                '<' => "&lt;",
                '>' => "&gt;",
                '"' => "&quot;",
                '\'' => "&#39;",
                _ => {
                    out.push(c);
                    return out;
                }
            });
            out
        })
}
pub fn url(value: &str) -> String {
    urlencoding::encode(value).into_owned()
}
fn page(content: String) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=240, initial-scale=1\"><title>WhatsApp</title></head><body style=\"font-family:sans-serif;margin:0;padding:4px;background:#eee;color:#000\">{content}</body></html>"
    )
}
fn hidden(name: &str, value: &str) -> String {
    format!(
        "<input type=\"hidden\" name=\"{}\" value=\"{}\">",
        escape(name),
        escape(value)
    )
}
fn token_suffix(token: Option<&str>) -> String {
    token
        .map(|t| format!("?token={}", url(t)))
        .unwrap_or_default()
}
pub fn login(csrf: &str) -> String {
    page(format!(
        "<h3>Login</h3><form action=\"/login\" method=\"post\">{}<input type=\"password\" name=\"pw\" maxlength=\"1024\"><input type=\"submit\" value=\"Login\"></form>",
        hidden("csrf", csrf)
    ))
}
pub fn chats(chats: &[Chat], token: Option<&str>) -> String {
    let mut body =
        String::from("<h3 style=\"background:#25d366\">WhatsApp</h3><table width=\"100%\">");
    for chat in chats.iter().take(crate::domain::CHAT_LIMIT) {
        body.push_str(&format!(
            "<tr><td><a href=\"/chat/{}{}\">{}</a> {}</td></tr>",
            url(&chat.id),
            token_suffix(token),
            escape(&chat.name),
            if chat.unread > 0 {
                format!("({})", chat.unread)
            } else {
                String::new()
            }
        ));
    }
    if chats.is_empty() {
        body.push_str("<tr><td>No conversations synced yet. Refresh after pairing.</td></tr>");
    }
    body.push_str("</table>");
    page(body)
}
pub fn conversation(
    conversation: &Conversation,
    csrf: &str,
    token: Option<&str>,
    read_failed: bool,
) -> String {
    let mut body = format!(
        "<a href=\"/{}\">[Back]</a><h3>{}</h3>",
        token_suffix(token),
        escape(&conversation.chat.name)
    );
    if read_failed {
        body.push_str("<p>Read status could not be updated.</p>");
    }
    if conversation.messages.is_empty() {
        body.push_str("<p>No messages synced yet.</p>");
    }
    for msg in &conversation.messages {
        let bg = if msg.from_me { "#dcf8c6" } else { "#fff" };
        let sender = if msg.from_me { "You" } else { &msg.sender_name };
        let media = msg
            .media
            .as_ref()
            .map(|kind| format!("[{}] ", escape(kind)))
            .unwrap_or_default();
        body.push_str(&format!("<div style=\"background:{bg};border:1px solid #ccc;padding:4px;margin-bottom:4px;word-wrap:break-word\"><b>{}</b><br>{}{}<br><small>{} UTC</small></div>", escape(sender), media, escape(&msg.text), escape(&msg.time)));
    }
    body.push_str(&format!("<form action=\"/send\" method=\"post\">{}{}{}<input type=\"text\" name=\"msg\" maxlength=\"4096\" style=\"width:140px\"><input type=\"submit\" value=\"OK\"></form>", hidden("to", &conversation.chat.id), hidden("csrf", csrf), token.map(|t| hidden("token",t)).unwrap_or_default()));
    page(body)
}
