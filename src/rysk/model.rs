//! Normalized shapes returned by [`super::RyskClient`]: amounts in human units,
//! token symbols instead of addresses where known, and UTC dates.

use serde::{Deserialize, Serialize};

/// Option side as seen by the seller: a covered call or a cash-secured put.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum OptionType {
    #[serde(alias = "Call", alias = "CALL", alias = "c", alias = "C")]
    Call,
    #[serde(alias = "Put", alias = "PUT", alias = "p", alias = "P")]
    Put,
}

impl OptionType {
    pub fn from_is_put(is_put: bool) -> Self {
        if is_put { Self::Put } else { Self::Call }
    }

    pub fn is_put(self) -> bool {
        self == Self::Put
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Call => "call",
            Self::Put => "put",
        }
    }
}

/// A tradable asset and the collateral tokens it can be sold against.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Asset {
    pub chain_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain: Option<&'static str>,
    pub symbol: String,
    pub underlying: String,
    pub address: String,
    pub decimals: u32,
    pub price_usd: Option<f64>,
    /// Smallest quantity accepted, in units of the asset. Quantities are multiples of it.
    pub min_trade_size: Option<f64>,
    pub max_trade_size: Option<f64>,
    pub active: bool,
    /// Collateral accepted when selling a covered call.
    pub call_collateral: Vec<String>,
    /// Collateral accepted when selling a cash-secured put.
    pub put_collateral: Vec<String>,
}

/// The indicative option chain of one underlying.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OptionChain {
    pub underlying: String,
    pub index_price: Option<f64>,
    /// When Rysk last refreshed these numbers, RFC 3339 UTC.
    pub as_of: Option<String>,
    pub options: Vec<OptionRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OptionRow {
    /// Expiry date, UTC.
    pub expiry: String,
    pub expiry_timestamp: i64,
    pub days_to_expiry: f64,
    pub option_type: OptionType,
    pub strike: f64,
    pub delta: f64,
    /// Indicative annualized yield for the seller, in percent.
    pub apy: f64,
    /// Per-contract bid/ask in USD, when Rysk publishes them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bid: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ask: Option<f64>,
    /// Asset/collateral pairs this strike can be sold with.
    pub products: Vec<ProductRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProductRef {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain_id: Option<u64>,
    pub asset: String,
    pub collateral: String,
}

/// The best maker quote for selling an option now.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Quote {
    pub asset: String,
    pub chain_id: u64,
    pub option_type: OptionType,
    pub strike: Option<f64>,
    pub expiry: String,
    pub expiry_timestamp: i64,
    pub quantity: Option<f64>,
    pub price_per_contract: Option<f64>,
    /// `price_per_contract × quantity`.
    pub total_premium: Option<f64>,
    pub premium_asset: String,
    /// Annualized yield in percent, as Rysk computes it.
    pub apy: Option<f64>,
    pub collateral: String,
    pub maker: String,
    /// The quote expires at this time, RFC 3339 UTC.
    pub valid_until: String,
}

/// An option position of a wallet.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Position {
    pub chain_id: u64,
    pub symbol: String,
    pub option_type: OptionType,
    pub status: String,
    pub strike: Option<f64>,
    pub expiry: String,
    pub expiry_timestamp: i64,
    pub quantity: Option<f64>,
    pub price_per_contract: Option<f64>,
    pub premium: Option<f64>,
    pub premium_asset: String,
    /// Annualized yield in percent at entry.
    pub apr: Option<f64>,
    pub collateral: String,
    pub created_at: String,
    /// For settled positions: whether the option finished in the money and the
    /// collateral was exchanged (`payout`) rather than returned.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exercised: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payout: Option<TokenAmount>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collateral_returned: Option<TokenAmount>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_url: Option<String>,
}

/// A token amount. `amount` is `None` when the token's decimals are unknown;
/// `base_units` always carries the exact on-chain value.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TokenAmount {
    pub asset: String,
    pub amount: Option<f64>,
    pub base_units: String,
}

/// Settlement prices of one asset, latest expiry first.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SettlementPrices {
    pub asset: String,
    pub chain_id: u64,
    pub prices: Vec<SettlementPrice>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SettlementPrice {
    pub expiry: String,
    pub expiry_timestamp: i64,
    /// USD settlement price; `None` until the expiry settles.
    pub price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verified: Option<bool>,
}
