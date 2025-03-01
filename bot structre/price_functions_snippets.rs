use crate::data;
use crate::log_functions;
use crate::structs::{Item, Market, Price, PriceCompare};
use std::collections::HashMap;

/// Compares prices across all markets to identify arbitrage opportunities
/// Returns a hashmap with market pairs as keys and profitable items as values
pub async fn price_compare_all(
    map: &HashMap<String, Item>,
) -> HashMap<(Market, Market), Vec<PriceCompare>> {
    let mut res: HashMap<(Market, Market), Vec<PriceCompare>> = HashMap::new();
    
    // Go through all the items in the Inventory hashmap
    for (_key, value) in map {
        let mut start_i = 0;
        // Loop through all the price entry combinations
        while start_i < value.price.len() {
            for i in (start_i + 1)..value.price.len() {
                let mut price_1 = value.price[start_i].clone();
                let mut price_2 = value.price[i].clone();
                let mut reversed = 0;

                while reversed < 2 {
                    // First take the price_1 as the buy market and the price_2 as the sell
                    let diff_perc_before_comm: i32 =
                        ((price_2.price_sell - price_1.price_buy) / price_1.price_buy * 100.0)
                            as i32;

                    let diff_perc_after_comm: i32 = ((price_2.price_sell
                        - (price_2.price_sell * price_2.commision as f32 / 100.0)
                        - price_1.price_buy)
                        / price_1.price_buy
                        * 100.0) as i32;

                    let diff_val_before_comm: f32 = price_2.price_sell - price_1.price_buy;

                    let diff_val_after_comm: f32 = (price_2.price_sell
                        - (price_2.price_sell * price_2.commision as f32 / 100.0))
                        - price_1.price_buy;

                    // Enter the value to the hashmap
                    let entry = res.get_mut(&(price_1.market.clone(), price_2.market.clone()));
                    if let Some(val) = entry {
                        val.push(PriceCompare {
                            name: value.name.clone(),
                            diff_perc_before_comm,
                            diff_perc_after_comm,
                            diff_val_before_comm,
                            diff_val_after_comm,
                            price: (price_1.clone(), price_2.clone()),
                        });
                    } else {
                        res.entry((price_1.market.clone(), price_2.market.clone()))
                            .or_insert(
                                [PriceCompare {
                                    name: value.name.clone(),
                                    diff_perc_before_comm,
                                    diff_perc_after_comm,
                                    diff_val_before_comm,
                                    diff_val_after_comm,
                                    price: (price_1.clone(), price_2.clone()),
                                }]
                                .to_vec(),
                            );
                    }

                    // Swap the markets and calculate again
                    let price_temp = price_1.clone();
                    price_1 = price_2.clone();
                    price_2 = price_temp.clone();
                    reversed += 1;
                }
            }
            start_i += 1;
        }
    }

    res
}

/// Finds the most profitable trade between markets for a given item
/// Returns (buy market, sell market, profit percentage, trade hold days)
pub async fn most_profitable(prices: Vec<Price>, item_hash_name: String) -> (Market, Market, f32, i32) {
    let buy_markets = vec![Market::DMarket, Market::BitSkins, Market::CSFloat, Market::LisSkins, Market::CSMoney];
    let sell_markets = vec![Market::MarketCSGO];
    let mut res = (Market::DMarket, Market::MarketCSGO, 0.0, 0);
    
    // Trade hold premium multipliers
    let trade_hold_2_extra = 1.02;
    let trade_hold_4_extra = 1.04;
    let trade_hold_7_extra = 1.07;

    // Search for the buy_markets
    for buy_market in &buy_markets {
        // Get the price of the buy_market
        for buy_price in &prices {
            if buy_price.market == *buy_market {
                // Search for the sell_markets
                for sell_market in &sell_markets {
                    // Get the price of the sell_market
                    for sell_price in &prices {
                        if sell_price.market == *sell_market {
                            // Check if sales data exists
                            let sales_data = sell_price.sale_stats.clone();

                            if sales_data.is_none() {
                                log_functions::log_err(&format!("No sales data found in the sell market. Item: {:?}, Sell Price: {:?}", item_hash_name, sell_price));
                            } else {
                                // Calculate prices accounting for trade hold periods
                                let current_buy = buy_price.price_buy_w_comm;
                                let trade_hold_2_price = buy_price.price_buy_trade_w_comm.2 * trade_hold_2_extra;
                                let trade_hold_4_price = buy_price.price_buy_trade_w_comm.1 * trade_hold_4_extra;
                                let trade_hold_7_price = buy_price.price_buy_trade_w_comm.0 * trade_hold_7_extra;

                                // Find best price considering all trade hold periods
                                let buy_price_best = f32::min(
                                    f32::min(
                                        f32::min(current_buy, trade_hold_2_price),
                                        trade_hold_4_price
                                    ), 
                                    trade_hold_7_price
                                );

                                // Determine which trade hold period yielded the best price
                                let trade_hold_duration = match buy_price_best {
                                    _ if buy_price_best == current_buy => 0,
                                    _ if buy_price_best == trade_hold_2_price => 2,
                                    _ if buy_price_best == trade_hold_4_price => 4,
                                    _ if buy_price_best == trade_hold_7_price => 7,
                                    _ => 0,
                                };

                                // Calculate profit percentage
                                let profit_perc = ((sales_data.unwrap().weekly_avg_price_w_comm / buy_price_best) - 1.0) * 100.0; 

                                // Update if better than current best
                                if profit_perc > res.2 {
                                    res = (buy_market.clone(), sell_market.clone(), profit_perc, trade_hold_duration);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    res
}

/// Calculates the maximum price to pay when buying an item to ensure target profit margin
pub fn max_buy_price(avg_sell_price_w_comm: f32, buy_market: Market, minimum_profit_margin: f32) -> f32 {
    let commisions_ = data::get_market_commisions(buy_market.clone(), "");

    if let Err(comms_err) = commisions_ {
        log_functions::log_err(&format!("Cannot get the commisions. E: {:?}", comms_err));
        return 0.0;
    }

    let commisions = commisions_.unwrap();
    
    // Adjust decimal precision based on market
    let decimal = if buy_market == Market::MarketCSGO {1000.0} else {100.0};
    
    // Calculate maximum buy price that still guarantees minimum profit margin
    let max_buy_price = avg_sell_price_w_comm / (1.0 + ((minimum_profit_margin) / 100.0));
    
    // Adjust for buying commission and round to appropriate decimal precision
    ((max_buy_price - (max_buy_price * (commisions.0 as f32 / 100.0))) * decimal).ceil() / decimal 
}
