//! Wire types of the Rysk V12 API, as served by `v12.rysk.finance`.
//!
//! These mirror the upstream JSON and stay crate-private; [`super::model`] holds
//! the normalized shapes the tools return.

use std::collections::BTreeMap;

use serde::Deserialize;

/// `GET /api/assets`: assets keyed by chain id.
pub(crate) type Assets = BTreeMap<String, Vec<RawAsset>>;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RawAsset {
    pub underlying: String,
    pub decimals: u32,
    /// 18-decimal fixed point, independent of the token's decimals.
    pub min_trade_size: String,
    pub max_trade_size: String,
    pub symbol: String,
    pub chain_id: u64,
    pub address: String,
    pub active: bool,
    /// USD price, 18-decimal fixed point.
    pub price: String,
}

/// One entry of `GET /api/products`: an asset that can be sold as a call or put
/// against a collateral token.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RawProduct {
    pub active: bool,
    pub asset_address: String,
    #[serde(rename = "chainId")]
    pub chain_id: u64,
    pub is_put: bool,
    /// Stablecoin the strike and premium are denominated in.
    pub usd: String,
    pub collateral: String,
}

/// `GET /api/inventory`: indicative strikes per underlying symbol.
pub(crate) type Inventory = BTreeMap<String, RawUnderlying>;

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct RawUnderlying {
    #[serde(default)]
    pub expiries: Option<Vec<i64>>,
    /// Keyed by `"<strike>-<expiry>"`.
    #[serde(default)]
    pub combinations: Option<BTreeMap<String, RawCombination>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RawCombination {
    #[serde(rename = "expiration_timestamp")]
    pub expiration_timestamp: i64,
    pub time_to_expiry_days: f64,
    pub strike: f64,
    pub is_put: bool,
    pub delta: f64,
    pub bid: f64,
    pub ask: f64,
    pub index: f64,
    /// Indicative annualized yield in percent.
    pub apy: f64,
    pub timestamp: i64,
    #[serde(default)]
    pub products: Vec<RawComboProduct>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RawComboProduct {
    pub asset: String,
    pub collateral_asset: String,
}

/// A JSON-RPC answer from the taker websocket.
#[derive(Debug, Deserialize)]
pub(crate) struct RpcResponse<T> {
    pub result: Option<T>,
    pub error: Option<RpcError>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RpcError {
    pub code: i64,
    pub message: String,
}

/// One entry of the `positions` RPC result.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RawPosition {
    /// The option's asset (e.g. WHYPE), not the collateral.
    pub address: String,
    pub apr: Option<String>,
    pub chain_id: u64,
    pub created_at: i64,
    pub expiry: i64,
    pub is_put: bool,
    /// 18-decimal fixed point, as are `strike`, `price` and `premium`.
    pub quantity: String,
    pub strike: String,
    pub price: String,
    pub premium: String,
    pub tx_hash: Option<String>,
    pub usd: String,
    pub collateral: String,
    pub status: String,
    #[serde(default)]
    pub symbol: String,
    /// Base units of `payout_asset`.
    #[serde(default)]
    pub payout: Option<String>,
    #[serde(default)]
    pub payout_asset: Option<String>,
    /// Base units of `collateral`.
    #[serde(default)]
    pub settlement_collateral_payout: Option<String>,
}

/// The `request` RPC result: the best maker quote for an RFQ.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RawQuote {
    pub expiry: i64,
    pub maker: String,
    /// Premium per contract, 18-decimal fixed point in `usd` (or `premium_asset`).
    pub price: String,
    pub quantity: String,
    /// 8-decimal fixed point.
    pub strike: String,
    pub usd: String,
    pub collateral_asset: String,
    pub valid_until: i64,
    #[serde(default)]
    pub premium_asset: Option<String>,
    pub apy: Option<f64>,
}

/// One value of `GET /api/expiry-prices/{chain}/{asset}`: an 8-decimal price,
/// `"0"` while the expiry is unsettled. The web app also accepts an object form.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(crate) enum RawExpiryPrice {
    Plain(String),
    Detailed {
        price: Option<String>,
        #[serde(default)]
        verified: bool,
    },
}
