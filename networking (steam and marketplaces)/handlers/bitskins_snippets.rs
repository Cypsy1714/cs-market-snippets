// bitskins.rs
//
// This module provides logic for BitSkins marketplace operations,
// building on top of the API layer to handle price discovery, purchase workflows,
// and item withdrawal processes.

use super::{api::bitskins_api, steam};
use crate::{
    data,
    structs::{
        ItemData, ItemSaleStats, ItemStatus, ItemStatusChangeTicket, ItemStatusChanges, Market,
        Price,
    },
};
use chrono::{Duration, Local, NaiveDate};
use serde::Deserialize;
use serde_json::Value;
use tokio::time::sleep;

/// BitSkins inventory item structure for parsing API responses
#[allow(dead_code)]
#[derive(Deserialize, Clone, Debug)]
struct InventoryEntryResult {
    id: String,
    tradehold: i32,
}

/// Structure for parsing active trade data
#[allow(dead_code)]
#[derive(Deserialize, Clone, Debug)]
struct ActiveTradesEntryResult {
    tradeofferid: String,
}

/// Structure for parsing item data from BitSkins market
#[allow(dead_code)]
#[derive(Deserialize, Clone, Debug)]
struct ItemEntryResult {
    id: String,
    asset_id: String,
    skin_id: i64,
    price: i64,
    name: String,
    tradehold: i64,
}

/// Structure for parsing price history statistics
#[allow(dead_code)]
#[derive(Deserialize, Clone, Debug)]
struct ItemStatResult {
    date: String,
    price_min: i64,
    counter: i64,
}

/// Helper function to determine if a date is within the last 7 days
fn in_the_week(date: &str) -> bool {
    // Parse the input date string
    let input_date = NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap();

    // Get the current date
    #[allow(deprecated)]
    let today = Local::today().naive_local();

    // Calculate the date 7 days ago
    let seven_days_ago = today - Duration::days(7);

    // Check if the input date is more recent than 7 days ago
    input_date > seven_days_ago
}

/// Retrieves current market prices for a specific CS item with trade hold filtering
///
/// - Identifies lowest prices based on trade hold duration
/// - Calculates buy/sell prices with marketplace commissions
/// - Handles special item categories
pub async fn get_item_price(
    market_hash_name: String,
    sale_stats_current: Option<Option<ItemSaleStats>>,
) -> Result<Price, String> {
    // Send the API request to search for the item
    let res = bitskins_api::get_item_price(market_hash_name.to_string(), 7)
        .await
        .map_err(|e| format!(
            "bitskins.rs | get_item_price(market_hash_name={}, sale_stats_current={:?}) | Error occured when sending the api request. E: {:?}",
            market_hash_name, sale_stats_current, e
        ))?;

    let parsed_data: serde_json::Value = res.json()
        .await
        .map_err(|e| format!(
            "bitskins.rs | get_item_price(market_hash_name={}, sale_stats_current={:?}) | Error occured when parsing the api request. E: {:?}",
            market_hash_name, sale_stats_current, e
        ))?;

    // Parse search results into structured data
    let item_data: Vec<ItemEntryResult> = serde_json::from_value(parsed_data["list"].clone())
        .map_err(|e| format!(
            "bitskins.rs | get_item_price(market_hash_name={}, sale_stats_current={:?}) | Error occured when parsing the api request to data structre. E: {:?}",
            market_hash_name, sale_stats_current, e
        ))?;

    // Ensure we found matching items
    if item_data.is_empty() {
        return Err(format!(
            "bitskins.rs | get_item_price(market_hash_name={}, sale_stats_current={:?}) | Error occured while the returned item price data vector is empty.",
            market_hash_name, sale_stats_current
        ));
    }

    // Process pricing data with trade hold categories
    let mut price_now = None;
    let mut price_2 = 0.0;
    let mut price_4 = 0.0;
    let mut price_7 = 0.0;

    for item in item_data.iter() {
        // Check if the name matches exactly
        if item.name == market_hash_name && price_now.is_none() {
            // Get the price and categorize by trade hold duration
            let price = item.price as f32 / 1000.0;
            if item.tradehold > 4 {
                price_7 = price;
            } else if item.tradehold > 2 {
                price_4 = price;
            } else if item.tradehold >= 1 {
                price_2 = price;
            } else {
                price_now = Some(price);
                // Fill in missing price categories with the current price
                if price_7 == 0.0 {
                    price_7 = price;
                }
                if price_4 == 0.0 {
                    price_4 = price;
                }
                if price_2 == 0.0 {
                    price_2 = price;
                }
            }
        }
    }

    // Ensure we found a current price
    if price_now.is_none() {
        return Err(format!(
            "bitskins.rs | get_item_price(market_hash_name={}, sale_stats_current={:?}) | Error occured price could not be fetched. Found item name: {:?}",
            market_hash_name, sale_stats_current, item_data[0].name
        ));
    }

    // Calculate prices with BitSkins commission rates
    let comms_ = data::get_market_commisions(Market::BitSkins, "");
    if let Err(_comms_err) = comms_ {
        return Err(format!(
            "bitskins.rs | get_item_price(market_hash_name={}, sale_stats_current={:?}) | Error occured when trying to get the commisions of the market.", 
            market_hash_name, sale_stats_current
        ));
    }

    let price = price_now.unwrap();
    let comms = comms_.unwrap();
    
    // Calculate effective buy and sell prices with commissions
    let price_buy_w_comm: f32 = ((price / ((100 - comms.0) as f32 / 100.0)) * 100.0).ceil() / 100.0;
    let price_buy_2_w_comm: f32 = ((price_2 / ((100 - comms.0) as f32 / 100.0)) * 100.0).ceil() / 100.0;
    let price_buy_4_w_comm: f32 = ((price_4 / ((100 - comms.0) as f32 / 100.0)) * 100.0).ceil() / 100.0;
    let price_buy_7_w_comm: f32 = ((price_7 / ((100 - comms.0) as f32 / 100.0)) * 100.0).ceil() / 100.0;

    let price_sell_w_comm_: f32 = price * (1.0 - ((comms.1 + comms.2) as f32 / 100.0));
    let price_sell_w_comm: f32 = (price_sell_w_comm_ * 100.0).ceil() / 100.0;

    // Create and return the Price structure with all calculated values
    let res = Price {
        market: Market::BitSkins,
        commision: 4,
        price_buy: price,
        price_buy_trade: (price_7, price_4, price_2),
        price_buy_w_comm,
        price_sell_w_comm,
        price_buy_trade_w_comm: (price_buy_7_w_comm, price_buy_4_w_comm, price_buy_2_w_comm),
        price_sell: price,
        sale_stats: None,
    };
    
    Ok(res)
}

/// Retrieves historical sales statistics for an item
///
/// - Calculates weekly and monthly sales volume
/// - Determines price trends
/// - Computes weighted average prices
pub async fn get_item_sale_stats(skin_id: &str) -> Result<ItemSaleStats, String> {
    // Retrieve historical sales data
    let res = bitskins_api::get_sale_stats(skin_id.to_string())
        .await
        .map_err(|e| format!(
            "bitskins.rs | get_item_sale_stats(skin_id={}) | Error occured when sending the api request. E: {:?}",
            skin_id, e
        ))?;

    let parsed_data: serde_json::Value = res.json()
        .await
        .map_err(|e| format!(
            "bitskins.rs | get_item_sale_stats(skin_id={}) | Error occured when parsing the api request. E: {:?}",
            skin_id, e
        ))?;

    // Parse the historical data into structured format
    let item_data: Vec<ItemStatResult> = serde_json::from_value(parsed_data.clone())
        .map_err(|e| format!(
            "bitskins.rs | get_item_sale_stats(skin_id={}) | Error occured when parsing the api request to data structre. E: {:?}.\nParsed Data: {:?}",
            skin_id, e, parsed_data
        ))?;
    
    // Filter data for weekly analysis
    let mut weekly_data = item_data.clone();
    weekly_data.retain(|a| in_the_week(&a.date));

    // Calculate sales metrics
    let weekly_sales_count: f32 = weekly_data.iter().map(|a| a.counter as f32).sum::<f32>();
    let monthly_sales_count: f32 = item_data.iter().map(|a| a.counter as f32).sum::<f32>();
    
    // Calculate weighted average prices (price × quantity)
    let weekly_avg_price: f32 = if !weekly_data.is_empty() {
        weekly_data
            .iter()
            .map(|a| (a.price_min as f32 / 1000.0) * a.counter as f32)
            .sum::<f32>()
            / weekly_sales_count
    } else {
        0.0
    };
    
    // Apply commission to get effective sell price
    let weekly_avg_price_w_comm = (weekly_avg_price * 0.88 * 100.0).ceil() / 100.0;
    
    // Calculate monthly average for trend analysis
    let monthly_avg_price = if !item_data.is_empty() {
        item_data
            .iter()
            .map(|a| (a.price_min as f32 / 1000.0) * a.counter as f32)
            .sum::<f32>()
            / monthly_sales_count
    } else {
        0.0
    };

    // Calculate price trend (percentage change week over month)
    let one_week_price_diff_perc = if monthly_avg_price != 0.0 {
        ((weekly_avg_price / monthly_avg_price) - 1.0) * 100.0
    } else {
        0.0
    };

    // Create the sales statistics structure
    let res = ItemSaleStats {
        name: "".to_string(),
        weekly_avg_price: weekly_avg_price as f32,
        weekly_avg_price_w_comm: weekly_avg_price_w_comm as f32,
        monthly_avg_price: monthly_avg_price as f32,
        weekly_sale_count: weekly_sales_count as i32,
        monthly_sale_count: monthly_sales_count as i32,
        weekly_price_change: one_week_price_diff_perc as f32,
        projected_price_next_week: 0.0,
    };

    Ok(res)
}

/// Executes a buy operation for a specific item on BitSkins
///
/// - Finds the lowest priced matching item within constraints
/// - Executes the purchase transaction
/// - Initiates withdrawal to Steam inventory
pub async fn buy_item(
    market_hash_name: String,
    price: f32,
    trade_hold: i32,
) -> Result<(ItemStatusChangeTicket, (String, ItemData), f32), String> {
    // Search for matching items within price range and trade hold constraints
    let res = bitskins_api::get_item_price(market_hash_name.to_string(), trade_hold)
        .await
        .map_err(|e| format!(
            "bitskins.rs | buy_item(market_hash_name={}, price={:?}) | Error occured when sending the get_item_price api request. E: {:?}",
            market_hash_name, price, e
        ))?;

    let parsed_data: serde_json::Value = res.json()
        .await
        .map_err(|e| format!(
            "bitskins.rs | buy_item(market_hash_name={}, price={:?}) | Error occured when parsing the get_item_price api request. E: {:?}",
            market_hash_name, price, e
        ))?;

    // Parse it into structured data
    let item_data: Vec<ItemEntryResult> = serde_json::from_value(parsed_data["list"].clone())
        .map_err(|e| format!(
            "bitskins.rs | buy_item(market_hash_name={}, price={:?}) | Error occured when parsing the get_item_price api request to data structre. E: {:?}",
            market_hash_name, price, e
        ))?;

    // Ensure we found matching items
    if item_data.is_empty() {
        return Err(format!(
            "bitskins.rs | buy_item(market_hash_name={}, price={:?}) | Error occured while the returned item price data vector is empty.",
            market_hash_name, price
        ));
    }

    // Try to find and purchase an item within our constraints
    for item in item_data.iter() {
        // Check for name match and also price match
        let max_buy_price: i64 = (price * 1000.0) as i64;
        if item.name == market_hash_name && item.price < max_buy_price {
            // Execute purchase transaction
            let res_buy = bitskins_api::buy_item(item.id.clone(), item.price)
                .await
                .map_err(|e| format!(
                    "bitskins.rs | buy_item(market_hash_name={}, price={:?}) | Error occured when sending the buy_item api request. E: {:?}",
                    market_hash_name, price, e
                ))?;

            let parsed_buy_data: serde_json::Value = res_buy.json()
                .await
                .map_err(|e| format!(
                    "bitskins.rs | buy_item(market_hash_name={}, price={:?}) | Error occured when parsing the buy_item api request. E: {:?}",
                    market_hash_name, price, e
                ))?;

            let success_ = &parsed_buy_data["result"][0]["success"];

            if let Value::Bool(success) = success_ {
                if *success {
                    // Purchase successful - allow inventory to update
                    sleep(tokio::time::Duration::from_secs(2)).await;

                    // Create item tracking data
                    let new_item = ItemData {
                        asset_id: item.asset_id.clone(),
                        trade_offer_id: "".to_string(),
                        instance_id: "".to_string(),
                        class_id: "".to_string(),
                        market: Market::Steam,
                        status: ItemStatus::OnHold,
                        marketcsgo_item_id: "0".to_string(),
                        dmarket_item_id: "0".to_string(),
                        csmoney_item_id: "0".to_string(),
                        csfloat_offer_id: "0".to_string(),
                        timestamp_unix: None,
                    };
                    
                    // Create status change ticket for tracking
                    let ticket = ItemStatusChangeTicket {
                        csmoney_item_id: "0".to_string(),
                        marketcsgo_item_id: "0".to_string(),
                        dmarket_item_id: "0".to_string(),
                        csfloat_offer_id: "0".to_string(),
                        change: ItemStatusChanges::BuySuccessBitSkins,
                        asset_id: item.asset_id.clone(),
                    };
                    
                    // Calculate actual buy price
                    let buy_price = (item.price as f32 / 10.0).ceil() / 100.0;

                    // Initiate withdrawal to Steam inventory
                    let res_withdraw_ = bitskins_api::withdraw_item(item.id.clone()).await;

                    if let Ok(res_withdraw) = res_withdraw_ {
                        let parsed_withdraw_data_: Result<serde_json::Value, reqwest::Error> = res_withdraw.json().await;
                        if let Ok(parsed_withdraw_data) = parsed_withdraw_data_ {
                            let success_withdrawal_ = &parsed_withdraw_data[0]["success"];
                            if let Value::Bool(success_withdrawal) = success_withdrawal_ {
                                if *success_withdrawal {
                                    // Withdrawal successful, complete buy operation
                                    return Ok((ticket, (market_hash_name, new_item), buy_price));
                                }
                            }
                        }
                    }

                    // Withdrawal not confirmed but purchase succeeded
                    println!("Could not withdraw the item: {:?}", market_hash_name);
                    return Ok((ticket, (market_hash_name, new_item), buy_price));
                }
            }

            // Purchase API call was unsuccessful
            return Err(format!(
                "bitskins.rs | buy_item(market_hash_name={}, price={:?}) | Error occured, the buy_item api call was not successfull. Parsed buy data: {:?}", 
                market_hash_name, price, parsed_buy_data
            ));
        }
    }

    // No matching items found at the desired price
    Err(format!(
        "bitskins.rs | buy_item(market_hash_name={}, price={:?}) | Error occured, could not find the given item for the desired price.", 
        market_hash_name, price
    ))
}

/// Checks and processes pending buy operations and trade offers
///
/// - Identifies items ready for withdrawal from BitSkins
/// - Processes active Steam trade offers
/// - Ensures withdrawals complete successfully
pub async fn check_buy_operations() -> Result<(), String> {
    // Retrieve current inventory and active trades data
    let res_inv = bitskins_api::get_buy_inventory()
        .await
        .map_err(|e| format!(
            "bitskins.rs | check_buy_operations() | Error occured when sending the inventory api request. E: {:?}", 
            e
        ))?;
    
    let res_trades = bitskins_api::get_active_trades()
        .await
        .map_err(|e| format!(
            "bitskins.rs | check_buy_operations() | Error occured when sending the active_trades api request. E: {:?}", 
            e
        ))?;

    let parsed_inv_data: serde_json::Value = res_inv.json()
        .await
        .map_err(|e| format!(
            "bitskins.rs | check_buy_operations() | Error occured when parsing the inventory api request. E: {:?}", 
            e
        ))?;
    
    let parsed_trades_data: serde_json::Value = res_trades.json()
        .await
        .map_err(|e| format!(
            "bitskins.rs | check_buy_operations() | Error occured when parsing the trades api request. E: {:?}", 
            e
        ))?;

    // Parse structured data
    let inv_data: Vec<InventoryEntryResult> = serde_json::from_value(parsed_inv_data["list"].clone())
        .map_err(|e| format!(
            "bitskins.rs | check_buy_operations() | Error occured when parsing the inventory api request into the data structre. E: {:?}", 
            e
        ))?;
    
    let trades_data: Vec<ActiveTradesEntryResult> = serde_json::from_value(parsed_trades_data["list"].clone())
        .map_err(|e| format!(
            "bitskins.rs | check_buy_operations() | Error occured when parsing the trades api request into the data structre. E: {:?}", 
            e
        ))?;

    // Process inventory items with no trade hold
    for item in inv_data {
        if item.tradehold == 0 {
            // Initiate withdrawal for items ready to trade
            let _ = bitskins_api::withdraw_item(item.id).await;
        }
    }

    // Process active trade offers
    for trade in trades_data {
        // Accept each active trade offer
        let res = steam::accept_trade_offer(trade.tradeofferid.clone()).await;
        if let Err(err_str) = res {
            println!("bitskins.rs | check_buy_operations() | Error occured when accepting the trade offer. E: {:?}", err_str);
        }
    }

    Ok(())
}
