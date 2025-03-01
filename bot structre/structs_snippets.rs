use std::time::SystemTime;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

// The enum which differentiates the markets
#[derive(Debug, Clone, EnumIter, PartialEq, Hash, Eq, Serialize, Deserialize)]
pub enum Market {
    Steam,
    DMarket,
    MarketCSGO,
    Buff,
    CSMoney,
    CSFloat,
    BitSkins,
    LisSkins,
    WaxPeer,
}

// The struct for every item type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub name: String,
    pub count: ItemCount,
    pub data: Vec<ItemData>,
    pub price: Vec<Price>,
    pub history: Vec<ItemHistory>,
}

// The struct that exists in every Item, tracks inventory counts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemCount {
    pub total: i16,
    pub available: i16,
    pub on_offer: i16,
    pub on_hold: i16,
    pub max_count: i16,
}

// The struct that has all the ids about that instance of the item
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemData {
    pub trade_offer_id: String,
    pub asset_id: String,
    pub instance_id: String,
    pub class_id: String,
    pub market: Market,
    pub status: ItemStatus,
    pub marketcsgo_item_id: String,
    pub dmarket_item_id: String,
    pub csmoney_item_id: String,
    pub csfloat_offer_id: String,
    pub timestamp_unix: Option<i64>,
}

// The struct that has all the item operation history
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemHistory {
    pub unix: i64,
    pub price: f32,
    pub bought_market: Market,
    pub min_sale_price: f32,
}

// The enum that contains all the possible states of an item
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ItemStatus {
    Available,
    OnSellOfferWaitingBuyer,
    OnSellOfferWaitingTradeOffer,
    OnSellOfferWaitingTrade,
    Sold,
    OnBuyOfferWaitingSeller,
    OnBuyOfferWaitingTradeOffer,
    OnBuyOfferWaitingTrade,
    Bought,
    BoughtLisSkins,
    Error,
    OnHold,
}

// The enum that contains all the possible state changes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ItemStatusChanges {
    Withdrawal,
    TradeLockDone,
    BuySuccessDmarket,
    BuyStartCSMoney,
    BuyStartCSFloat,
    BuyStartLisSkins,
    BuySuccessCSMoney,
    BuySuccessCSFloat,
    BuySuccessBitSkins,
    BuySuccessLisSkins(String),
    BuyFailure,
    SellOfferCreated(Market),
    SellOfferBought(Market),
    SellTradeCanceled,
    SellTradeSent(Market, i64),
    SellSuccess(Market, f32),
    SellError(i64),
}

// The struct that contains the data about the items status change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemStatusChangeTicket {
    pub dmarket_item_id: String,
    pub csmoney_item_id: String,
    pub marketcsgo_item_id: String,
    pub csfloat_offer_id: String,
    pub asset_id: String,
    pub change: ItemStatusChanges,
}

// The struct that has all the price data of an Item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Price {
    pub market: Market,
    pub commision: i32,
    pub price_buy_trade: (f32, f32, f32),
    pub price_buy_trade_w_comm: (f32, f32, f32),
    pub price_buy: f32,
    pub price_buy_w_comm: f32,
    pub price_sell: f32,
    pub price_sell_w_comm: f32,
    pub sale_stats: Option<ItemSaleStats>,
}

// The struct that has the data of an items price in two different markets
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceCompare {
    pub name: String,
    pub diff_perc_before_comm: i32,
    pub diff_perc_after_comm: i32,
    pub diff_val_before_comm: f32,
    pub diff_val_after_comm: f32,
    pub price: (Price, Price),
}

// The struct that contains all the sale stats of an item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemSaleStats {
    pub name: String,
    pub weekly_avg_price: f32,
    pub weekly_avg_price_w_comm: f32,
    pub weekly_sale_count: i32,
    pub monthly_avg_price: f32,
    pub monthly_sale_count: i32,
    pub weekly_price_change: f32,
    pub projected_price_next_week: f32,
}

// Declare the type structure of all the market functions
#[allow(async_fn_in_trait)]
pub trait MarketFunctions {
    async fn get_item_price(&self, market: &Market) -> Result<Price, String>;
    async fn get_all_prices(&mut self);
    async fn get_given_prices(&mut self, markets: Vec<Market>);
    fn get_min_sell_price(&self, market: Market, price: f32) -> f32;
    fn get_min_sell_price_auto(&self, profit_margin: f32, current_market: Option<Market>) -> (f32, Market);
    fn get_sell_market(&self, item: ItemData) -> (Option<Market>, f32, f32);
    fn get_sell_market_other(&self, item: ItemData, main_market: Market, main_sell_price: f32) -> Vec<(Option<Market>, f32, f32)>;
    async fn buy_item(&mut self, market: Market, price: f32, trade_hold: i32) -> Result<ItemStatusChangeTicket, String>;
    async fn check_buy_conditions_and_buy(&mut self, profit_margin: f32, iteration: i32) -> Result<ItemStatusChangeTicket, String>;
}

// Declare the type structure of ItemData functions
#[allow(async_fn_in_trait)]
pub trait ItemDataFunctions {
    async fn update_price(&self, market: Market, price: f32) -> Result<(), String>;
    async fn sell_item(&mut self, market: Market, price: f32) -> Result<ItemStatusChangeTicket, String>;
    async fn get_sell_price(&self, item_name: &str, market: Market, min_sell_price: f32, current_price: f32, sales_data: Option<ItemSaleStats>, bought_time_unix: i64) -> Option<f32>;
    async fn remove_sell(&self) -> Result<ItemStatusChangeTicket, String>;
    async fn remove_sell_no_error(&self, ignored_market: Market);
    fn get_unix(&mut self, item_name: String) -> Option<i64>;
}

// Get the UNIX timestamp
fn get_sys_time_in_secs() -> u64 {
    match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        Ok(n) => n.as_secs(),
        Err(_) => panic!("SystemTime before UNIX EPOCH!"),
    }
}
