use actix_web::error::{ErrorBadRequest, ErrorInternalServerError};
use scraper::Html;

use crate::{Group, Rank, Role};
use url::Url;

pub fn extract_portrait(document: &Html) -> actix_web::Result<Option<String>> {
    let portrait_selector = scraper::Selector::parse(".Profile-player--portrait")
        .map_err(|_| crate::parsing_error())?;

    Ok(document
        .select(&portrait_selector)
        .next()
        .ok_or_else(crate::parsing_error)?
        .value()
        .attr("src")
        .map_or_else(
            || None,
            |s| {
                if s.is_empty() {
                    None
                } else {
                    Some(s.to_owned()) // Better semantics imo
                }
            },
        ))
}

pub fn extract_title(document: &Html) -> actix_web::Result<Option<String>> {
    let title_selector =
        scraper::Selector::parse(".Profile-player--title").map_err(|_| crate::parsing_error())?;
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

fn url_file_name(url: &Url) -> actix_web::Result<&str> {
    url.path_segments()
        .and_then(Iterator::last)
        .ok_or_else(crate::parsing_error)
}

pub fn extract_endorsement(document: &Html) -> actix_web::Result<u8> {
    let endorsement_selector = scraper::Selector::parse(".Profile-playerSummary--endorsement")
        .map_err(|_| crate::parsing_error())?;

    let endorsement_url = Url::parse(
        document
            .select(&endorsement_selector)
            .next()
            .ok_or_else(crate::parsing_error)?
            .value()
            .attr("src")
            .ok_or_else(crate::parsing_error)?,
    )
    .map_err(|_| ErrorBadRequest("Could not parse endorsement URL"))?;

    let endorsement = match url_file_name(&endorsement_url)? {
        "1-9de6d43ec5.svg" => 1,
        "2-8b9f0faa25.svg" => 2,
        "3-8ccb5f0aef.svg" => 3,
        "4-48261e1164.svg" => 4,
        "5-8697f241ca.svg" => 5,
        _ => {
            return Err(ErrorInternalServerError(
                "Found invalid endorsement level while parsing",
            ))
        }
    };

    Ok(endorsement)
}

pub enum Platform {
    Pc,
    Console,
}

pub fn extract_roles(
    document: &Html,
    region: Platform,
) -> actix_web::Result<(Option<Rank>, Option<Rank>, Option<Rank>)> {
    let role_wrapper_selector = match region {
        Platform::Pc => {
            scraper::Selector::parse(".mouseKeyboard-view>.Profile-playerSummary--roleWrapper")
                .map_err(|e| ErrorInternalServerError(e.to_string()))?
        }
        Platform::Console => {
            scraper::Selector::parse(".controller-view>.Profile-playerSummary--roleWrapper")
                .map_err(|e| ErrorInternalServerError(e.to_string()))?
        }
    };

    let role_selector = match region {
        Platform::Pc => scraper::Selector::parse(".Profile-playerSummary--role>img")
            .map_err(|e| ErrorInternalServerError(e.to_string()))?,
        Platform::Console => scraper::Selector::parse(".Profile-playerSummary--role>use")
            .map_err(|e| ErrorInternalServerError(e.to_string()))?,
    };

    let tier_selector = scraper::Selector::parse(".Profile-playerSummary--rank")
        .map_err(|_| crate::parsing_error())?;

    let mut tank: Option<Rank> = None;
    let mut damage: Option<Rank> = None;
    let mut support: Option<Rank> = None;

    let rank_container = document.select(&role_wrapper_selector);

    for rank in rank_container {
        let role_url = Url::parse(
            rank.select(&role_selector)
                .next()
                .ok_or_else(|| ErrorInternalServerError("Could not find role url element"))?
                .value()
                .attrs
                .values()
                .next()
                .ok_or_else(|| {
                    ErrorInternalServerError("Could not find role url element src tag")
                })?,
        )
        .map_err(|_| ErrorBadRequest("Could not parse Role URL"))?;
        let tier_url = Url::parse(
            rank.select(&tier_selector)
                .next()
                .ok_or_else(|| ErrorInternalServerError("Could not find tier url element"))?
                .value()
                .attr("src")
                .ok_or_else(|| {
                    ErrorInternalServerError("Could not find tier url element src tag")
                })?,
        )
        .map_err(|_| ErrorBadRequest("Could not parse Tier URL"))?;

        let role = match url_file_name(&role_url)? {
            "tank-f64702b684.svg" => Role::Tank,
            "offense-ab1756f419.svg" => Role::Damage,
            "support-0258e13d85.svg" => Role::Support,
            _ => return Err(ErrorInternalServerError("Found invalid role while parsing")),
        };

        let mut tier_parts = url_file_name(&tier_url)?.split('-');
        let group = match tier_parts.next().ok_or_else(crate::parsing_error)? {
            "BronzeTier" => Group::Bronze,
            "SilverTier" => Group::Silver,
            "GoldTier" => Group::Gold,
            "PlatinumTier" => Group::Platinum,
            "DiamondTier" => Group::Diamond,
            "MasterTier" => Group::Master,
            "GrandmasterTier" => Group::Grandmaster,
            _ => return Err(ErrorInternalServerError("Found invalid tier while parsing")),
        };

        let tier = tier_parts
            .next()
            .ok_or_else(crate::parsing_error)?
            .parse::<u8>()
            .map_err(|_| ErrorInternalServerError("Found invalid tier number while parsing"))?;

        match role {
            Role::Tank => tank = Some(Rank { group, tier }),
            Role::Damage => damage = Some(Rank { group, tier }),
            Role::Support => support = Some(Rank { group, tier }),
        }
    }
    Ok((tank, damage, support))
}
