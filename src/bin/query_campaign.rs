use powerupgameon_api::config::Config;
use powerupgameon_api::init_crypto_providers;
use powerupgameon_api::services::firestore::FirestoreService;
use serde_json::Map;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    init_crypto_providers()?;
    let slug = std::env::args().nth(1).unwrap_or_else(|| "test3".into());
    let config = Config::load()?;
    let db = FirestoreService::new(&config).await?;

    let rows: Vec<Map<String, serde_json::Value>> = db
        .client
        .fluent()
        .select()
        .from("campaigns")
        .filter(|q| q.field("slug").eq(&slug))
        .obj()
        .query()
        .await?;

    if rows.is_empty() {
        println!("No campaign with slug '{slug}'");
    } else {
        for doc in rows {
            println!("{doc:#?}");
        }
    }
    Ok(())
}
