extern crate core;

use actix_web::{error, get, web, Responder, Result};
use num_derive::FromPrimitive;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_repr::Serialize_repr;
use std::time::Instant;

#[derive(Deserialize)]
struct Battletag {
    name: String,
    discriminator: u32,
}

#[derive(Serialize, Debug)]
enum Role {
    Tank,
    Damage,
    Support,
}

#[derive(Serialize, Debug)]
enum Tier {
    Bronze,
    Silver,
    Gold,
    Platinum,
    Diamond,
    Master,
    Grandmaster,
}

#[derive(Serialize_repr, FromPrimitive)]
#[repr(u8)]
enum TierNumber {
    One = 1,
    Two,
    Three,
    Four,
    Five,
}

#[derive(Serialize)]
struct Rank {
    role: Role,
    tier: Tier,
    tier_number: TierNumber,
}

#[derive(Serialize)]
struct Player {
    battletag: String,
    name: String,
    discriminator: u32,
    ranks: Vec<Rank>,
}

/// extract path info using serde
#[get("/player/{name}/{discriminator}")]
async fn get_battletag(info: web::Path<Battletag>) -> Result<impl Responder> {
    let start = Instant::now();
    let res = reqwest::get(format!(
        "https://overwatch.blizzard.com/en-us/career/{}-{}",
        info.name, info.discriminator
    ))
    .await
    .map_err(|e| error::ErrorBadRequest(format!("{e}")))?;

    println!("Request took {:?}", start.elapsed());

    if !res.status().is_success() {
        return Err(error::ErrorBadRequest("Could not get player"));
    }

    let body = res
        .text()
        .await
        .map_err(|_| error::ErrorBadRequest("Could not parse body"))?;

    println!("Body took {:?}", start.elapsed());

    let document = scraper::Html::parse_document(&body);

    let role_wrapper_selector = scraper::Selector::parse(".Profile-playerSummary--roleWrapper")
        .map_err(|_| error::ErrorBadRequest("Profile does not have any placed roles"))?;

    let role_selector = scraper::Selector::parse(".Profile-playerSummary--role>img")
        .map_err(|_| error::ErrorBadRequest("Profile does not have any placed roles"))?;
    let tier_selector = scraper::Selector::parse(".Profile-playerSummary--rank")
        .map_err(|_| error::ErrorBadRequest("Profile does not have any placed roles"))?;

    let mut ranks: Vec<Rank> = Vec::new();
    let rank_container = document.select(&role_wrapper_selector);

    for rank in rank_container {
        let role_url = Url::parse(
            rank.select(&role_selector)
                .next()
                .ok_or_else(|| error::ErrorBadRequest("Could not parse Role URL"))?
                .value()
                .attr("src")
                .ok_or_else(|| error::ErrorBadRequest("Could not parse Role URL"))?,
        )
        .map_err(|_| error::ErrorBadRequest("Could not parse Role URL"))?;
        let tier_url = Url::parse(
            rank.select(&tier_selector)
                .next()
                .ok_or_else(|| error::ErrorBadRequest("Could not parse Tier URL"))?
                .value()
                .attr("src")
                .ok_or_else(|| error::ErrorBadRequest("Could not parse Tier URL"))?,
        )
        .map_err(|_| error::ErrorBadRequest("Could not parse Tier URL"))?;

        let role_file_name = role_url
            .path_segments()
            .ok_or_else(|| error::ErrorBadRequest("Could not get Role name"))?
            .last()
            .ok_or_else(|| error::ErrorBadRequest("Could not get Role name"))?;
        let tier_file_name = tier_url
            .path_segments()
            .ok_or_else(|| error::ErrorBadRequest("Could not get Tier name"))?
            .last()
            .ok_or_else(|| error::ErrorBadRequest("Could not get Role name"))?;

        let role = match role_file_name {
            "tank-f64702b684.svg" => Role::Tank,
            "offense-ab1756f419.svg" => Role::Damage,
            "support-0258e13d85.svg" => Role::Support,
            _ => return Err(error::ErrorBadRequest("Invalid role")),
        };

        let mut tier_parts = tier_file_name.splitn(3, '-');
        let tier_name = match tier_parts
            .next()
            .ok_or_else(|| error::ErrorBadRequest("Could not get Tier name"))?
        {
            "BronzeTier" => Tier::Bronze,
            "SilverTier" => Tier::Silver,
            "GoldTier" => Tier::Gold,
            "PlatinumTier" => Tier::Platinum,
            "DiamondTier" => Tier::Diamond,
            "MasterTier" => Tier::Master,
            "GrandmasterTier" => Tier::Grandmaster,
            _ => return Err(error::ErrorBadRequest("Invalid rank")),
        };

        let tier_number: TierNumber = num::FromPrimitive::from_u8(
            tier_parts
                .next()
                .ok_or_else(|| error::ErrorBadRequest("Could not get Tier number"))?
                .parse::<u8>()
                .map_err(|_| error::ErrorBadRequest("Could not get Tier name"))?,
        )
        .ok_or_else(|| error::ErrorBadRequest("Could not get Tier name"))?;
        ranks.push(Rank {
            role,
            tier: tier_name,
            tier_number,
        });
    }

    println!("Parsing took {:?}", start.elapsed());

    Ok(web::Json(Player {
        battletag: format!("{}#{}", info.name, info.discriminator),
        name: info.name.clone(),
        discriminator: info.discriminator,
        ranks,
    }))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    use actix_web::{App, HttpServer};

    HttpServer::new(|| App::new().service(get_battletag))
        .bind(("127.0.0.1", 8080))?
        .run()
        .await
}
