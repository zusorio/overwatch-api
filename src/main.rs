extern crate core;

use actix_web::{error, get, web, Error, HttpResponse, Responder, Result};
use num_derive::FromPrimitive;
use reqwest::{StatusCode, Url};
use scraper::error::SelectorErrorKind;
use scraper::Html;
use serde::{Deserialize, Serialize};
use serde_repr::Serialize_repr;
use std::time::Instant;

struct AppState {
    client: reqwest::Client,
}

#[derive(Serialize, Deserialize)]
struct Battletag {
    name: String,
    discriminator: u32,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
enum Role {
    Tank,
    Damage,
    Support,
}

#[derive(Serialize, Debug, Clone)]
enum Tier {
    Bronze,
    Silver,
    Gold,
    Platinum,
    Diamond,
    Master,
    Grandmaster,
}

#[derive(Serialize_repr, FromPrimitive, Clone)]
#[repr(u8)]
enum TierNumber {
    One = 1,
    Two,
    Three,
    Four,
    Five,
}

#[derive(Serialize, Clone)]
struct Rank {
    #[serde(skip_serializing)]
    role: Role,
    tier: Tier,
    tier_number: TierNumber,
}

#[derive(Serialize)]
struct Player {
    name: String,
    battletag: Battletag,
    private: bool,
    profile_picture: Option<String>,
    title: Option<String>,
    endorsement: u8,
    tank: Option<Rank>,
    damage: Option<Rank>,
    support: Option<Rank>,
}

fn parsing_error(_: SelectorErrorKind) -> Error {
    error::ErrorInternalServerError("Could not parse page")
}

fn parsing_error_empty() -> Error {
    error::ErrorInternalServerError("Could not parse page")
}

fn extract_profile_picture(document: &Html) -> Result<Option<String>> {
    let profile_picture_selector =
        scraper::Selector::parse(".Profile-player--portrait").map_err(parsing_error)?;

    Ok(document
        .select(&profile_picture_selector)
        .next()
        .ok_or_else(parsing_error_empty)?
        .value()
        .attr("src")
        .map_or_else(
            || None,
            |s| {
                if s.is_empty() {
                    None
                } else {
                    Some(s.to_string())
                }
            },
        ))
}

fn extract_title(document: &Html) -> Result<Option<String>> {
    let title_selector =
        scraper::Selector::parse(".Profile-player--title").map_err(parsing_error)?;
    let title_element = document.select(&title_selector).next();

    let title_inner_text = title_element.map(|e| e.inner_html());
    Ok(title_inner_text.map_or_else(
        || None,
        |s| {
            if s.is_empty() || s == "No Title" {
                None
            } else {
                Some(s)
            }
        },
    ))
}

fn extract_endorsement(document: &Html) -> Result<u8> {
    let endorsement_selector =
        scraper::Selector::parse(".Profile-playerSummary--endorsement").map_err(parsing_error)?;

    let endorsement_url = Url::parse(
        document
            .select(&endorsement_selector)
            .next()
            .ok_or_else(parsing_error_empty)?
            .value()
            .attr("src")
            .ok_or_else(parsing_error_empty)?,
    )
    .map_err(|_| error::ErrorBadRequest("Could not parse endorsement URL"))?;

    let endorsement_file_name = endorsement_url
        .path_segments()
        .ok_or_else(parsing_error_empty)?
        .last()
        .ok_or_else(parsing_error_empty)?;

    let endorsement = match endorsement_file_name {
        "1-9de6d43ec5.svg" => 1,
        "2-8b9f0faa25.svg" => 2,
        "3-8ccb5f0aef.svg" => 3,
        "4-48261e1164.svg" => 4,
        "5-8697f241ca.svg" => 5,
        _ => {
            return Err(error::ErrorInternalServerError(
                "Found invalid endorsement level while parsing",
            ))
        }
    };

    Ok(endorsement)
}

fn extract_roles(document: &Html) -> Result<Vec<Rank>> {
    let role_wrapper_selector =
        scraper::Selector::parse(".Profile-playerSummary--roleWrapper").map_err(parsing_error)?;

    let role_selector =
        scraper::Selector::parse(".Profile-playerSummary--role>img").map_err(parsing_error)?;
    let tier_selector =
        scraper::Selector::parse(".Profile-playerSummary--rank").map_err(parsing_error)?;
    let mut ranks: Vec<Rank> = Vec::new();
    let rank_container = document.select(&role_wrapper_selector);

    for rank in rank_container {
        let role_url = Url::parse(
            rank.select(&role_selector)
                .next()
                .ok_or_else(parsing_error_empty)?
                .value()
                .attr("src")
                .ok_or_else(parsing_error_empty)?,
        )
        .map_err(|_| error::ErrorBadRequest("Could not parse Role URL"))?;
        let tier_url = Url::parse(
            rank.select(&tier_selector)
                .next()
                .ok_or_else(parsing_error_empty)?
                .value()
                .attr("src")
                .ok_or_else(parsing_error_empty)?,
        )
        .map_err(|_| error::ErrorBadRequest("Could not parse Tier URL"))?;

        let role_file_name = role_url
            .path_segments()
            .ok_or_else(parsing_error_empty)?
            .last()
            .ok_or_else(parsing_error_empty)?;
        let tier_file_name = tier_url
            .path_segments()
            .ok_or_else(parsing_error_empty)?
            .last()
            .ok_or_else(parsing_error_empty)?;

        let role = match role_file_name {
            "tank-f64702b684.svg" => Role::Tank,
            "offense-ab1756f419.svg" => Role::Damage,
            "support-0258e13d85.svg" => Role::Support,
            _ => {
                return Err(error::ErrorInternalServerError(
                    "Found invalid role while parsing",
                ))
            }
        };

        let mut tier_parts = tier_file_name.splitn(3, '-');
        let tier_name = match tier_parts.next().ok_or_else(parsing_error_empty)? {
            "BronzeTier" => Tier::Bronze,
            "SilverTier" => Tier::Silver,
            "GoldTier" => Tier::Gold,
            "PlatinumTier" => Tier::Platinum,
            "DiamondTier" => Tier::Diamond,
            "MasterTier" => Tier::Master,
            "GrandmasterTier" => Tier::Grandmaster,
            _ => {
                return Err(error::ErrorInternalServerError(
                    "Found invalid tier while parsing",
                ))
            }
        };

        let tier_number: TierNumber = num::FromPrimitive::from_u8(
            tier_parts
                .next()
                .ok_or_else(parsing_error_empty)?
                .parse::<u8>()
                .map_err(|_| {
                    error::ErrorInternalServerError("Found invalid tier number while parsing")
                })?,
        )
        .ok_or_else(parsing_error_empty)?;
        ranks.push(Rank {
            role,
            tier: tier_name,
            tier_number,
        });
    }
    Ok(ranks)
}

/// extract path info using serde
#[get("/player/{name}-{discriminator}")]
async fn get_battletag(
    info: web::Path<Battletag>,
    data: web::Data<AppState>,
) -> Result<impl Responder> {
    let start = Instant::now();
    let res = data
        .client
        .get(format!(
            "https://overwatch.blizzard.com/en-us/career/{}-{}",
            info.name, info.discriminator
        ))
        .send()
        .await
        .map_err(|e| error::ErrorInternalServerError(format!("{e}")))?;

    dbg!("Request took {:?}", start.elapsed());

    if res.status() == StatusCode::NOT_FOUND {
        return Err(error::ErrorNotFound("Player not found"));
    } else if !res.status().is_success() {
        return Err(error::ErrorInternalServerError("Failed getting player"));
    }

    let body = res
        .text()
        .await
        .map_err(|_| error::ErrorBadRequest("Could not parse body"))?;

    dbg!("Body took {:?}", start.elapsed());

    let document = Html::parse_document(&body);

    let private_profile_selector =
        scraper::Selector::parse(".Profile-player--private").map_err(parsing_error)?;

    let private_profile = document.select(&private_profile_selector).next().is_some();

    let profile_picture = extract_profile_picture(&document)?;
    let title = extract_title(&document)?;
    let endorsement = extract_endorsement(&document)?;
    let ranks = extract_roles(&document)?;

    dbg!("Parsing took {:?}", start.elapsed());

    Ok(web::Json(Player {
        name: format!("{}#{}", info.name, info.discriminator),
        battletag: Battletag {
            name: info.name.clone(),
            discriminator: info.discriminator,
        },
        private: private_profile,
        profile_picture,
        title,
        endorsement,
        tank: ranks.iter().find(|r| r.role == Role::Tank).cloned(),
        damage: ranks.iter().find(|r| r.role == Role::Damage).cloned(),
        support: ranks.iter().find(|r| r.role == Role::Support).cloned(),
    }))
}

#[get("/")]
async fn index() -> impl Responder {
    HttpResponse::Ok().body(include_str!("../static/index.html"))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    use actix_web::{App, HttpServer};

    let client = reqwest::ClientBuilder::new()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/109.0.0.0 Safari/537.36")
        .build()
        .expect("Could not build reqwest client");

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(AppState {
                client: client.clone(),
            }))
            .service(index)
            .service(get_battletag)
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
