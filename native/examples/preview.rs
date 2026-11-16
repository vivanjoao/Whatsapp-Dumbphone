//! Offline UI fixture. This example never constructs a WhatsApp client.
use anyhow::Result;
use async_trait::async_trait;
use dumbphone_gateway::{
    config::Config,
    domain::{Chat, Conversation, Gateway, GatewayError, Message},
    http::{self, App},
    store::Store,
};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;

const TOKEN: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
struct Preview {
    conversations: Mutex<Vec<Conversation>>,
}
#[async_trait]
impl Gateway for Preview {
    fn ready(&self) -> bool {
        true
    }
    async fn chats(&self) -> Result<Vec<Chat>, GatewayError> {
        Ok(self
            .conversations
            .lock()
            .await
            .iter()
            .map(|c| c.chat.clone())
            .collect())
    }
    async fn conversation(&self, id: &str) -> Result<Conversation, GatewayError> {
        self.conversations
            .lock()
            .await
            .iter()
            .find(|c| c.chat.id == id)
            .cloned()
            .ok_or(GatewayError::NotFound)
    }
    async fn mark_read(&self, conversation: &Conversation) -> Result<(), GatewayError> {
        if let Some(c) = self
            .conversations
            .lock()
            .await
            .iter_mut()
            .find(|c| c.chat.id == conversation.chat.id)
        {
            c.chat.unread = 0;
        }
        Ok(())
    }
    async fn send(&self, id: &str, text: &str) -> Result<(), GatewayError> {
        let mut conversations = self.conversations.lock().await;
        let chat = conversations
            .iter_mut()
            .find(|c| c.chat.id == id)
            .ok_or(GatewayError::NotFound)?;
        chat.messages.push(Message {
            id: format!("preview-{}", chat.messages.len()),
            sender_id: "999@lid".into(),
            sender_name: "You".into(),
            text: text.into(),
            from_me: true,
            time: "10:00".into(),
            media: None,
        });
        Ok(())
    }
}
fn fixture() -> Preview {
    let people = [
        ("Ana Martins", 2),
        ("Product studio", 4),
        ("Lucas Ferreira", 0),
        ("Marina Costa", 0),
        ("Weekend plans", 1),
        ("Felipe Santos", 0),
        ("Clara Oliveira", 0),
    ];
    let conversations = people
        .iter()
        .enumerate()
        .map(|(i, (name, unread))| {
            let id = format!(
                "{}@{}",
                100 + i,
                if matches!(i, 1 | 4) {
                    "g.us"
                } else {
                    "s.whatsapp.net"
                }
            );
            let messages = [
                (
                    false,
                    "Morning! Did you get a chance to look at the new sketches?",
                    "09:41",
                ),
                (
                    true,
                    "I did. The simpler direction feels really good.",
                    "09:42",
                ),
                (
                    false,
                    "Exactly what I was thinking. More space, fewer distractions.",
                    "09:43",
                ),
                (
                    false,
                    "Want to grab a coffee and go through the details?",
                    "09:43",
                ),
                (true, "Sounds perfect. The usual place at 11?", "09:44"),
                (false, "See you there ☕", "09:45"),
            ]
            .iter()
            .enumerate()
            .map(|(n, (own, text, time))| Message {
                id: n.to_string(),
                sender_id: id.clone(),
                sender_name: name.to_string(),
                text: text.to_string(),
                from_me: *own,
                time: time.to_string(),
                media: None,
            })
            .collect();
            Conversation {
                chat: Chat {
                    id,
                    name: name.to_string(),
                    unread: *unread,
                },
                messages,
            }
        })
        .collect();
    Preview {
        conversations: Mutex::new(conversations),
    }
}
#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::from_values(HashMap::from([
        ("PASSWORD".into(), "preview-password-only".into()),
        (
            "SESSION_SECRET".into(),
            "preview-secret-not-for-production".repeat(2),
        ),
        ("ACCESS_TOKEN".into(), TOKEN.into()),
    ]))?;
    let dir = tempfile::tempdir()?;
    let store = Store::open(&dir.path().join("preview.db")).await?;
    let app = http::router(App::new(config, store, Arc::new(fixture())));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3080").await?;
    println!("Offline preview (fictional data only): http://127.0.0.1:3080/?token={TOKEN}");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
    Ok(())
}
