// bitskins_api.rs
//
// This module provides a robust client implementation for interacting with BitSkins' marketplace API,
// handling authentication, item searching, pricing, buying, and withdrawal operations.
// It demonstrates advanced marketplace integration with proxy rotation, retry logic, and
// thorough error handling for reliable trading operations.

use crate::{data, log_functions::log_write, structs::Market};
use async_std::task::sleep;
use chrono::{Duration, Local};
use rand::Rng;
use reqwest::{
    header::{self, HeaderMap},
    Client, ClientBuilder, Proxy,
};
use std::time::SystemTime;

static P_KEY: &str = "XXX";
static SCRAPE_KEYS: [&str; 4] = [
    "XXX",
    "XXX",
    "XXX",
    "XXX",
];

/// Rotates between multiple API keys to avoid rate limiting
fn get_scrape_key() -> String {
    let mut rng = rand::thread_rng();
    let random_number = rng.gen_range(0..=3);
    SCRAPE_KEYS[random_number].to_string()
}

/// Advanced request handler with proxy support, timeout, and automatic retries
/// 
/// This function demonstrates techniques for building reliable marketplace integration:
/// - Proxy rotation to avoid IP-based rate limiting
/// - Timeout handling to prevent hung connections
/// - Automatic retry logic for transient failures
async fn send_request_with_proxy_and_timeout_and_retry(
    url: &str,
    proxy_url: &str,
    headers: HeaderMap,
    body: String,
    username: &str,
    password: &str,
    timeout_secs: u64,
    max_retries: usize,
) -> Result<reqwest::Response, reqwest::Error> {
    let proxy = Proxy::all(proxy_url)
        .unwrap()
        .basic_auth(username, password);
    let client = Client::builder()
        .proxy(proxy)
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()?;

    let mut attempts = 0;

    loop {
        attempts += 1;
        match client
            .post(url)
            .headers(headers.clone())
            .body(body.clone())
            .send()
            .await
        {
            Ok(response) => {
                return Ok(response);
            }
            Err(_) if attempts <= max_retries => {
                sleep(std::time::Duration::from_secs(1)).await; // Wait before retry
            }
            Err(e) => return Err(e),
        }
    }
}

/// Searches for a specific CS item on BitSkins marketplace
/// 
/// This function demonstrates knowledge and integration of BitSkins API and CS item categorization:
/// - Handles special categories (StatTrak™, Souvenir)
/// - Filters by trade hold duration for faster arbitrage
/// - Sorts by lowest price for efficient market analysis
pub async fn get_item_price(
    market_hash_name: String,
    max_trade_hold: i32,
) -> Result<reqwest::Response, reqwest::Error> {
    // Start the timer for performance logging
    let start = SystemTime::now();

    // Determine correct category ID based on CS:GO item naming conventions
    let mut category = "1";
    if market_hash_name.contains("StatTrak") {
        category = "3";
    }
    if market_hash_name.contains("Souvenir") {
        category = "5";
    }

    // Build search query with appropriate filters
    let url = "https://api.bitskins.com/market/search/730";
    let json_str = format!(
        r#"{{"order":[{{"field":"price","order":"ASC"}}],"offset":0,"limit":30,"where":{{"skin_name":"{}","tradehold_to":{},"price_from":10,"price_to":25000000,"category_id":[{}]}}}}"#,
        market_hash_name, max_trade_hold, category
    );

    // Set up request headers
    let mut header = reqwest::header::HeaderMap::new();
    header.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_str("application/json").unwrap(),
    );

    // Send request through proxy system to avoid rate limiting
    let proxy_data = data::get_proxy(Market::BitSkins);
    let body = send_request_with_proxy_and_timeout_and_retry(
        url,
        &proxy_data.0,
        header.clone(),
        json_str.clone(),
        &proxy_data.1,
        &proxy_data.2,
        15,
        0,
    )
    .await;

    // Log performance data
    let after = SystemTime::now();
    let passed = after.duration_since(start).unwrap();
    let log_txt = format!(
        "bitskins_api | get_item_price(market_hash_name: {}) | The HTTP request took {:?}.\n",
        market_hash_name, passed
    );
    log_write(&log_txt);
    body
}

/// Retrieves 30-day price history for a specific CS item
/// 
/// - Fetches historical data for trend analysis
/// - Uses proper date formatting for API compatibility
/// - Implements key rotation for higher throughput
pub async fn get_sale_stats(skin_id: String) -> Result<reqwest::Response, reqwest::Error> {
    let start = SystemTime::now();

    // Calculate 30-day date range for historical data
    let now = Local::now();
    let one_month_ago = now - Duration::days(30);
    let formatted_date_now = now.format("%Y-%m-%d").to_string();
    let formatted_date_ago = one_month_ago.format("%Y-%m-%d").to_string();

    // Build pricing history query
    let url = "https://api.bitskins.com/market/pricing/summary";
    let json_str = format!(
        r#"{{"app_id":730,"skin_id":{},"date_from":"{}","date_to":"{}"}}"#,
        skin_id, formatted_date_ago, formatted_date_now
    );

    // Set up headers with API key rotation
    let auth_token = get_scrape_key();
    let mut header = reqwest::header::HeaderMap::new();
    header.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_str("application/json").unwrap(),
    );
    header.insert(
        "x-apikey",
        header::HeaderValue::from_str(&auth_token).unwrap(),
    );

    // Send request via proxy with retry capability
    let proxy_data = data::get_bitskins_proxy();
    let body = send_request_with_proxy_and_timeout_and_retry(
        url,
        &proxy_data.0,
        header.clone(),
        json_str.clone(),
        &proxy_data.1,
        &proxy_data.2,
        10,
        2,
    )
    .await;

    // Log performance data
    let after = SystemTime::now();
    let passed = after.duration_since(start).unwrap();
    let log_txt = format!(
        "bitskins_api | get_sale_stats(skin_id: {}) | The HTTP request took {:?}.\n",
        skin_id, passed
    );
    log_write(&log_txt);
    body
}

/// Purchases a CS item from BitSkins marketplace
/// 
/// - Properly formatted purchase request
/// - Maximum price specification to prevent price manipulation
/// - Direct API key authentication for secure transactions
pub async fn buy_item(item_id: String, price: i64) -> Result<reqwest::Response, reqwest::Error> {
    let start = SystemTime::now();

    // Build purchase request payload
    let url = "https://api.bitskins.com/market/buy/many";
    let json_str = format!(
        r#"{{"app_id":730,"items":[{{"id":"{}","max_price":{}}}]}}"#,
        item_id, price
    );

    // Set up headers with API key for authenticated transaction
    let mut header = reqwest::header::HeaderMap::new();
    header.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_str("application/json").unwrap(),
    );
    header.insert("x-apikey", header::HeaderValue::from_str(P_KEY).unwrap());

    // Send purchase request
    let client = ClientBuilder::new().build()?;
    let body = client
        .post(url)
        .timeout(std::time::Duration::from_secs(30))
        .headers(header)
        .body(json_str)
        .send()
        .await;

    // Log transaction attempt
    let after = SystemTime::now();
    let passed = after.duration_since(start).unwrap();
    let log_txt = format!(
        "bitskins_api | buy_item(item_id={}, price={}) | The HTTP request took {:?}.\n",
        item_id, price, passed
    );
    log_write(&log_txt);
    body
}

/// Withdraws a purchased item to Steam inventory
/// 
/// - Initiates withdrawal process to player inventory
/// - Maintains proper API authorization
/// - Enables cross-marketplace arbitrage completion
pub async fn withdraw_item(item_id: String) -> Result<reqwest::Response, reqwest::Error> {
    let start = SystemTime::now();

    // Build withdrawal request
    let url = "https://api.bitskins.com/market/withdraw/many";
    let json_str = format!(r#"{{"items":[{{"app_id":730,"id":"{}"}}]}}"#, item_id);

    // Set up authenticated headers
    let mut header = reqwest::header::HeaderMap::new();
    header.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_str("application/json").unwrap(),
    );
    header.insert("x-apikey", header::HeaderValue::from_str(P_KEY).unwrap());

    // Send withdrawal request
    let client = ClientBuilder::new().build()?;
    let body = client
        .post(url)
        .timeout(std::time::Duration::from_secs(30))
        .headers(header)
        .body(json_str)
        .send()
        .await;

    // Log withdrawal attempt
    let after = SystemTime::now();
    let passed = after.duration_since(start).unwrap();
    let log_txt = format!(
        "bitskins_api | withdraw_item(item_id={}) | The HTTP request took {:?}.\n",
        item_id, passed
    );
    log_write(&log_txt);
    body
}

/// Retrieves currently owned items on BitSkins
/// 
/// - Filters by trade hold status for arbitrage planning
/// - Properly handles authentication for protected inventory access
/// - Supports complete item lifecycle management
pub async fn get_buy_inventory() -> Result<reqwest::Response, reqwest::Error> {
    let start = SystemTime::now();

    // Build inventory query with trade hold filter
    let url = "https://api.bitskins.com/market/search/mine/730";
    let json_str = r#"{"offset":0,"where":{"tradehold_to":0},"where_mine":{"status":[4,0]},"limit":100,"order":[{"field":"bumped_at","order":"DESC"}]}"#.to_string();

    // Set up authenticated headers
    let mut header = reqwest::header::HeaderMap::new();
    header.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_str("application/json").unwrap(),
    );
    header.insert("x-apikey", header::HeaderValue::from_str(P_KEY).unwrap());

    // Send inventory request
    let client = ClientBuilder::new().build()?;
    let body = client
        .post(url)
        .timeout(std::time::Duration::from_secs(30))
        .headers(header)
        .body(json_str)
        .send()
        .await;

    // Log request
    let after = SystemTime::now();
    let passed = after.duration_since(start).unwrap();
    let log_txt = format!(
        "bitskins_api | get_buy_inventory() | The HTTP request took {:?}.\n",
        passed
    );
    log_write(&log_txt);
    body
}

/// Monitors active Steam trade offers for BitSkins withdrawals
/// 
/// - Monitors active offers to track withdrawal status
/// - Ensures trades are completing successfully
/// - Provides data for automated trade management
pub async fn get_active_trades() -> Result<reqwest::Response, reqwest::Error> {
    let start = SystemTime::now();

    // Build trade status query
    let url = "https://api.bitskins.com/steam/trade/active";
    let json_str = r#"{"limit":5}"#.to_string();

    // Set up authenticated headers
    let mut header = reqwest::header::HeaderMap::new();
    header.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_str("application/json").unwrap(),
    );
    header.insert("x-apikey", header::HeaderValue::from_str(P_KEY).unwrap());

    // Send trade status request
    let client = ClientBuilder::new().build()?;
    let body = client
        .post(url)
        .timeout(std::time::Duration::from_secs(30))
        .headers(header)
        .body(json_str)
        .send()
        .await;

    // Log request
    let after = SystemTime::now();
    let passed = after.duration_since(start).unwrap();
    let log_txt = format!(
        "bitskins_api | get_active_trades() | The HTTP request took {:?}.\n",
        passed
    );
    log_write(&log_txt);
    body
}
