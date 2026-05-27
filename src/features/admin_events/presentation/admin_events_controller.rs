use crate::app_state::AppState;
use crate::features::admin_events::domain::parse_admin_live_topics;
use crate::features::campaigns::presentation::CampaignContext;
use axum::{
    extract::{Query, State},
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::stream::{self, StreamExt};
use serde::Deserialize;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

#[derive(Deserialize)]
pub struct AdminEventsQuery {
    pub topics: Option<String>,
}

pub async fn stream_admin_events(
    State(state): State<Arc<AppState>>,
    ctx: CampaignContext,
    Query(query): Query<AdminEventsQuery>,
) -> Sse<impl stream::Stream<Item = Result<Event, Infallible>>> {
    let topics = parse_admin_live_topics(query.topics.as_deref());
    let event_stream = state
        .admin_events
        .subscribe(ctx.campaign_id(), topics)
        .map(|event| {
            Ok(Event::default().json_data(event).unwrap_or_else(|_| {
                Event::default().comment("invalid-admin-event")
            }))
        });

    Sse::new(event_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(25))
            .text("keepalive"),
    )
}
