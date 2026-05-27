use crate::features::admin_events::domain::{AdminLiveEvent, AdminLiveTopic};
use futures_util::Stream;
use futures_util::StreamExt;
use redis::AsyncCommands;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::broadcast;

const CHANNEL_PREFIX: &str = "admin_live:";

#[derive(Clone)]
pub struct AdminEventBus {
    local: Arc<broadcast::Sender<String>>,
    redis_url: Option<String>,
}

impl AdminEventBus {
    pub fn new(redis_url: Option<String>) -> Self {
        let (local, _) = broadcast::channel(512);
        Self {
            local: Arc::new(local),
            redis_url,
        }
    }

    pub async fn publish(
        &self,
        redis: &Option<redis::aio::ConnectionManager>,
        campaign_id: &str,
        event: AdminLiveEvent,
    ) {
        let payload = match serde_json::to_string(&event) {
            Ok(payload) => payload,
            Err(err) => {
                tracing::warn!(%err, "admin_live_event_serialize_failed");
                return;
            }
        };

        if let Some(conn) = redis {
            let channel = format!("{CHANNEL_PREFIX}{campaign_id}");
            let mut connection = conn.clone();
            if let Err(err) = connection.publish::<_, _, ()>(channel, payload.clone()).await {
                tracing::warn!(%err, "admin_live_event_redis_publish_failed");
            }
            return;
        }

        let _ = self.local.send(payload);
    }

    pub fn subscribe(
        &self,
        campaign_id: &str,
        topics: Vec<AdminLiveTopic>,
    ) -> Pin<Box<dyn Stream<Item = AdminLiveEvent> + Send>> {
        if let Some(redis_url) = self.redis_url.clone() {
            return Box::pin(redis_subscription_stream(redis_url, campaign_id.to_string(), topics));
        }

        let receiver = self.local.subscribe();
        let campaign_id = campaign_id.to_string();
        Box::pin(local_subscription_stream(receiver, campaign_id, topics))
    }
}

fn redis_subscription_stream(
    redis_url: String,
    campaign_id: String,
    topics: Vec<AdminLiveTopic>,
) -> impl Stream<Item = AdminLiveEvent> + Send {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let channel = format!("{CHANNEL_PREFIX}{campaign_id}");

    tokio::spawn(async move {
        let client = match redis::Client::open(redis_url.as_str()) {
            Ok(client) => client,
            Err(err) => {
                tracing::error!(%err, "admin_live_event_redis_client_failed");
                return;
            }
        };

        let mut pubsub = match client.get_async_pubsub().await {
            Ok(pubsub) => pubsub,
            Err(err) => {
                tracing::error!(%err, "admin_live_event_redis_pubsub_failed");
                return;
            }
        };

        if let Err(err) = pubsub.subscribe(channel).await {
            tracing::error!(%err, "admin_live_event_redis_subscribe_failed");
            return;
        }

        let mut message_stream = pubsub.on_message();
        while let Some(message) = message_stream.next().await {
            match message.get_payload::<String>() {
                Ok(payload) => {
                    if tx.send(payload).is_err() {
                        break;
                    }
                }
                Err(err) => tracing::warn!(%err, "admin_live_event_redis_payload_failed"),
            }
        }
    });

    futures_util::stream::unfold((rx, topics), |(mut rx, topics)| async move {
        loop {
            let payload = rx.recv().await?;
            if let Some(event) = parse_event(&payload, &topics) {
                return Some((event, (rx, topics)));
            }
        }
    })
}

fn local_subscription_stream(
    receiver: broadcast::Receiver<String>,
    campaign_id: String,
    topics: Vec<AdminLiveTopic>,
) -> impl Stream<Item = AdminLiveEvent> + Send {
    futures_util::stream::unfold(
        (receiver, campaign_id, topics),
        |(mut receiver, campaign_id, topics)| async move {
            loop {
                match receiver.recv().await {
                    Ok(payload) => {
                        if event_matches_campaign(&payload, &campaign_id) {
                            if let Some(event) = parse_event(&payload, &topics) {
                                return Some((event, (receiver, campaign_id, topics)));
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => return None,
                }
            }
        },
    )
}

fn event_matches_campaign(payload: &str, campaign_id: &str) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) else {
        return true;
    };
    value
        .get("campaignId")
        .and_then(|v| v.as_str())
        .is_none_or(|id| id == campaign_id)
}

fn parse_event(payload: &str, topics: &[AdminLiveTopic]) -> Option<AdminLiveEvent> {
    let value: serde_json::Value = serde_json::from_str(payload).ok()?;
    let event_value = value.get("event").cloned().unwrap_or(value);
    let event: AdminLiveEvent = serde_json::from_value(event_value).ok()?;
    let topic = AdminLiveTopic::parse(&event.topic)?;
    if !topics.contains(&topic) {
        return None;
    }
    Some(event)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::admin_events::domain::{AdminLiveChange, AdminLiveEvent};

    #[tokio::test]
    async fn local_bus_delivers_matching_campaign_events() {
        let bus = AdminEventBus::new(None);
        let mut stream = bus.subscribe("campaign-1", vec![AdminLiveTopic::Registrations]);

        let bus_clone = bus.clone();
        let publish = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            bus_clone
                .publish(
                    &None,
                    "campaign-1",
                    AdminLiveEvent::new(
                        AdminLiveTopic::Registrations,
                        AdminLiveChange::Added,
                        "session-1",
                        None,
                    ),
                )
                .await;
        });

        let event = tokio::time::timeout(std::time::Duration::from_secs(1), stream.next())
            .await
            .expect("timed out waiting for admin event")
            .expect("stream ended");
        publish.await.expect("publish task failed");
        assert_eq!(event.id, "session-1");
        assert_eq!(event.topic, "registrations");
    }
}
