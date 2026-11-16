use crate::domain::{CHAT_LIMIT, Chat, Conversation};
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    #[default]
    Auto,
    Compact,
}
#[derive(Clone, Copy)]
pub struct Context<'a> {
    pub token: Option<&'a str>,
    pub layout: Layout,
}
impl Context<'_> {
    pub fn href(&self, path: &str) -> String {
        let mut query = Vec::new();
        if let Some(token) = self.token {
            query.push(format!("token={}", url(token)));
        }
        if self.layout == Layout::Compact {
            query.push("view=compact".to_owned());
        }
        if query.is_empty() {
            path.to_owned()
        } else {
            format!("{path}?{}", query.join("&"))
        }
    }
    fn fields(&self) -> String {
        let mut fields = self.token.map(|t| hidden("token", t)).unwrap_or_default();
        if self.layout == Layout::Compact {
            fields.push_str(&hidden("view", "compact"));
        }
        fields
    }
}
pub fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}
pub fn url(value: &str) -> String {
    urlencoding::encode(value).into_owned()
}
fn hidden(name: &str, value: &str) -> String {
    format!(
        "<input type=\"hidden\" name=\"{}\" value=\"{}\">",
        escape(name),
        escape(value)
    )
}
fn icon(name: &str) -> String {
    let path = match name {
        "refresh" => {
            "M20 7v5h-5M4 17v-5h5M6.1 8a6.5 6.5 0 0 1 10.7-2L20 9M4 15l3.2 3a6.5 6.5 0 0 0 10.7-2"
        }
        "back" => "m14 6-6 6 6 6",
        "send" => "m21 3-7 18-4-7-7-4 18-7ZM10 14 21 3",
        _ => {
            "M5 4h14a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2h-8l-5 3v-3H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2ZM7 9h10M7 13h6"
        }
    };
    format!(
        "<svg class=\"icon\" width=\"17\" height=\"17\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.6\" stroke-linecap=\"round\" stroke-linejoin=\"round\" aria-hidden=\"true\"><path d=\"{path}\"/></svg>"
    )
}
fn avatar(name: &str, tone: usize) -> String {
    let initials: String = name
        .split_whitespace()
        .take(2)
        .filter_map(|word| word.chars().next())
        .flat_map(char::to_uppercase)
        .take(2)
        .collect();
    format!(
        "<span class=\"avatar avatar--{}\" aria-hidden=\"true\">{}</span>",
        tone % 4,
        escape(&initials)
    )
}
fn page(content: String, kind: &str, title: &str, context: Context<'_>) -> String {
    let css = match context.layout {
        Layout::Auto => include_str!("styles/responsive.css"),
        Layout::Compact => "",
    };
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>{} · WhatsApp Gateway</title><style>{}{css}</style></head><body class=\"page--{kind}\"><header class=\"app-brand\"><span class=\"brand-mark\">{}</span><span class=\"brand-name\">WhatsApp Gateway</span><span class=\"brand-note\">A little closer.</span></header><main class=\"app-shell\">{content}</main></body></html>",
        escape(title),
        include_str!("styles/base.css"),
        icon("chat")
    )
}
fn footer(path: &str, context: Context<'_>, hint: &str) -> String {
    let (layout, label) = match context.layout {
        Layout::Auto => (Layout::Compact, "Compact view"),
        Layout::Compact => (Layout::Auto, "Responsive view"),
    };
    let target = Context { layout, ..context }.href(path);
    format!(
        "<footer class=\"page-footer\"><span>{}</span><a href=\"{}\">{label}</a></footer>",
        escape(hint),
        escape(&target)
    )
}
fn sidebar(chats: &[Chat], current: Option<&str>, context: Context<'_>, failed: bool) -> String {
    let count = if failed {
        String::new()
    } else {
        let count = chats.len().min(CHAT_LIMIT);
        format!(
            "<span class=\"chat-count\" aria-label=\"{count} recent conversations\">{count}</span>"
        )
    };
    let mut body = format!(
        "<nav class=\"sidebar\" aria-label=\"Conversations\"><header class=\"sidebar-header\"><div class=\"sidebar-title\"><h2>Conversations</h2>{count}</div><p class=\"sidebar-intro\">Your recent chats, all in one place.</p><div class=\"sidebar-tools\"><a class=\"text-link\" href=\"{}\">{}Refresh chats</a></div></header><ul class=\"chat-list\">",
        escape(&context.href("/")),
        icon("refresh")
    );
    for (i, chat) in chats.iter().take(CHAT_LIMIT).enumerate() {
        let active = if current == Some(chat.id.as_str()) {
            " aria-current=\"page\""
        } else {
            ""
        };
        let subtitle = if chat.unread != 0 {
            "Unread messages"
        } else if chat.id.ends_with("@g.us") {
            "Group conversation"
        } else {
            "Open conversation"
        };
        let badge = if chat.unread > 0 {
            format!(
                "<span class=\"unread-badge\" aria-label=\"{} unread messages\">{}</span>",
                chat.unread, chat.unread
            )
        } else {
            String::new()
        };
        body.push_str(&format!("<li><a class=\"chat-link\" href=\"{}\"{active}>{}<span class=\"chat-copy\"><span class=\"chat-name\">{}</span><span class=\"chat-subtitle\">{subtitle}</span></span>{badge}</a></li>", escape(&context.href(&format!("/chat/{}", url(&chat.id)))), avatar(&chat.name,i), escape(&chat.name)));
    }
    if chats.is_empty() {
        let text = if failed {
            "The chat list could not be loaded. Use Refresh chats to try again."
        } else {
            "No conversations synced yet. Refresh after pairing."
        };
        body.push_str(&format!("<li class=\"empty-list\">{text}</li>"));
    }
    body.push_str(
        "</ul><div class=\"sidebar-foot\">Only your latest conversations are shown.</div>",
    );
    if current.is_none() {
        body.push_str(&footer("/", context, "Refresh for new messages"));
    }
    body.push_str("</nav>");
    body
}
pub fn login(csrf: &str, context: Context<'_>) -> String {
    let content = format!(
        "<aside class=\"login-aside\"><div><span class=\"welcome-icon\">{}</span><h2>Keep the conversation<br>close.</h2><p>A simple space for your messages.<br>On whichever screen feels right.</p></div></aside><section class=\"login-panel\"><h1>Welcome back.</h1><p class=\"login-description\">Sign in to open your conversations.</p><form action=\"/login\" method=\"post\">{}{}<label for=\"password\">Password</label><input id=\"password\" type=\"password\" name=\"pw\" size=\"12\" maxlength=\"1024\" autocomplete=\"current-password\" required><button type=\"submit\">Open conversations</button></form><p class=\"login-note\">Use the password set for your gateway.</p>{}</section>",
        icon("chat"),
        hidden("csrf", csrf),
        context.fields(),
        footer("/", context, "")
    );
    page(content, "login", "Sign in", context)
}
pub fn chats(chats: &[Chat], context: Context<'_>) -> String {
    let welcome = format!(
        "<section class=\"welcome\" aria-label=\"No conversation selected\"><div class=\"welcome-copy\"><div class=\"welcome-icon\">{}</div><h1>A good place to<br>pick things up.</h1><p>Choose a conversation to read your latest<br>messages and send a little hello.</p><span class=\"welcome-hint\">Text first. Distractions last.</span></div></section>",
        icon("chat")
    );
    let nav = sidebar(chats, None, context, false);
    page(format!("{nav}{welcome}"), "list", "Conversations", context)
}
pub fn conversation(
    conversation: &Conversation,
    chats: &[Chat],
    csrf: &str,
    context: Context<'_>,
    read_failed: bool,
    sidebar_failed: bool,
    refresh_id: &str,
) -> String {
    let path = format!("/chat/{}", url(&conversation.chat.id));
    let refresh_path = context.href(&path);
    let separator = if refresh_path.contains('?') { '&' } else { '?' };
    let refresh_path = format!(
        "{refresh_path}{separator}refresh={}#messages-end",
        url(refresh_id)
    );
    let nav = if context.layout == Layout::Compact {
        String::new()
    } else {
        sidebar(chats, Some(&conversation.chat.id), context, sidebar_failed)
    };
    let mut body = format!(
        "{nav}<section class=\"thread\" aria-label=\"Conversation\"><header class=\"thread-header\"><a class=\"back-link\" href=\"{}\" aria-label=\"Back to conversations\">{}<span class=\"back-word\">Back</span></a><div class=\"thread-person\">{}<div><h1>{}</h1><p class=\"thread-subtitle\">Latest synced messages · Times in UTC</p></div></div><div class=\"thread-actions\"><a class=\"text-link\" href=\"{}\" aria-label=\"Refresh conversation\">{}Refresh</a></div></header><div class=\"thread-scroll\" id=\"messages\"><p class=\"thread-label\">RECENT MESSAGES</p>",
        escape(&context.href("/")),
        icon("back"),
        avatar(&conversation.chat.name, 0),
        escape(&conversation.chat.name),
        escape(&refresh_path),
        icon("refresh")
    );
    if read_failed {
        body.push_str("<p class=\"notice\" role=\"status\">Read status could not be updated.</p>");
    }
    if conversation.messages.is_empty() {
        body.push_str("<p class=\"empty-list\">No messages synced yet.</p>");
    }
    for msg in &conversation.messages {
        let own = if msg.from_me { " message--own" } else { "" };
        let sender = if msg.from_me { "You" } else { &msg.sender_name };
        let media = msg
            .media
            .as_ref()
            .map(|kind| format!("<span class=\"media-tag\">[{}]</span>", escape(kind)))
            .unwrap_or_default();
        body.push_str(&format!("<article class=\"message{own}\"><div class=\"bubble\"><span class=\"message-author\">{}</span>{media}<p class=\"message-text\">{}</p><time class=\"message-time\" title=\"UTC\">{}</time></div></article>", escape(sender), escape(&msg.text), escape(&msg.time)));
    }
    body.push_str(&format!("<span id=\"messages-end\"></span></div><form class=\"composer\" action=\"/send\" method=\"post\">{}{}{}<label class=\"compose-label\" for=\"message\">Message</label><div class=\"compose-row\"><textarea id=\"message\" name=\"msg\" cols=\"12\" rows=\"2\" placeholder=\"Write a message…\" required></textarea><button type=\"submit\">Send{}</button></div><p class=\"composer-note\">Up to 4,096 characters. Refresh to see new messages.</p></form>{}</section>", hidden("to",&conversation.chat.id), hidden("csrf",csrf), context.fields(), icon("send"), footer(&path,context, "Refresh for new messages")));
    page(body, "chat", &conversation.chat.name, context)
}
