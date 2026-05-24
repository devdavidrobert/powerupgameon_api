use clap::Parser;
use powerupgameon_api::config::Config;
use powerupgameon_api::services::firebase_auth::FirebaseAuth;
use powerupgameon_api::services::firestore::FirestoreService;

#[derive(Parser)]
#[command(name = "update-auth-user-email")]
struct Args {
    uid: String,
    email: String,
    #[arg(long)]
    no_verify: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    powerupgameon_api::init_crypto_providers()?;
    let args = Args::parse();

    let config = Config::load()?;
    let db = FirestoreService::new(&config).await?;
    let auth = FirebaseAuth::new(&config, &db).await?;
    let user = auth
        .update_user_email(&args.uid, &args.email, !args.no_verify)
        .await?;
    println!("Updated user: {user}");
    Ok(())
}
