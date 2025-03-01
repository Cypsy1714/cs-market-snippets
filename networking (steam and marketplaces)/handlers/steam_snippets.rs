// steam.rs
//
// This module provides logic for Steam operations,
// building on top of the API layer to handle inventory management, trade offers,
// and trade lock status tracking.

use std::collections::HashMap;

use crate::markets::api::steam_api;
use crate::structs::{ItemData, ItemCount, Item, ItemStatus, ItemStatusChangeTicket, ItemStatusChanges, Market};

use serde_json;
use serde_json::Value;
use serde::Deserialize;

// Items to ignore when processing inventory
const IGNORE: [&'static str; 5] = ["Loyalty Badge", "5 Year Veteran Coin", "Music Kit", "Graffiti |", "Global Offensive Badge"];

/// Structure for parsing trade offer data from Steam API
#[derive(Deserialize, Debug, Clone)]
#[allow(dead_code)]
struct SteamTradeOfferData {
    tradeofferid: String,
    items_to_receive: Vec<InventoryReturn>,
}

/// Structure for parsing inventory item data from Steam API
#[derive(Deserialize, Debug, Clone)]
#[allow(dead_code)]
struct InventoryReturn {
    amount: String,
    appid: i32,
    assetid: String,
    classid: String,
    contextid: String,
    instanceid: String,
}

/// Structure for parsing item description data from Steam API
#[derive(Deserialize, Debug)]
struct DescriptionsReturn {
    classid: String,
    instanceid: String,
    market_name: String,
    tradable: i32,
}

/// Internal structure for processing inventory data
#[derive(Debug)]
struct InventoryRequestReturn {
    total_count: i32,
    id_data: Vec<InventoryReturn>,
    names: Vec<String>,
    tradable: Vec<bool>,
}

/// Retrieves and processes a user's complete Steam CS:GO inventory
///
/// - Handles paginated inventory retrieval for large inventories
/// - Processes complex nested item data structures
/// - Properly categorizes items by trade status
pub async fn get_inventory(user_id: String) -> Result<HashMap<String, Item>, String> {
    let mut inv: HashMap<String, Item> = HashMap::new();
    let mut temp_data = InventoryRequestReturn {
        total_count: 100,
        id_data: Vec::new(),
        names: Vec::new(),
        tradable: Vec::new(),
    };
    let mut last_asset_id = "".to_string();
    let storage_container_amount = 12;

    // Retrieve inventory in batches until we have all items
    while temp_data.total_count - storage_container_amount > temp_data.names.len() as i32 {
        let res = get_inventory_request(user_id.clone(), &last_asset_id).await;
        if let Ok(mut data) = res {
            last_asset_id = data.id_data[data.id_data.len()-1].assetid.clone();
            temp_data.total_count = data.total_count;
            temp_data.id_data.append(&mut data.id_data);
            temp_data.names.append(&mut data.names);
            temp_data.tradable.append(&mut data.tradable);
        } else {
            let err_str = res.unwrap_err();
            return Err(format!("Fix steam cookie!!! {:?}", err_str));
        }
    }

    // Process inventory data into a structured format
    for i in 0..temp_data.names.len() {
        let item_name = &temp_data.names[i];
        let ids = &temp_data.id_data[i];
        let tradable = &temp_data.tradable[i];

        // Skip items we don't want to track
        let mut ignored = false;
        for ignore_str in IGNORE {
            if item_name.contains(ignore_str) {
                ignored = true;
            }
        }
        if ignored {
            continue;
        }
        
        // Add item to inventory with proper status
        let entry = inv.entry(item_name.to_string()).or_insert(Item{
            name: item_name.to_string(), 
            count: ItemCount{total: 0, available: 0, on_offer: 0, on_hold: 0, max_count: 0}, 
            data: Vec::new(), 
            price: Vec::new(), 
            history: Vec::new()
        });

        entry.count.total += 1;
        
        if *tradable {
            // Item is available for trading
            entry.count.available += 1;
            entry.data.push(ItemData{
                asset_id: ids.assetid.clone(), 
                instance_id: ids.instanceid.clone(), 
                class_id: ids.classid.clone(), 
                market: Market::Steam, 
                status: ItemStatus::Available, 
                marketcsgo_item_id: "".to_string(), 
                trade_offer_id: "0".to_string(), 
                dmarket_item_id: "0".to_string(), 
                csmoney_item_id: "0".to_string(), 
                csfloat_offer_id: "0".to_string(), 
                timestamp_unix: None
            });
        } else {
            // Item is on trade hold
            entry.count.on_hold += 1;
            entry.data.push(ItemData{
                asset_id: ids.assetid.clone(), 
                instance_id: ids.instanceid.clone(), 
                class_id: ids.classid.clone(), 
                market: Market::Steam, 
                status: ItemStatus::OnHold, 
                marketcsgo_item_id: "".to_string(), 
                trade_offer_id: "0".to_string(), 
                dmarket_item_id: "0".to_string(), 
                csmoney_item_id: "0".to_string(), 
                csfloat_offer_id: "0".to_string(), 
                timestamp_unix: None
            });
        }
    }

    Ok(inv)
}

/// Checks for items that have completed their trade hold period
///
/// - Identifies newly tradable items and changes their status
/// - Creates status change tickets for trading system to process
pub async fn check_trade_lock(user_id: String) -> Result<Vec<ItemStatusChangeTicket>, String> {
    let mut tickets_vec: Vec<ItemStatusChangeTicket> = Vec::new();
    let mut temp_data = InventoryRequestReturn {
        total_count: 100,
        id_data: Vec::new(),
        names: Vec::new(),
        tradable: Vec::new(),
    };
    let mut last_asset_id = "".to_string();
    let storage_container_amount = 12;

    // Retrieve complete inventory in batches
    while temp_data.total_count - storage_container_amount > temp_data.names.len() as i32 {
        let res = get_inventory_request(user_id.clone(), &last_asset_id).await;
        if let Ok(mut data) = res {
            last_asset_id = data.id_data[data.id_data.len()-1].assetid.clone();
            temp_data.total_count = data.total_count;
            temp_data.id_data.append(&mut data.id_data);
            temp_data.names.append(&mut data.names);
            temp_data.tradable.append(&mut data.tradable);
        } else {
            let err_str = res.unwrap_err();
            if err_str.contains("Error occured while trying to parse the response body.") {
                // The inventory has reached the end
                break;
            } 
            return Err(err_str);
        }
    }

    // Find tradable items and create status change tickets
    for i in 0..temp_data.names.len() {
        let item_name = &temp_data.names[i];
        let ids = &temp_data.id_data[i];
        let tradable = &temp_data.tradable[i];

        // Skip non-tradable items
        let mut ignored = false;
        for ignore_str in IGNORE {
            if item_name.contains(ignore_str) {
                ignored = true;
            }
        }
        if ignored {
            continue;
        }
               
        // Create status change tickets for tradable items
        if *tradable {
            let entry = ItemStatusChangeTicket{
                asset_id: ids.assetid.clone(),
                csfloat_offer_id: "0".to_string(), 
                marketcsgo_item_id: "0".to_string(), 
                dmarket_item_id: "0".to_string(), 
                csmoney_item_id: "0".to_string(), 
                change: ItemStatusChanges::TradeLockDone
            };
            tickets_vec.push(entry);
        }
    }

    Ok(tickets_vec)
}

/// Internal function to handle inventory data retrieval and parsing
async fn get_inventory_request(user_id: String, last_asset_id: &str) -> Result<InventoryRequestReturn, String> {
    let res = steam_api::get_inventory(user_id.clone(), last_asset_id).await;

    // A hashmap that contains the classid as the key and the item name as the value
    let mut name_map: HashMap<(String, String), (String, i32)> = HashMap::new();
    
    // The Result
    let mut result: InventoryRequestReturn = InventoryRequestReturn{
        total_count: 0,
        id_data: Vec::new(),
        names: Vec::new(),
        tradable: Vec::new(),
    };

    match res {
        Ok(val) => {
            let parsed_data: Result<serde_json::Value, reqwest::Error> = val.json().await;

            match parsed_data {
                Ok(json) => {
                    // Get the data from the json
                    let total_count = &json["total_inventory_count"];
                    let assets = &json["assets"];
                    let descriptions = &json["descriptions"];

                    // Break and return error if somehow the json is empty
                    if assets == &Value::Null || total_count == &Value::Null || descriptions == &Value::Null {
                        // Assume that we reached the end 
                        Err(format!("steam.rs | get_inventory() | user_id = {} | Error occured while trying to parse the response body.", &user_id))
                    } else {
                        // Process the data if everything checks out
                        let des_res: Vec<DescriptionsReturn> = serde_json::from_value(descriptions.clone()).unwrap();
                        let inv_res: Vec<InventoryReturn> = serde_json::from_value(assets.clone()).unwrap(); 

                        // Map all the names for classids
                        for i in 0..des_res.len() {
                            name_map.insert(
                                (des_res[i].classid.clone(), des_res[i].instanceid.clone()), 
                                (des_res[i].market_name.clone(), des_res[i].tradable.clone())
                            );
                        }

                        // Find the total count and write it to the result 
                        let total_c = &json["total_inventory_count"].as_i64();
                        if let Some(n) = total_c {
                            result.total_count = *n as i32;
                        } else {
                            return Err(format!("steam.rs | get_inventory() | user_id = {} | Error occured while trying to parse the response body. | Toal Count", &user_id));
                        } 

                        // Go through all the inv data and return
                        for i in 0..inv_res.len() {
                            let entry = &inv_res[i];
                            if let Some(s) = name_map.get(&(entry.classid.clone(), entry.instanceid.clone())) {
                                result.id_data.push(entry.clone());
                                result.names.push(s.0.to_string());
                                let tradable = if s.1 == 1 { true} else {false};
                                result.tradable.push(tradable);
                            } else {
                                return Err(format!("steam.rs | get_inventory() | user_id = {} | Error occured while trying to parse the response body. | name_map", &user_id));
                            }
                        }

                        return Ok(result);
                    }
                },
                Err(e) => {
                    Err(format!("steam.rs | get_inventory() | user_id = {} | Error occured while trying to parse the response body.| {}", user_id, e))
                }
            }
        }
        Err(e) => {
            Err(format!("steam.rs | get_inventory() | user_id = {} | Error occured while trying to get the inventory data.| {}", user_id, e))
        }
    }
}

/// Accepts a trade offer and retrieves the received item's asset ID
///
/// - Fetches trade offer details to identify incoming items
/// - Accepts the trade offer
/// - Returns the new asset ID for inventory tracking
pub async fn accept_trade_offer_get_asset_id(trade_offer_id: String) -> Result<String, String> {
    // Get trade offer details to identify the item being received
    let res = steam_api::get_trade_offer(trade_offer_id.clone())
        .await
        .map_err(|e| format!(
            "steam.rs | accept_trade_offer_get_asset_id(tradeofferid={}) | Error occured when getting the trade offer. | {:?}", 
            trade_offer_id.clone(), e
        ))?;

    let parsed_data: serde_json::Value = res.json()
        .await
        .map_err(|e| format!(
            "steam.rs | accept_trade_offer_get_asset_id(tradeofferid={}) | Error occured when parsing the data into json. | {:?}", 
            trade_offer_id.clone(), e
        ))?;

    let offer_data: SteamTradeOfferData = serde_json::from_value(parsed_data["response"]["offer"].clone())
        .map_err(|e| format!(
            "steam.rs | accept_trade_offer_get_asset_id(tradeofferid={}) | Error occured when parsing the data into the data structre. | {:?}", 
            trade_offer_id.clone(), e
        ))?;

    if offer_data.items_to_receive.is_empty() {
        return Err(format!(
            "steam.rs | accept_trade_offer_get_asset_id(tradeofferid={}) | Error occured while the items_to_receive array is empty.", 
            trade_offer_id.clone()
        ));
    }

    // Extract the asset ID of the item we'll receive
    let asset_id = offer_data.items_to_receive[0].assetid.clone();

    // Accept the trade offer
    accept_trade_offer(trade_offer_id.clone()).await?;

    Ok(asset_id)
}

/// Accepts a Steam trade offer
pub async fn accept_trade_offer(trade_offer_id: String) -> Result<(), String> {
    let res = steam_api::accept_trade_offer(&trade_offer_id)
        .await
        .map_err(|e| format!("Steam accept trade api error: {:?}", e))?;
    
    let status = res.status();

    if status == 403 {
        return Err("The steam authentication is not working.".to_string());
    }

    if status == 200 {
        return Ok(());
    } 

    Err(format!("The steam accept trade returned an error status: {}", status))
}

/// Retrieves the Steam Web API token needed for API operations
pub async fn get_webapi() -> Result<String, String> {
    let res = steam_api::get_steam_webapi()
        .await
        .map_err(|e| format!("steam.rs | get_webapi() | Error occured when sending the api request. E: {:?}", e))?;

    let parsed_data: serde_json::Value = res.json()
        .await
        .map_err(|e| format!("steam.rs | get_webapi() | Error occured while parsing the api request. E: {:?}", e))?;

    if let Value::String(webapi) = &parsed_data["data"]["webapi_token"] {
        return Ok(webapi.to_string());
    }
    
    Err("steam.rs | get_webapi() | The cookie is not valid to get the token.".to_string())
}
