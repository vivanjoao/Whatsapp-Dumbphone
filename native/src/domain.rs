use async_trait::async_trait;

pub const CHAT_LIMIT: usize = 20;
pub const MESSAGE_LIMIT: usize = 10;
pub const MESSAGE_MAX: usize = 4096;
#[derive(Clone, Debug)]
pub struct Chat {
    pub id: String,
    pub name: String,
    pub unread: i32,
}
#[derive(Clone, Debug)]
pub struct Message {
    pub id: String,
    pub sender_id: String,
    pub sender_name: String,
    pub text: String,
    pub from_me: bool,
    pub time: String,
    pub media: Option<String>,
}
#[derive(Clone, Debug)]
pub struct Conversation {
    pub chat: Chat,
    pub messages: Vec<Message>,
}
#[derive(Debug, Clone, Copy)]
pub enum GatewayError {
    Unavailable,
    NotFound,
    Failed,
}
#[async_trait]
pub trait Gateway: Send + Sync {
    fn ready(&self) -> bool;
    async fn chats(&self) -> Result<Vec<Chat>, GatewayError>;
    async fn conversation(&self, id: &str) -> Result<Conversation, GatewayError>;
    async fn mark_read(&self, conversation: &Conversation) -> Result<(), GatewayError>;
    async fn send(&self, id: &str, text: &str) -> Result<(), GatewayError>;
}
pub fn valid_id(id: &str) -> bool {
    if id.len() > 80 {
        return false;
    }
    let Some((user, server)) = id.split_once('@') else {
        return false;
    };
    matches!(server, "s.whatsapp.net" | "c.us" | "g.us" | "lid")
        && !user.is_empty()
        && user.split('-').count() <= 2
        && user
            .split('-')
            .all(|p| !p.is_empty() && p.len() <= 20 && p.bytes().all(|b| b.is_ascii_digit()))
}
pub fn valid_text(text: &str) -> bool {
    !text.trim().is_empty() && text.chars().count() <= MESSAGE_MAX
}
