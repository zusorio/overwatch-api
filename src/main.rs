use actix_cors::Cors;
use actix_web::{
    error::{ErrorBadRequest, ErrorInternalServerError, ErrorNotFound},
    get, App, HttpResponse, HttpServer, Responder, Result,
};
use paperclip::actix::{
    api_v2_operation,
    web::{self, Json},
    Apiv2Schema, OpenApiExt,
};

use bb8_redis::{bb8, redis::AsyncCommands, RedisConnectionManager};
use scraper::Html;
use std::env;
use std::sync::Arc;
use tokio::sync::Semaphore;

use serde::{Deserialize, Serialize};
use std::fmt::Display;

use slog::{debug, error, info, o, Drain, Logger};

mod extractors;

struct AppState {
    client: reqwest::Client,
    semaphore: Arc<Semaphore>,
    redis_pool: bb8::Pool<RedisConnectionManager>,
    log: Logger,
}

#[derive(Apiv2Schema, Serialize, Deserialize)]
pub struct Battletag {
    name: String,
    numbers: u32,
}

impl Display for Battletag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}#{}", self.name, self.numbers)
    }
}

enum Role {
    Tank,
    Damage,
    Support,
}

#[derive(Serialize, Deserialize, Debug, Clone, Apiv2Schema)]
enum Group {
    Bronze,
    Silver,
    Gold,
    Platinum,
    Diamond,
    Master,
    Grandmaster,
}

#[derive(Serialize, Deserialize, Clone, Apiv2Schema)]
pub struct Rank {
    group: Group,
    tier: u8,
}

#[derive(Serialize, Deserialize, Apiv2Schema)]
struct Player {
    name: String,
    battletag: Battletag,
    private: bool,
    portrait: Option<String>,
    title: Option<String>,
    endorsement: u8,
    tank: Option<Rank>,
    damage: Option<Rank>,
    support: Option<Rank>,
}

fn configure_log() -> Logger {
    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain).build().fuse();
    Logger::root(drain, o!())
}

fn parsing_error() -> actix_web::Error {
    ErrorInternalServerError("Could not parse page")
}

#[api_v2_operation(
    summary = "Get data from a battletag",
    description = "Gets basic data like ranks and profile customization from a battletag by scraping the Overwatch website. Cached for 10 minutes."
)]
async fn get_battletag(
    info: web::Path<Battletag>,
    data: web::Data<AppState>,
) -> Result<Json<Player>, actix_web::Error> {
    let info = info.into_inner();

    let request_log = data
        .log
        .new(o!("battletag" => format!("{}-{}", info.name.clone(), info.numbers) ));

    info!(request_log, "Getting player");

    debug!(request_log, "Making redis connection");
    let mut redis_connection = data.redis_pool.get().await.map_err(|e| {
        error!(request_log, "Could not get redis connection, {}", e);
        ErrorInternalServerError("Could not get redis connection, this should never happen :(")
    })?;

    debug!(request_log, "Checking redis cache");
    let redis_result: redis::RedisResult<String> = redis_connection
        .get(format!("{}-{}", info.name, info.numbers))
        .await;

    if let Ok(player) = redis_result {
        debug!(request_log, "Hit redis cache, decoding...");
        let player: Player = serde_json::from_str(&player).map_err(|_| parsing_error())?;

        debug!(request_log, "Returning cached player");
        return Ok(Json(player));
    }

    let permit = data.semaphore.acquire().await.map_err(|e| {
        error!(request_log, "Could not acquire semaphore, {}", e);
        ErrorInternalServerError("Could not acquire semaphore, this should never happen :(")
    })?;

    debug!(request_log, "Making request to Blizzard server");
    let res = data
        .client
        .get(format!(
            "https://overwatch.blizzard.com/en-us/career/{}-{}",
            info.name, info.numbers
        ))
        .send()
        .await
        .map_err(ErrorInternalServerError)?;
    debug!(
        request_log,
        "Finished request to Blizzard server, dropping Semaphore permit"
    );

    drop(permit);

    match res.status() {
        reqwest::StatusCode::NOT_FOUND => Err(ErrorNotFound("Player not found")),
        status if !status.is_success() => Err(ErrorInternalServerError("Failed getting player")),
        _ => Ok(()),
    }?;

    debug!(request_log, "Reading response body");
    let body = res
        .text()
        .await
        .map_err(|_| ErrorBadRequest("Could not parse body"))?;

    debug!(request_log, "Parsing response body");
    let document = Html::parse_document(&body);

    let private_profile_selector =
        scraper::Selector::parse(".Profile-player--private").map_err(|_| parsing_error())?;

    let private_profile = document.select(&private_profile_selector).next().is_some();

    debug!(request_log, "Extracting data from page");

    let portrait = extractors::extract_portrait(&document)?;
    let title = extractors::extract_title(&document)?;
    let endorsement = extractors::extract_endorsement(&document)?;
    let (tank, damage, support) = extractors::extract_roles(&document)?;

    debug!(request_log, "Done extracting data from page");

    let battletag = Battletag {
        name: info.name.clone(),
        numbers: info.numbers,
    };

    let player = Player {
        name: battletag.to_string(),
        battletag,
        private: private_profile,
        portrait,
        title,
        endorsement,
        tank,
        damage,
        support,
    };

    debug!(request_log, "Constructing response");

    let player_json = serde_json::to_string(&player);

    if let Ok(player_json) = player_json {
        debug!(request_log, "Caching player in redis");
        let redis_result: redis::RedisResult<String> = redis_connection
            .set_ex(format!("{}-{}", info.name, info.numbers), player_json, 600)
            .await;

        if let Err(e) = redis_result {
            error!(request_log, "Redis error: {:?}", e);
        }
    }

    info!(request_log, "Responding with player");

    // Ok(HttpResponse::Ok()
    //     .insert_header(("X-Cache", "MISS"))
    //     .json(player))
    Ok(Json(player))
}

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().body(include_str!("../static/index.html"))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let log = configure_log();

    let redis_url = env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    info!(log, "Using redis url: {}", redis_url);

    let manager = RedisConnectionManager::new(redis_url).expect("Could not create redis manager");
    let pool = bb8::Pool::builder()
        .build(manager)
        .await
        .expect("Could not create redis pool");

    info!(log, "Starting server on port 8080");

    HttpServer::new(move || {
        let client = reqwest::ClientBuilder::new()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/109.0.0.0 Safari/537.36")
            .build()
            .expect("Could not build reqwest client");

        let semaphore = Arc::new(Semaphore::new(20));

        App::new()
            .service(index)
            .wrap_api()
            .wrap(Cors::permissive())
            .app_data(web::Data::new(AppState { client, semaphore, redis_pool: pool.clone(), log: log.clone()}))
            .service(web::resource("/v1/player/{name}-{numbers}").route(web::get().to(get_battletag)))
            .with_json_spec_at("/api/spec/v2")
            .with_swagger_ui_at("/docs")
            .build()
    })
    .bind(("0.0.0.0", 8080))?
    .run()
    .await
}
