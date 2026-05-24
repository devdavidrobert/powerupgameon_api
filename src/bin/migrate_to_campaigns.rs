use anyhow::{Context, Result};
use clap::Parser;
use firestore::FirestoreQueryDirection;
use powerupgameon_api::config::Config;
use powerupgameon_api::features::campaigns::domain::{CampaignStatus, GeoEnforcement, StaggerMode};
use powerupgameon_api::features::campaigns::infrastructure::CampaignPaths;
use powerupgameon_api::init_crypto_providers;
use powerupgameon_api::services::firestore::FirestoreService;
use powerupgameon_api::utils::firestore::document_id_from_map;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

#[derive(Parser)]
#[command(name = "migrate-to-campaigns")]
struct Args {
    #[arg(long, default_value = "default")]
    slug: String,
    #[arg(long, default_value = "Legacy Campaign")]
    name: String,
}

/// Each migrated doc adds 2 batch ops (copy to campaign subcollection + delete root doc).
/// Firestore WriteBatch limit is 500 ops, so flush every 250 documents.
const MIGRATE_BATCH_FLUSH_EVERY: usize = 250;

const ROOT_COLLECTIONS: &[&str] = &[
    "questions",
    "prizes",
    "registrations",
    "submissions",
    "sessions",
    "spin_tokens",
    "raffles",
    "raffle_winners",
];

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    init_crypto_providers()?;
    let args = Args::parse();
    let config = Config::load()?;
    let db = FirestoreService::new(&config).await?;

    let existing: Vec<Map<String, Value>> = db
        .client
        .fluent()
        .select()
        .from("campaigns")
        .filter(|q| q.field("slug").eq(&args.slug))
        .obj()
        .query()
        .await
        .context("query campaigns")?;

    let campaign_id = if let Some(doc) = existing.into_iter().next() {
        doc.get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string()
    } else {
        create_campaign(&db, &args.slug, &args.name).await?
    };

    println!("Using campaign id: {campaign_id}");

    let settings: Option<Map<String, Value>> = db
        .client
        .fluent()
        .select()
        .by_id_in("settings")
        .obj()
        .one("general")
        .await
        .context("read settings")?;

    if let Some(settings) = settings {
        let mut payload = Map::new();
        if let Some(v) = settings.get("challengeStartTime") {
            payload.insert("challengeStartTime".into(), v.clone());
        }
        if let Some(v) = settings.get("challengeEndTime") {
            payload.insert("challengeEndTime".into(), v.clone());
        }
        if !payload.is_empty() {
            db.client
                .fluent()
                .update()
                .in_col("campaigns")
                .document_id(&campaign_id)
                .object(&payload)
                .execute::<Map<String, Value>>()
                .await
                .context("copy settings timers")?;
        }
    }

    let paths = CampaignPaths::new(&campaign_id);
    let parent = paths.parent(&db.client)?;

    for collection in ROOT_COLLECTIONS {
        migrate_collection(&db, collection, parent.as_ref()).await?;
    }

    let default_location_id = ensure_default_location(&db, &paths).await?;

    let prize_counts = read_prize_counts(&db).await?;
    migrate_inventory(&db, &paths, &default_location_id, &prize_counts).await?;

    backfill_location_ids(&db, &paths, &default_location_id).await?;

    println!("Migration complete for campaign slug '{}'.", args.slug);
    Ok(())
}

async fn create_campaign(db: &FirestoreService, slug: &str, name: &str) -> Result<String> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    db.client
        .fluent()
        .insert()
        .into("campaigns")
        .document_id(&id)
        .object(&json!({
            "id": id,
            "slug": slug,
            "name": name,
            "status": CampaignStatus::Active.as_str(),
            "staggerMode": StaggerMode::Linear.as_str(),
            "geoEnforcement": GeoEnforcement::Reject.as_str(),
            "createdAt": now,
            "updatedAt": now,
        }))
        .execute::<Map<String, Value>>()
        .await
        .context("create campaign")?;
    Ok(id)
}

async fn migrate_collection(db: &FirestoreService, collection: &str, parent: &str) -> Result<()> {
    let docs: Vec<Map<String, Value>> = db
        .client
        .fluent()
        .select()
        .from(collection)
        .obj()
        .query()
        .await
        .with_context(|| format!("read root collection {collection}"))?;

    if docs.is_empty() {
        println!("No documents in root '{collection}' — skipping.");
        return Ok(());
    }

    let writer = db.batch_writer().await?;
    let mut batch = writer.new_batch();
    let mut count = 0usize;

    for doc in docs {
        let Some(id) = document_id_from_map(&doc) else {
            eprintln!("Skipping document in '{collection}' with no resolvable id: {doc:?}");
            continue;
        };

        db.client
            .fluent()
            .update()
            .in_col(collection)
            .document_id(&id)
            .parent(&parent)
            .object(&doc)
            .add_to_batch(&mut batch)
            .with_context(|| format!("stage {collection}/{id}"))?;

        db.client
            .fluent()
            .delete()
            .from(collection)
            .document_id(&id)
            .add_to_batch(&mut batch)
            .with_context(|| format!("delete root {collection}/{id}"))?;

        count += 1;
        if count % MIGRATE_BATCH_FLUSH_EVERY == 0 {
            batch.write().await?;
            batch = writer.new_batch();
        }
    }

    batch.write().await?;
    println!("Migrated {count} documents from '{collection}'.");
    Ok(())
}

async fn ensure_default_location(db: &FirestoreService, paths: &CampaignPaths) -> Result<String> {
    let parent = paths
        .parent_str(&db.client)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let existing: Vec<Map<String, Value>> = db
        .client
        .fluent()
        .select()
        .from("locations")
        .parent(&parent)
        .obj()
        .query()
        .await?;

    if let Some(doc) = existing.into_iter().next() {
        let id = document_id_from_map(&doc)
            .ok_or_else(|| anyhow::anyhow!("existing location document has no resolvable id"))?;
        return Ok(id);
    }

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    db.client
        .fluent()
        .update()
        .in_col("locations")
        .document_id(&id)
        .parent(&parent)
        .object(&json!({
            "id": id,
            "name": "All Regions",
            "centerLat": 0.0,
            "centerLng": 0.0,
            "radiusMeters": 20_000_000.0,
            "enabled": true,
            "createdAt": now,
            "updatedAt": now,
        }))
        .execute::<Map<String, Value>>()
        .await?;
    Ok(id)
}

async fn read_prize_counts(db: &FirestoreService) -> Result<HashMap<String, i64>> {
    let doc: Option<Map<String, Value>> = db
        .client
        .fluent()
        .select()
        .by_id_in("system")
        .obj()
        .one("aggregates")
        .await?;

    let mut counts = HashMap::new();
    if let Some(doc) = doc {
        if let Some(obj) = doc.get("prizeAwardCounts").and_then(|v| v.as_object()) {
            for (k, v) in obj {
                if let Some(n) = v.as_i64() {
                    counts.insert(k.clone(), n);
                }
            }
        }
    }
    Ok(counts)
}

async fn migrate_inventory(
    db: &FirestoreService,
    paths: &CampaignPaths,
    location_id: &str,
    prize_counts: &HashMap<String, i64>,
) -> Result<()> {
    let parent = paths
        .parent_str(&db.client)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let prizes: Vec<Map<String, Value>> = db
        .client
        .fluent()
        .select()
        .from("prizes")
        .parent(&parent)
        .order_by([("order", FirestoreQueryDirection::Ascending)])
        .obj()
        .query()
        .await?;

    let prize_count = prizes.len();
    for prize in &prizes {
        let prize_id = prize
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let prize_name = prize.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let is_real = prize
            .get("isRealPrize")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        if !is_real || prize_id.is_empty() {
            continue;
        }
        let awarded = prize_counts.get(prize_name).copied().unwrap_or(0);
        let total_quantity = awarded.max(1);
        let slot_id = format!("{location_id}_{prize_id}");
        db.client
            .fluent()
            .update()
            .in_col("inventory")
            .document_id(&slot_id)
            .parent(&parent)
            .object(&json!({
                "locationId": location_id,
                "prizeId": prize_id,
                "totalQuantity": total_quantity,
                "awardedCount": awarded,
                "updatedAt": chrono::Utc::now().timestamp_millis(),
            }))
            .execute::<Map<String, Value>>()
            .await?;
    }

    println!("Created inventory slots for {} prizes.", prize_count);
    Ok(())
}

async fn backfill_location_ids(
    db: &FirestoreService,
    paths: &CampaignPaths,
    location_id: &str,
) -> Result<()> {
    let parent = paths
        .parent_str(&db.client)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    for collection in ["registrations", "submissions"] {
        let docs: Vec<Map<String, Value>> = db
            .client
            .fluent()
            .select()
            .from(collection)
            .parent(&parent)
            .obj()
            .query()
            .await?;

        let writer = db.batch_writer().await?;
        let mut batch = writer.new_batch();
        let mut count = 0usize;

        for doc in docs {
            if doc.get("locationId").is_some() {
                continue;
            }
            let Some(id) = document_id_from_map(&doc).or_else(|| {
                doc.get("sessionId")
                    .and_then(|v| v.as_str())
                    .map(String::from)
            }) else {
                eprintln!("Skipping '{collection}' doc with no id during location backfill.");
                continue;
            };
            db.client
                .fluent()
                .update()
                .in_col(collection)
                .document_id(&id)
                .parent(&parent)
                .object(&json!({
                    "locationId": location_id,
                    "geoStatus": "valid",
                }))
                .add_to_batch(&mut batch)?;
            count += 1;
        }

        if count > 0 {
            batch.write().await?;
            println!("Backfilled locationId on {count} '{collection}' documents.");
        }
    }
    Ok(())
}
