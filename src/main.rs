use std::env;
use std::error::Error;
use std::fs::File;
use std::io::Read;

#[actix_web::main]
pub async fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let confname = args.next().expect("missing arguments: config command");
    let mut conftext = String::new();
    File::open(confname)?
        .read_to_string(&mut conftext)?;

    let config = toml::from_str(&conftext)?;

    match args.next().expect("missing argument: command").as_str() {
        "start" => {
            music_quiz::routing::start_server(config).await?;
        },
        "migrate" => {
            let pool = sqlx::PgPool::connect_lazy(&config.database_url)?;
            sqlx::migrate!()
                .run(&pool)
                .await?;
        },
        _ => {
            panic!("invalid command: must be either start or migrate");
        }
    }

    Ok(())
}
