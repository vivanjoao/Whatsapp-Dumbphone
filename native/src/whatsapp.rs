//! Native protocol boundary. No HTTP types or HTML cross this module.
use crate::domain::{self, Chat, Conversation, Gateway, GatewayError, Message};
use anyhow::Result;
use async_trait::async_trait;
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::Mutex;
use whatsapp_rust::store::{SqliteStoreConfig, Synchronous};
use whatsapp_rust::wacore::types::events::{BatchOrigin, InboundMessage, MessageBatch};
use whatsapp_rust::{InboundDurabilityHook, prelude::*};
use whatsapp_rust_chat_store::ChatStore;

pub struct NativeGateway {
    client: Arc<Client>,
    history: Arc<ChatStore>,
    healthy: Arc<AtomicBool>,
    connected: Arc<AtomicBool>,
}
struct DurableInbox {
    history: Arc<ChatStore>,
    gate: Mutex<()>,
    healthy: Arc<AtomicBool>,
}
#[async_trait]
impl InboundDurabilityHook for DurableInbox {
    async fn on_messages(&self, _client: Arc<Client>, batch: &[InboundMessage]) -> Result<()> {
        self.persist(batch).await
    }
}
impl DurableInbox {
    async fn persist(&self, batch: &[InboundMessage]) -> Result<()> {
        let _guard = self.gate.lock().await;
        // The SDK acknowledges only after every chunk has been durably committed.
        // Replay is safe: chat-store deduplicates its stored message identity.
        for chunk in batch.chunks(128) {
            let event = Event::Messages(
                MessageBatch::builder()
                    .messages(Arc::<[InboundMessage]>::from(chunk))
                    .origin(BatchOrigin::Live)
                    .build(),
            );
            self.history.handler().handle_event(Arc::new(event));
            if let Err(err) = self.history.flush().await {
                self.healthy.store(false, Ordering::Release);
                return Err(err.into());
            }
        }
        Ok(())
    }
}
impl NativeGateway {
    pub async fn open(dir: &Path) -> Result<(Arc<Self>, Bot)> {
        let backend = SqliteStore::with_config(
            dir.join("whatsapp.db").to_string_lossy().as_ref(),
            SqliteStoreConfig {
                read_pool_size: 2,
                synchronous: Synchronous::Full,
                busy_timeout: std::time::Duration::from_secs(5),
                ..Default::default()
            },
        )
        .await?;
        let history = ChatStore::new(&backend).await?;
        history.skip_hook_committed_batches(true);
        let healthy = Arc::new(AtomicBool::new(true));
        let connected = Arc::new(AtomicBool::new(false));
        let event_connected = connected.clone();
        let event_history = history.clone();
        let event_health = healthy.clone();
        let bot = Bot::builder().with_backend(backend)
            .with_event_delivery(whatsapp_rust::bot::EventDelivery::Ordered { capacity: 128 })
            .with_inbound_durability_hook(DurableInbox { history: history.clone(), gate: Mutex::new(()), healthy: healthy.clone() })
            .on_qr_code(|payload, _| async move {
                if let Ok(code) = qrcode::QrCode::new(payload.as_bytes()) {
                    eprintln!("Pair using the smartphone's Linked Devices menu (administrative terminal only):\n{}",code.render::<qrcode::render::unicode::Dense1x2>().build());
                }
            })
            .on_event(move |event, _| {
                let history = event_history.clone();
                let healthy = event_health.clone();
                let connected = event_connected.clone();
                async move {
                    match event.as_ref() {
                        Event::Connected(_) => connected.store(true, Ordering::Release),
                        Event::Disconnected(_) | Event::LoggedOut(_) => connected.store(false, Ordering::Release),
                        _ => {},
                    }
                    history.handler().handle_event(event);
                    if history.flush().await.is_err() { healthy.store(false,Ordering::Release); }
                }
            }).build().await?;
        let gateway = Arc::new(Self {
            client: bot.client(),
            history,
            healthy,
            connected,
        });
        Ok((gateway, bot))
    }
    pub async fn flush(&self) -> Result<()> {
        self.history.flush().await?;
        Ok(())
    }
    fn jid(id: &str) -> Result<Jid, GatewayError> {
        if !domain::valid_id(id) {
            return Err(GatewayError::NotFound);
        }
        id.replace("@c.us", "@s.whatsapp.net")
            .parse()
            .map_err(|_| GatewayError::NotFound)
    }
}
fn failed<T>(result: Result<T, impl std::fmt::Display>) -> Result<T, GatewayError> {
    result.map_err(|_| GatewayError::Failed)
}
#[async_trait]
impl Gateway for NativeGateway {
    fn ready(&self) -> bool {
        self.healthy.load(Ordering::Acquire)
            && self.connected.load(Ordering::Acquire)
            && self.client.is_connected()
            && self.client.is_logged_in()
            && self.client.stats().events_dropped == 0
    }
    async fn chats(&self) -> Result<Vec<Chat>, GatewayError> {
        let rows = failed(self.history.chats(true, domain::CHAT_LIMIT as i64).await)?;
        Ok(rows
            .into_iter()
            .map(|c| Chat {
                id: c.jid.to_string(),
                name: c.name.unwrap_or_else(|| c.jid.to_string()),
                unread: c.unread_count,
            })
            .collect())
    }
    async fn conversation(&self, id: &str) -> Result<Conversation, GatewayError> {
        let jid = Self::jid(id)?;
        let c = failed(self.history.chat(&jid).await)?.ok_or(GatewayError::NotFound)?;
        let mut rows = failed(
            self.history
                .messages(&jid, None, domain::MESSAGE_LIMIT as i64)
                .await,
        )?;
        rows.reverse();
        let mut messages = Vec::with_capacity(rows.len());
        for row in rows {
            let contact = failed(self.history.contact(&row.sender_jid).await)?;
            let name = contact
                .as_ref()
                .and_then(|c| c.display_name())
                .map(str::to_owned)
                .unwrap_or_else(|| row.sender_jid.to_string());
            let kind = row.kind.as_str();
            messages.push(Message {
                id: row.id,
                sender_id: row.sender_jid.to_string(),
                sender_name: name,
                text: if row.revoked {
                    "[Message removed]".into()
                } else {
                    row.text
                        .unwrap_or_default()
                        .chars()
                        .take(domain::MESSAGE_MAX)
                        .collect()
                },
                from_me: row.from_me,
                time: row.timestamp.format("%H:%M").to_string(),
                media: (kind != "text").then(|| kind.to_owned()),
            });
        }
        Ok(Conversation {
            chat: Chat {
                id: c.jid.to_string(),
                name: c.name.unwrap_or_else(|| c.jid.to_string()),
                unread: c.unread_count,
            },
            messages,
        })
    }
    async fn mark_read(&self, conversation: &Conversation) -> Result<(), GatewayError> {
        let jid = Self::jid(&conversation.chat.id)?;
        for msg in conversation.messages.iter().filter(|m| !m.from_me) {
            let sender = Self::jid(&msg.sender_id)?;
            failed(
                self.client
                    .mark_as_read(&jid, Some(&sender), &[&msg.id])
                    .await,
            )?;
        }
        failed(
            self.client
                .chat_actions()
                .mark_chat_as_read(&jid, true, None)
                .await,
        )?;
        Ok(())
    }
    async fn send(&self, id: &str, text: &str) -> Result<(), GatewayError> {
        if !self.ready() {
            return Err(GatewayError::Unavailable);
        }
        let jid = Self::jid(id)?;
        let message = wa::Message::text(text);
        let sent = failed(self.client.send_message(jid.clone(), message.clone()).await)?;
        failed(
            self.history
                .record_outgoing(&jid, sent.message_id, &message, chrono::Utc::now()),
        )?;
        failed(self.history.flush().await)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use whatsapp_rust::wacore::types::message::{MessageInfo, MessageSource};
    #[tokio::test]
    async fn native_store_materializes_duplicate_inbound_messages_once() {
        let dir = tempfile::tempdir().unwrap();
        let backend = SqliteStore::new(dir.path().join("test.db").to_str().unwrap())
            .await
            .unwrap();
        let history = ChatStore::new(&backend).await.unwrap();
        let inbox = DurableInbox {
            history: history.clone(),
            gate: Mutex::new(()),
            healthy: Arc::new(AtomicBool::new(true)),
        };
        let jid: Jid = "123@s.whatsapp.net".parse().unwrap();
        let item = InboundMessage::builder()
            .message(Arc::new(wa::Message::text("Olá native")))
            .info(Arc::new(MessageInfo {
                id: "test-id".into(),
                push_name: "Contact".into(),
                timestamp: chrono::Utc::now(),
                source: MessageSource {
                    chat: jid.clone(),
                    sender: jid.clone(),
                    ..Default::default()
                },
                ..Default::default()
            }))
            .build();
        inbox.persist(&[item.clone(), item.clone()]).await.unwrap();
        inbox.persist(&[item]).await.unwrap();
        let messages = history.messages(&jid, None, 10).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].text.as_deref(), Some("Olá native"));
        let chats = history.chats(true, 20).await.unwrap();
        assert_eq!(chats.len(), 1);
        assert_eq!(chats[0].unread_count, 1);
        // A separately opened query view sees the durable commit, not a cache.
        let second = ChatStore::new(&backend).await.unwrap();
        assert_eq!(second.messages(&jid, None, 10).await.unwrap().len(), 1);
    }
}
