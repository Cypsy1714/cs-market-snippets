// steam_api.rs
//
// This module provides a robust client implementation for interacting with Steam's API,
// handling authentication flows, inventory management, and trade operations.
// It demonstrates advanced HTTP client implementation with proper error handling,
// authentication management, and response validation.

use crate::statics::{
    self, get_marketcsgo_access_token, get_steam_cookie, get_steam_session_id, get_steam_web_api,
};
use async_std::{fs::OpenOptions, io::WriteExt};
use reqwest::{
    self,
    header::{HeaderMap, CONTENT_TYPE, COOKIE, REFERER},
};
use serde::{Deserialize, Serialize};
use std::{i128, time::SystemTime};

/// Data structure for creating trade offers
#[derive(Debug, Serialize, Deserialize)]
struct TradeOfferData {
    sessionid: String,
    serverid: i32,
    partner: String,
    tradeoffermessage: String,
    trade_offer_create_params: String,
    json_tradeoffer: String,
    captcha: String,
}

/// Data structure for accepting trade offers
#[derive(Debug, Serialize, Deserialize)]
struct TradeOfferAcceptData {
    sessionid: String,
    serverid: i32,
    tradeofferid: String,
    captcha: String,
}

/// Retrieves detailed information about a specific trade offer
/// 
/// This function demonstrates API key authentication and proper
/// request parameter handling with Steam's API
pub async fn get_trade_offer(tradeofferid: String) -> Result<reqwest::Response, reqwest::Error> {
    let url = "https://api.steampowered.com/IEconService/GetTradeOffer/v1/";

    let access_token = get_marketcsgo_access_token().unwrap_or("0".to_string());
    let web_api = get_steam_web_api().unwrap_or("0".to_string());

    let client = reqwest::Client::new();
    client
        .get(url)
        .timeout(std::time::Duration::from_secs(30))
        .query(&[
            ("key", &web_api),
            ("access_token", &access_token),
            ("tradeofferid", &tradeofferid),
        ])
        .send()
        .await
}

/// Fetches a user's CS:GO inventory with proper authentication
/// 
/// This function shows handling of Steam's cookie-based authentication
/// and includes performance logging to track API response times
pub async fn get_inventory(user_id: String, last_asset: &str) -> Result<reqwest::Response, String> {
    // Start the timer and open the log file
    let start = SystemTime::now();
    let mut log = OpenOptions::new()
        .append(true)
        .open("api_log.txt")
        .await
        .expect("Cannot open api_log.txt file.");

    let asset_str = if last_asset == "" {
        "".to_string()
    } else {
        format!("&start_assetid={}", last_asset)
    };

    let url = format!(
        "https://steamcommunity.com/inventory/{}/730/2?l=english&count=1000",
        user_id
    ) + &asset_str;

    let cookie_ = get_steam_cookie();
    if let Err(statics_err) = cookie_ {
        return Err(statics_err);
    }
    let mut cookie = cookie_.unwrap();
    cookie = cookie.trim().to_string();

    // Create the headers
    let mut headers = HeaderMap::new();
    headers.insert(COOKIE, cookie.parse().unwrap());
    
    let session_id_ = get_steam_session_id();
    if let Err(statics_err) = session_id_ {
        return Err(statics_err);
    }

    let client = reqwest::Client::new();
    let body = client
        .get(url)
        .timeout(std::time::Duration::from_secs(30))
        .headers(headers)
        .send()
        .await;

    // After the request has been sent log the interaction
    let after = SystemTime::now();
    let passed = after.duration_since(start).unwrap();
    let log_txt = format!(
        "steam_api | get_inventory(user_id: {}, last_asset: {}) | The HTTP request took {:?}.\n",
        user_id, last_asset, passed
    );
    log.write(log_txt.as_bytes())
        .await
        .expect("Cannot write to api_log.txt file.");

    if let Err(body_err) = body {
        return Err(format!("{:?}", body_err));
    }
    Ok(body.unwrap())
}

/// Sends a trade offer to another Steam user
///
/// This function demonstrates complex form submission with proper headers,
/// handling of session authentication, and Steam's trading API integration
pub async fn send_trade_offer(
    partner_id: &str,
    partner_token: &str,
    trade_offer_message: &str,
    items: &str,
) -> Result<reqwest::Response, String> {
    let steam_id: i128 = partner_id.parse::<i128>().unwrap() + 76561197960265728;
    let url = "https://steamcommunity.com/tradeoffer/new/send";

    // Create the headers
    let mut headers = HeaderMap::new();

    headers.insert(
        REFERER,
        format!(
            "https://steamcommunity.com/tradeoffer/new/?partner={}&token={}",
            partner_id, partner_token
        )
        .parse()
        .unwrap(),
    );

    headers.insert(
        CONTENT_TYPE,
        "application/x-www-form-urlencoded; charset=UTF-8"
            .parse()
            .unwrap(),
    );

    let cookie_ = get_steam_cookie();
    if let Err(statics_err) = cookie_ {
        return Err(statics_err);
    }
    let cookie = cookie_.unwrap();

    headers.insert(COOKIE, cookie.parse().unwrap());
    let session_id_ = get_steam_session_id();
    if let Err(statics_err) = session_id_ {
        return Err(statics_err);
    }
    let session_id = session_id_.unwrap();

    // Create the body (json)
    let body_obj = TradeOfferData {
        sessionid: session_id,
        serverid: 1,
        partner: steam_id.to_string(),
        tradeoffermessage: trade_offer_message.to_string(),
        trade_offer_create_params: format!("{{\"trade_offer_access_token\": \"{}\"}}", partner_token),
        json_tradeoffer: format!("{{\"newversion\": true, \"version\": 2, \"me\": {{\"assets\":{}, \"currency\": [], \"ready\": false}}, \"them\": {{\"assets\":[], \"currency\": [], \"ready\": false}}}}", items),
        captcha: "".to_string(),
    };

    let data = serde_urlencoded::to_string(&body_obj).expect("serialize issue");

    let client = reqwest::Client::new();

    let response = client
        .post(url)
        .timeout(std::time::Duration::from_secs(30))
        .headers(headers)
        .body(data)
        .send()
        .await;

    if let Ok(body) = response {
        Ok(body)
    } else {
        Err(format!(
            "Error occured when sending the request: {:?}",
            response.unwrap_err()
        ))
    }
}

/// Accepts an incoming trade offer
///
/// This function shows how to handle Steam's trade acceptance flow,
/// demonstrating session management and proper HTTP header configuration
pub async fn accept_trade_offer(
    trade_offer_id: &str,
) -> Result<reqwest::Response, String> {
    let url = format!(
        "https://steamcommunity.com/tradeoffer/{}/accept",
        trade_offer_id
    );

    // Create the headers
    let mut headers = HeaderMap::new();

    headers.insert(
        REFERER,
        format!("https://steamcommunity.com/tradeoffer/{}", trade_offer_id)
            .parse()
            .unwrap(),
    );

    headers.insert(
        CONTENT_TYPE,
        "application/x-www-form-urlencoded; charset=UTF-8"
            .parse()
            .unwrap(),
    );

    let cookie_ = get_steam_cookie();
    if let Err(statics_err) = cookie_ {
        return Err(statics_err);
    }
    let cookie = cookie_.unwrap();

    headers.insert(COOKIE, cookie.parse().unwrap());
    let session_id_ = get_steam_session_id();
    if let Err(statics_err) = session_id_ {
        return Err(statics_err);
    }
    let session_id = session_id_.unwrap();

    // Create the body (json)
    let body_obj = TradeOfferAcceptData {
        sessionid: session_id,
        serverid: 1,
        tradeofferid: trade_offer_id.to_string(),
        captcha: "".to_string(),
    };

    let data = serde_urlencoded::to_string(&body_obj).expect("serialize issue");

    let client = reqwest::Client::new();

    let response = client
        .post(url)
        .timeout(std::time::Duration::from_secs(30))
        .headers(headers)
        .body(data)
        .send()
        .await;

    if let Ok(body) = response {
        Ok(body)
    } else {
        Err(format!(
            "Error occured when sending the request: {:?}",
            response.unwrap_err()
        ))
    }
}
