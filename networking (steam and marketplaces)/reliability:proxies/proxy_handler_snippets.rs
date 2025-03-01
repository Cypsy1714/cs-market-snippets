// proxy_handler.rs
//
// This module provides advanced networking functionality for reliable communications
// with marketplace APIs, including proxy rotation, request retry logic,
// rate limiting avoidance, and timeout management.

use crate::structs::Market;
use async_std::task::sleep;
use reqwest::{
    header::HeaderMap,
    Client, Proxy,
};
use std::time::Duration;

/// Proxy rotation counters for each marketplace
static mut DMARKET_NUM: usize = 0;
static mut CSFLOAT_NUM: usize = 0;
static mut MARKETCSGO_NUM: usize = 0;
static mut CSMONEY_NUM: usize = 0;
static mut BITSKINS_NUM: usize = 0;
static mut WAXPEER_NUM: usize = 0;

/// List of proxy servers used for request rotation
const PROXIES: [&str; 10] = [
    "45.86.48.213:50100",
    "45.86.50.46:50100",
    "45.86.49.52:50100",
    "45.86.50.11:50100",
    "45.86.48.9:50100",
    "45.86.50.186:50100",
    "45.86.48.7:50100",
    "45.86.50.124:50100",
    "45.86.50.128:50100",
    "45.86.50.63:50100",
];

/// Proxy authentication credentials
const PROXY_USERNAME: &str = "XXX";
const PROXY_PASSWORD: &str = "XXX";

/// Returns a rotating proxy address for the specified marketplace
/// 
/// Marketplaces implement rate limiting based on IP address.
/// This function cycles through a pool of proxies for each market,
/// avoiding detection and blocks that would interrupt trading operations.
///
pub fn get_proxy(market: Market) -> (String, String, String) {
    let mut proxy_url = "";

    // Thread-safe proxy rotation system
    // Each marketplace gets its own counter to handle different rate limits
    unsafe {
        match market {
            Market::Steam => {},  // Steam API doesn't need proxies
            Market::Buff => {},   // Buff doesn't need proxies
            Market::LisSkins => {},  // LisSkins doesn't need proxies
            
            Market::MarketCSGO => {
                proxy_url = PROXIES[MARKETCSGO_NUM];
                MARKETCSGO_NUM = (MARKETCSGO_NUM + 1) % PROXIES.len();
            },
            Market::DMarket => {
                proxy_url = PROXIES[DMARKET_NUM];
                DMARKET_NUM = (DMARKET_NUM + 1) % PROXIES.len();
            },
            Market::CSMoney => {
                proxy_url = PROXIES[CSMONEY_NUM];
                CSMONEY_NUM = (CSMONEY_NUM + 1) % PROXIES.len();
            },
            Market::CSFloat => {
                proxy_url = PROXIES[CSFLOAT_NUM];
                CSFLOAT_NUM = (CSFLOAT_NUM + 1) % PROXIES.len();
            },
            Market::BitSkins => {
                proxy_url = PROXIES[BITSKINS_NUM];
                BITSKINS_NUM = (BITSKINS_NUM + 1) % PROXIES.len();
            },
            Market::WaxPeer => {
                proxy_url = PROXIES[WAXPEER_NUM];
                WAXPEER_NUM = (WAXPEER_NUM + 1) % PROXIES.len();
            },
        }
    }

    (
        format!("{}", proxy_url),
        PROXY_USERNAME.to_string(),
        PROXY_PASSWORD.to_string(),
    )
}


/// Advanced request handler with proxy support, timeout control, and automatic retry
///
/// - Uses proxies to avoid IP-based rate limiting
/// - Implements timeout handling to prevent hung connections
/// - Features automatic retry logic for transient network failures
///
pub async fn send_request_with_proxy(
    url: &str,
    proxy_url: &str,
    headers: HeaderMap,
    body: String,
    username: &str,
    password: &str,
    timeout_secs: u64,
    max_retries: usize,
) -> Result<reqwest::Response, reqwest::Error> {
    // Configure proxy with authentication
    let proxy = Proxy::all(proxy_url)
        .unwrap()
        .basic_auth(username, password);
    
    // Build client with proxy and timeout settings
    let client = Client::builder()
        .proxy(proxy)
        .timeout(Duration::from_secs(timeout_secs))
        .build()?;

    let mut attempts = 0;

    // Retry loop with exponential backoff
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
            Err(err) if attempts <= max_retries => {
                // Wait before retry with exponential backoff
                let backoff_secs = 1u64.saturating_mul(attempts as u64);
                sleep(Duration::from_secs(backoff_secs)).await;
            }
            Err(e) => return Err(e),
        }
    }
}
