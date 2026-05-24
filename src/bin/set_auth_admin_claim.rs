use clap::Parser;
use powerupgameon_api::config::Config;
use powerupgameon_api::services::firebase_auth::FirebaseAuth;
use powerupgameon_api::services::firestore::FirestoreService;

#[derive(Parser)]
#[command(name = "set-auth-admin-claim")]
struct Args {
    uid: String,
    #[arg(long)]
    grant: bool,
    #[arg(long)]
    revoke: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    powerupgameon_api::init_crypto_providers()?;
    let args = Args::parse();
    if (args.grant && args.revoke) || (!args.grant && !args.revoke) {
        eprintln!("Usage: set-auth-admin-claim <uid> --grant | --revoke");
        std::process::exit(1);
    }

    let config = Config::load()?;
    let db = FirestoreService::new(&config).await?;
    let auth = FirebaseAuth::new(&config, &db).await?;
    auth.set_admin_claim(&args.uid, args.grant).await?;
    if args.grant {
        println!("Granted admin:true for UID {}", args.uid);
    } else {
        println!("Revoked admin claim for UID {}", args.uid);
    }
    println!("Tell the user to sign out and sign in again.");
    Ok(())
}
