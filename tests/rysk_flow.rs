//! End-to-end flow tests for the Rysk client over a fake transport — no network.
//! Fixtures are trimmed copies of real `v12.rysk.finance` responses.

use std::collections::HashMap;
use std::sync::Mutex;

use rysk_mcp::rysk::transport::{Transport, TransportError};
use rysk_mcp::rysk::{
    API_URL, ChainFilter, OptionType, PositionFilter, ProductRef, QuoteRequest, RyskClient,
    RyskError, TAKER_WS_URL, TokenAmount,
};
use serde_json::{Value, json};

const WHYPE: &str = "0x5555555555555555555555555555555555555555";
const USDC_HYPEREVM: &str = "0xb88339cb7199b77e23db6e890353e22632ba630f";
const USDT0: &str = "0xb8ce59fc3717ada4c02eadf9682a9e934f625ebb";
const WALLET: &str = "0x418e14e4d34add3c23d2be1c11e6b02e628ae6dd";

const ASSETS: &str = r#"{
  "1": [{"underlying":"ETH","underlyingAssetAddress":"0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2","decimals":18,
         "minTradeSize":"1000000000000000000","maxTradeSize":"100000000000000000000","symbol":"WETH","chainId":1,
         "address":"0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2","active":true,"price":"2125396357110000000000","multipliers":[]}],
  "8453": [{"underlying":"ETH","underlyingAssetAddress":"0x4200000000000000000000000000000000000006","decimals":18,
         "minTradeSize":"1000000000000000000","maxTradeSize":"100000000000000000000","symbol":"WETH","chainId":8453,
         "address":"0x4200000000000000000000000000000000000006","active":true,"price":"2125396357110000000000","multipliers":[]}],
  "999": [
    {"underlying":"HYPE","underlyingAssetAddress":"0x5555555555555555555555555555555555555555","decimals":18,
     "minTradeSize":"25000000000000000000","maxTradeSize":"5000000000000000000000","symbol":"WHYPE","chainId":999,
     "address":"0x5555555555555555555555555555555555555555","active":true,"price":"73793286950000000000","multipliers":[]},
    {"underlying":"HYPE","underlyingAssetAddress":"0xfd739d4e423301ce9385c1fb8850539d657c296d","decimals":18,
     "minTradeSize":"25000000000000000000","maxTradeSize":"5000000000000000000000","symbol":"kHYPE","chainId":999,
     "address":"0xfd739d4e423301ce9385c1fb8850539d657c296d","active":true,"price":"75000000000000000000","multipliers":[]},
    {"underlying":"BTC","underlyingAssetAddress":"0x9fdbda0a5e284c32744d2f17ee5c74b284993463","decimals":8,
     "minTradeSize":"25000000000000000","maxTradeSize":"10000000000000000000","symbol":"UBTC","chainId":999,
     "address":"0x9fdbda0a5e284c32744d2f17ee5c74b284993463","active":false,"price":"74611750000000000000000","multipliers":[]}
  ]
}"#;

const PRODUCTS: &str = r#"[
  {"active":true,"asset_address":"0x5555555555555555555555555555555555555555","chainId":999,"is_put":true,"underlying_symbol":"HYPE",
   "usd":"0xb8ce59fc3717ada4c02eadf9682a9e934f625ebb","collateral":"0xb8ce59fc3717ada4c02eadf9682a9e934f625ebb","notionalCap":"1","contractsCap":"1"},
  {"active":true,"asset_address":"0x5555555555555555555555555555555555555555","chainId":999,"is_put":false,"underlying_symbol":"HYPE",
   "usd":"0xb88339cb7199b77e23db6e890353e22632ba630f","collateral":"0x5555555555555555555555555555555555555555","notionalCap":"1","contractsCap":"1"},
  {"active":true,"asset_address":"0x5555555555555555555555555555555555555555","chainId":999,"is_put":true,"underlying_symbol":"HYPE",
   "usd":"0xb88339cb7199b77e23db6e890353e22632ba630f","collateral":"0xb88339cb7199b77e23db6e890353e22632ba630f","notionalCap":"1","contractsCap":"1"},
  {"active":false,"asset_address":"0x5555555555555555555555555555555555555555","chainId":999,"is_put":true,"underlying_symbol":"HYPE",
   "usd":"0x111111a1a0667d36bd57c0a9f569b98057111111","collateral":"0x111111a1a0667d36bd57c0a9f569b98057111111","notionalCap":"1","contractsCap":"1"},
  {"active":true,"asset_address":"0xfd739d4e423301ce9385c1fb8850539d657c296d","chainId":999,"is_put":false,"underlying_symbol":"HYPE",
   "usd":"0xb88339cb7199b77e23db6e890353e22632ba630f","collateral":"0xfd739d4e423301ce9385c1fb8850539d657c296d","notionalCap":"1","contractsCap":"1"}
]"#;

const INVENTORY: &str = r#"{
  "HYPE": {
    "strikes": {"1794556800": [65.5, 70, 75], "1798185600": [100.5]},
    "expiries": [1794556800, 1798185600],
    "combinations": {
      "100.500000-1798185600": {"expiry":"25DEC26","expiration_timestamp":1798185600,"timeToExpiryDays":45.86089120370371,
        "strike":100.5,"isPut":false,"delta":0.0985066935962885,"bid":0,"ask":0,"index":73.67517611,"apy":7.511438404475595,
        "timestamp":1794223219,"products":[{"asset":"0x5555555555555555555555555555555555555555","strikeAsset":"0xb88339cb7199b77e23db6e890353e22632ba630f","collateralAsset":"0x5555555555555555555555555555555555555555"}]},
      "75.000000-1794556800": {"expiry":"13NOV26","expiration_timestamp":1794556800,"timeToExpiryDays":3.86,
        "strike":75,"isPut":false,"delta":0.38731895235166813,"bid":0,"ask":0,"index":73.67517611,"apy":141.2007640721469,
        "timestamp":1794223219,"products":[
          {"asset":"0x5555555555555555555555555555555555555555","strikeAsset":"0xb88339cb7199b77e23db6e890353e22632ba630f","collateralAsset":"0x5555555555555555555555555555555555555555"},
          {"asset":"0xfd739d4e423301ce9385c1fb8850539d657c296d","strikeAsset":"0xb88339cb7199b77e23db6e890353e22632ba630f","collateralAsset":"0xfd739d4e423301ce9385c1fb8850539d657c296d"},
          {"asset":"0x00000000000000000000000000000000000000aa","strikeAsset":"0xb88339cb7199b77e23db6e890353e22632ba630f","collateralAsset":"0x00000000000000000000000000000000000000aa"}]},
      "70.000000-1794556800": {"expiry":"13NOV26","expiration_timestamp":1794556800,"timeToExpiryDays":3.86,
        "strike":70,"isPut":true,"delta":-0.31,"bid":0.6,"ask":0.7,"index":73.67517611,"apy":85.1,
        "timestamp":1794223300,"products":[{"asset":"0x5555555555555555555555555555555555555555","strikeAsset":"0xb88339cb7199b77e23db6e890353e22632ba630f","collateralAsset":"0xb88339cb7199b77e23db6e890353e22632ba630f"}]},
      "75.000000-1794556800p": {"expiry":"13NOV26","expiration_timestamp":1794556800,"timeToExpiryDays":3.86,
        "strike":75,"isPut":true,"delta":-0.61,"bid":0,"ask":0,"index":73.67517611,"apy":160.3,
        "timestamp":1794223219,"products":[]},
      "65.500000-1794556800": {"expiry":"13NOV26","expiration_timestamp":1794556800,"timeToExpiryDays":3.86,
        "strike":65.5,"isPut":true,"delta":-0.12,"bid":0,"ask":0,"index":73.67517611,"apy":30,
        "timestamp":1794223219,"products":[]}
    }
  },
  "ZEC": {"strikes": {}, "expiries": [], "combinations": {}},
  "PUMP": {}
}"#;

const POSITIONS: &str = r#"{"jsonrpc":"2.0","id":"positions","result":[
  {"address":"0x5555555555555555555555555555555555555555","apr":"75.19","chainId":999,"createdAt":1785491468,"expiry":1786089600,
   "isBuy":true,"isPut":false,"quantity":"700000000000000000000","strike":"56500000000000000000","price":"775012109602763011",
   "txHash":"0x1e5e","usd":"0xb88339cb7199b77e23db6e890353e22632ba630f","collateral":"0x5555555555555555555555555555555555555555",
   "fees":"0","status":"SETTLED","symbol":"WHYPE","premium":"542508476000000000000","settlementCollateralPayout":"700000000000000000000",
   "payout":"0","payoutAsset":"0xb88339cb7199b77e23db6e890353e22632ba630f"},
  {"address":"0x9fdbda0a5e284c32744d2f17ee5c74b284993463","apr":"314.37","chainId":999,"createdAt":1790000000,"expiry":1790500000,
   "isBuy":true,"isPut":true,"quantity":"1000000000000000","strike":"100000000000000000000000","price":"10000000000000000000000",
   "txHash":"0xabcd","usd":"0xb8ce59fc3717ada4c02eadf9682a9e934f625ebb","collateral":"0xB8CE59FC3717ada4C02eaDF9682A9e934F625ebb",
   "fees":"0","status":"SETTLED","symbol":"UBTC","premium":"10000000000000000000","settlementCollateralPayout":"0",
   "payout":"100000","payoutAsset":"0x9fdbda0a5e284c32744d2f17ee5c74b284993463"},
  {"address":"0x5555555555555555555555555555555555555555","apr":"80","chainId":999,"createdAt":1794200000,"expiry":1794556800,
   "isBuy":true,"isPut":false,"quantity":"25000000000000000000","strike":"75000000000000000000","price":"1100000000000000000",
   "txHash":"","usd":"0xb88339cb7199b77e23db6e890353e22632ba630f","collateral":"0x94e8396e0869c9F2200760aF0621aFd240E1CF38",
   "fees":"0","status":"OPEN","symbol":"","premium":"27500000000000000000","settlementCollateralPayout":"0",
   "payout":"0","payoutAsset":"0xb88339cb7199b77e23db6e890353e22632ba630f"},
  {"address":"0x00000000000000000000000000000000000000aa","apr":"12","chainId":1,"createdAt":1780000000,"expiry":1780500000,
   "isBuy":true,"isPut":false,"quantity":"1000000000000000000","strike":"4000000000000000000000","price":"22000000000000000000",
   "txHash":"0xeeee","usd":"0xdac17f958d2ee523a2206206994597c13d831ec7","collateral":"0x00000000000000000000000000000000000000aa",
   "fees":"0","status":"SETTLED","symbol":"XYZ","premium":"22000000000000000000","settlementCollateralPayout":"0",
   "payout":"4000000000","payoutAsset":"0x00000000000000000000000000000000000000bb"}
]}"#;

const QUOTE: &str = r#"{"jsonrpc":"2.0","id":"request","result":{"assetAddress":"0x5555555555555555555555555555555555555555",
  "chainId":999,"isPut":false,"strike":"7500000000","expiry":1794556800,"maker":"0x66799BC69D7A7DAAF50B6AA03CD5C6A2DF1A9454",
  "nonce":"1794223645950579207","price":"1106612087450960600","quantity":"25000000000000000000","isTakerBuy":false,
  "signature":"0xeb7e","usd":"0xb88339cb7199b77e23db6e890353e22632ba630f","collateralAsset":"0x5555555555555555555555555555555555555555",
  "validUntil":1794223711,"premiumAsset":"","apy":142.43613245614674}}"#;

/// Serves canned GET bodies by path and RPC answers by method, and records every call.
#[derive(Default)]
struct FakeTransport {
    gets: HashMap<String, Result<String, TransportError>>,
    rpcs: HashMap<String, String>,
    get_calls: Mutex<Vec<String>>,
    rpc_calls: Mutex<Vec<(String, String, Value)>>,
}

impl FakeTransport {
    fn new() -> Self {
        Self::default()
            .get("/api/assets", ASSETS)
            .get("/api/products", PRODUCTS)
            .get("/api/inventory", INVENTORY)
    }

    fn get(mut self, path: &str, body: &str) -> Self {
        self.gets
            .insert(format!("{API_URL}{path}"), Ok(body.to_string()));
        self
    }

    fn get_error(mut self, path: &str, message: &str) -> Self {
        self.gets.insert(
            format!("{API_URL}{path}"),
            Err(TransportError(message.to_string())),
        );
        self
    }

    fn rpc(mut self, method: &str, body: &str) -> Self {
        self.rpcs.insert(method.to_string(), body.to_string());
        self
    }

    fn get_calls(&self) -> Vec<String> {
        let mut calls = self.get_calls.lock().unwrap().clone();
        calls.sort();
        calls
    }

    fn rpc_calls(&self) -> Vec<(String, String, Value)> {
        self.rpc_calls.lock().unwrap().clone()
    }
}

impl Transport for FakeTransport {
    async fn get(&self, url: &str) -> Result<String, TransportError> {
        self.get_calls.lock().unwrap().push(url.to_string());
        self.gets
            .get(url)
            .cloned()
            .unwrap_or_else(|| Err(TransportError(format!("fake: no GET for {url}"))))
    }

    async fn rpc(&self, url: &str, id: &str, request: &str) -> Result<String, TransportError> {
        let request: Value = serde_json::from_str(request).expect("RPC request is JSON");
        self.rpc_calls
            .lock()
            .unwrap()
            .push((url.to_string(), id.to_string(), request));
        self.rpcs
            .get(id)
            .cloned()
            .ok_or_else(|| TransportError(format!("fake: no RPC answer for {id}")))
    }
}

fn whype_call_quote() -> QuoteRequest {
    QuoteRequest {
        asset: "whype".into(),
        chain_id: None,
        option_type: OptionType::Call,
        strike: 75.0,
        expiry: "2026-11-13".into(),
        quantity: 25.0,
        collateral: None,
    }
}

fn invalid_message(err: RyskError) -> String {
    match err {
        RyskError::Invalid(message) => message,
        other => panic!("expected an input error, got {other:?}"),
    }
}

// --- assets ---

#[tokio::test]
async fn assets_join_products_into_collateral_lists() {
    let client = RyskClient::new(FakeTransport::new());

    let assets = client.assets(Some(999), Some("hype")).await.unwrap();

    let symbols: Vec<&str> = assets.iter().map(|a| a.symbol.as_str()).collect();
    assert_eq!(symbols, ["WHYPE", "kHYPE"]);
    let whype = &assets[0];
    assert_eq!(whype.chain, Some("HyperEVM"));
    assert_eq!(whype.address, WHYPE);
    assert_eq!(whype.price_usd, Some(73.79328695));
    assert_eq!(whype.min_trade_size, Some(25.0));
    assert_eq!(whype.max_trade_size, Some(5000.0));
    assert_eq!(whype.call_collateral, ["WHYPE"]);
    // The inactive USDH product is left out; Rysk's order is kept.
    assert_eq!(whype.put_collateral, ["USDT0", "USDC"]);
    assert_eq!(assets[1].call_collateral, ["kHYPE"]);
    assert!(assets[1].put_collateral.is_empty());
}

#[tokio::test]
async fn assets_are_sorted_by_chain_then_symbol_and_keep_inactive_ones() {
    let client = RyskClient::new(FakeTransport::new());

    let assets = client.assets(None, None).await.unwrap();

    let order: Vec<(u64, &str, bool)> = assets
        .iter()
        .map(|a| (a.chain_id, a.symbol.as_str(), a.active))
        .collect();
    assert_eq!(
        order,
        [
            (1, "WETH", true),
            (999, "UBTC", false),
            (999, "WHYPE", true),
            (999, "kHYPE", true),
            (8453, "WETH", true),
        ]
    );
    // Checksummed addresses are lowercased.
    assert_eq!(
        assets[0].address,
        "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"
    );
}

#[tokio::test]
async fn http_failures_surface_as_transport_errors() {
    let transport = FakeTransport::new().get_error("/api/products", "HTTP 502 Bad Gateway: oops");
    let client = RyskClient::new(&transport);

    let err = client.assets(None, None).await.unwrap_err();

    assert_eq!(
        err,
        RyskError::Transport("GET /api/products: HTTP 502 Bad Gateway: oops".into())
    );
}

#[tokio::test]
async fn malformed_bodies_are_parse_errors() {
    let transport = FakeTransport::new().get("/api/assets", "<html>maintenance</html>");
    let client = RyskClient::new(&transport);

    let err = client.assets(None, None).await.unwrap_err();

    assert!(
        matches!(err, RyskError::Parse(ref m) if m.starts_with("/api/assets:")),
        "{err:?}"
    );
}

// --- option chain ---

#[tokio::test]
async fn option_chain_is_sorted_by_expiry_then_strike_then_calls_first() {
    let client = RyskClient::new(FakeTransport::new());

    let chain = client
        .option_chain(&ChainFilter {
            underlying: "hype".into(),
            ..Default::default()
        })
        .await
        .unwrap();

    assert_eq!(chain.underlying, "HYPE");
    assert_eq!(chain.index_price, Some(73.67517611));
    assert_eq!(chain.as_of.as_deref(), Some("2026-11-09T11:21:40Z"));
    let order: Vec<(&str, f64, OptionType)> = chain
        .options
        .iter()
        .map(|o| (o.expiry.as_str(), o.strike, o.option_type))
        .collect();
    assert_eq!(
        order,
        [
            ("2026-11-13", 65.5, OptionType::Put),
            ("2026-11-13", 70.0, OptionType::Put),
            ("2026-11-13", 75.0, OptionType::Call),
            ("2026-11-13", 75.0, OptionType::Put),
            ("2026-12-25", 100.5, OptionType::Call),
        ]
    );
}

#[tokio::test]
async fn option_chain_rows_are_normalized() {
    let client = RyskClient::new(FakeTransport::new());

    let chain = client
        .option_chain(&ChainFilter {
            underlying: "HYPE".into(),
            option_type: Some(OptionType::Call),
            expiry: Some("2026-11-13".into()),
            ..Default::default()
        })
        .await
        .unwrap();

    let [row] = chain.options.as_slice() else {
        panic!("expected one row, got {:?}", chain.options);
    };
    assert_eq!(row.expiry_timestamp, 1_794_556_800);
    assert_eq!(row.days_to_expiry, 3.86);
    assert_eq!(row.delta, 0.3873);
    assert_eq!(row.apy, 141.2);
    assert_eq!((row.bid, row.ask), (None, None));
    assert_eq!(
        row.products,
        [
            ProductRef {
                chain_id: Some(999),
                asset: "WHYPE".into(),
                collateral: "WHYPE".into(),
            },
            ProductRef {
                chain_id: Some(999),
                asset: "kHYPE".into(),
                collateral: "kHYPE".into(),
            },
            // Unknown tokens keep their address and have no chain.
            ProductRef {
                chain_id: None,
                asset: "0x00000000000000000000000000000000000000aa".into(),
                collateral: "0x00000000000000000000000000000000000000aa".into(),
            },
        ]
    );
}

#[tokio::test]
async fn option_chain_filters_by_apy_delta_and_strike_range() {
    let client = RyskClient::new(FakeTransport::new());

    let chain = client
        .option_chain(&ChainFilter {
            underlying: "WHYPE".into(),
            option_type: Some(OptionType::Put),
            min_strike: Some(65.5),
            max_strike: Some(75.0),
            min_apy: Some(30.0),
            max_abs_delta: Some(0.31),
            ..Default::default()
        })
        .await
        .unwrap();

    let strikes: Vec<f64> = chain.options.iter().map(|o| o.strike).collect();
    assert_eq!(strikes, [65.5, 70.0]);
    assert_eq!(chain.options[1].bid, Some(0.6));
    assert_eq!(chain.options[1].ask, Some(0.7));
}

#[tokio::test]
async fn option_chain_of_an_underlying_without_strikes_is_empty() {
    let client = RyskClient::new(FakeTransport::new());

    for underlying in ["ZEC", "PUMP"] {
        let chain = client
            .option_chain(&ChainFilter {
                underlying: underlying.into(),
                ..Default::default()
            })
            .await
            .unwrap();
        assert!(chain.options.is_empty(), "{underlying}");
        assert_eq!(chain.index_price, None);
    }
}

#[tokio::test]
async fn option_chain_rejects_unknown_underlyings_with_the_listed_ones() {
    let client = RyskClient::new(FakeTransport::new());

    let err = client
        .option_chain(&ChainFilter {
            underlying: "DOGE".into(),
            ..Default::default()
        })
        .await
        .unwrap_err();

    assert_eq!(
        invalid_message(err),
        r#"unknown underlying "DOGE"; Rysk lists: HYPE, PUMP, ZEC"#
    );
}

#[tokio::test]
async fn option_chain_rejects_a_malformed_expiry_before_any_request() {
    let transport = FakeTransport::new();
    let client = RyskClient::new(&transport);

    let err = client
        .option_chain(&ChainFilter {
            underlying: "HYPE".into(),
            expiry: Some("13NOV26".into()),
            ..Default::default()
        })
        .await
        .unwrap_err();

    assert!(invalid_message(err).contains("YYYY-MM-DD"));
    assert!(transport.get_calls().is_empty());
}

// --- quote ---

#[tokio::test]
async fn quote_sends_an_anonymous_rfq_and_normalizes_the_answer() {
    let transport = FakeTransport::new().rpc("request", QUOTE);
    let client = RyskClient::new(&transport);

    let quote = client.quote(&whype_call_quote()).await.unwrap();

    let calls = transport.rpc_calls();
    let [(url, id, request)] = calls.as_slice() else {
        panic!("expected one RPC call, got {calls:?}");
    };
    assert_eq!(url, TAKER_WS_URL);
    assert_eq!(id, "request");
    assert_eq!(
        request,
        &json!({
            "jsonrpc": "2.0",
            "id": "request",
            "method": "request",
            "params": {
                "asset": WHYPE,
                "chainId": 999,
                "expiry": 1_794_556_800,
                "isPut": false,
                "isTakerBuy": false,
                "quantity": "25000000000000000000",
                "strike": "7500000000",
                "taker": "0x0000000000000000000000000000000000000000",
                "usd": USDC_HYPEREVM,
                "collateralAsset": WHYPE,
            }
        })
    );

    assert_eq!(quote.asset, "WHYPE");
    assert_eq!(quote.chain_id, 999);
    assert_eq!(quote.option_type, OptionType::Call);
    assert_eq!(quote.strike, Some(75.0));
    assert_eq!(quote.expiry, "2026-11-13");
    assert_eq!(quote.quantity, Some(25.0));
    assert_eq!(quote.price_per_contract, Some(1.1066120874509606));
    assert_eq!(quote.total_premium, Some(27.665302));
    assert_eq!(quote.premium_asset, "USDC");
    assert_eq!(quote.apy, Some(142.44));
    assert_eq!(quote.collateral, "WHYPE");
    assert_eq!(quote.maker, "0x66799bc69d7a7daaf50b6aa03cd5c6a2df1a9454");
    assert_eq!(quote.valid_until, "2026-11-09T11:28:31Z");
}

#[tokio::test]
async fn quote_picks_the_requested_collateral_and_passes_timestamps_through() {
    let transport = FakeTransport::new().rpc("request", QUOTE);
    let client = RyskClient::new(&transport);

    client
        .quote(&QuoteRequest {
            option_type: OptionType::Put,
            strike: 70.0,
            expiry: "1794556800".into(),
            collateral: Some("usdc".into()),
            ..whype_call_quote()
        })
        .await
        .unwrap();

    let (_, _, request) = &transport.rpc_calls()[0];
    assert_eq!(request["params"]["isPut"], true);
    assert_eq!(request["params"]["strike"], "7000000000");
    assert_eq!(request["params"]["expiry"], 1_794_556_800);
    assert_eq!(request["params"]["usd"], USDC_HYPEREVM);
    assert_eq!(request["params"]["collateralAsset"], USDC_HYPEREVM);
    // A timestamp expiry needs no inventory lookup.
    assert!(
        !transport
            .get_calls()
            .iter()
            .any(|u| u.ends_with("/api/inventory"))
    );
}

#[tokio::test]
async fn quote_defaults_put_collateral_to_the_first_listed_product() {
    let transport = FakeTransport::new().rpc("request", QUOTE);
    let client = RyskClient::new(&transport);

    client
        .quote(&QuoteRequest {
            option_type: OptionType::Put,
            strike: 70.0,
            ..whype_call_quote()
        })
        .await
        .unwrap();

    let (_, _, request) = &transport.rpc_calls()[0];
    assert_eq!(request["params"]["usd"], USDT0);
    assert_eq!(request["params"]["collateralAsset"], USDT0);
}

#[tokio::test]
async fn quote_rejects_collateral_the_product_does_not_accept() {
    let transport = FakeTransport::new();
    let client = RyskClient::new(&transport);

    let err = client
        .quote(&QuoteRequest {
            option_type: OptionType::Put,
            collateral: Some("USDH".into()),
            ..whype_call_quote()
        })
        .await
        .unwrap_err();

    assert_eq!(
        invalid_message(err),
        "WHYPE puts cannot use USDH as collateral; accepted: USDT0, USDC"
    );
    assert!(transport.rpc_calls().is_empty());
}

#[tokio::test]
async fn quote_rejects_an_asset_without_that_option_type() {
    let client = RyskClient::new(FakeTransport::new());

    let err = client
        .quote(&QuoteRequest {
            asset: "kHYPE".into(),
            option_type: OptionType::Put,
            ..whype_call_quote()
        })
        .await
        .unwrap_err();

    assert_eq!(
        invalid_message(err),
        "Rysk offers no put on kHYPE (chain 999)"
    );
}

#[tokio::test]
async fn quote_needs_a_chain_for_symbols_on_several_chains() {
    let client = RyskClient::new(FakeTransport::new());

    let err = client
        .quote(&QuoteRequest {
            asset: "WETH".into(),
            ..whype_call_quote()
        })
        .await
        .unwrap_err();

    assert_eq!(
        invalid_message(err),
        r#""WETH" matches several assets: WETH (chain 1), WETH (chain 8453); pass one symbol and its chain_id"#
    );
}

#[tokio::test]
async fn quote_lists_assets_when_an_underlying_is_ambiguous_or_unknown() {
    let client = RyskClient::new(FakeTransport::new());

    let ambiguous = client
        .quote(&QuoteRequest {
            asset: "HYPE".into(),
            ..whype_call_quote()
        })
        .await
        .unwrap_err();
    assert_eq!(
        invalid_message(ambiguous),
        r#""HYPE" matches several assets: WHYPE (chain 999), kHYPE (chain 999); pass one symbol and its chain_id"#
    );

    let unknown = client
        .quote(&QuoteRequest {
            asset: "DOGE".into(),
            chain_id: Some(999),
            ..whype_call_quote()
        })
        .await
        .unwrap_err();
    assert_eq!(
        invalid_message(unknown),
        r#"unknown asset "DOGE" on chain 999; active assets: WHYPE (chain 999), kHYPE (chain 999)"#
    );
}

#[tokio::test]
async fn quote_rejects_an_unlisted_expiry_with_the_listed_ones() {
    let client = RyskClient::new(FakeTransport::new());

    let err = client
        .quote(&QuoteRequest {
            expiry: "2026-11-14".into(),
            ..whype_call_quote()
        })
        .await
        .unwrap_err();

    assert_eq!(
        invalid_message(err),
        "no HYPE expiry on 2026-11-14; listed expiries: 2026-11-13, 2026-12-25"
    );
}

#[tokio::test]
async fn quote_rejects_strikes_finer_than_rysk_supports_before_any_request() {
    let transport = FakeTransport::new();
    let client = RyskClient::new(&transport);

    let err = client
        .quote(&QuoteRequest {
            strike: 75.000000001,
            ..whype_call_quote()
        })
        .await
        .unwrap_err();

    assert!(invalid_message(err).starts_with("strike 75.000000001 has more than 8"));
    assert!(transport.get_calls().is_empty());
}

#[tokio::test]
async fn quote_surfaces_rysk_rejections() {
    let transport = FakeTransport::new().rpc(
        "request",
        r#"{"jsonrpc":"2.0","id":"request","error":{"code":-32603,"message":"invalid strike"}}"#,
    );
    let client = RyskClient::new(&transport);

    let err = client.quote(&whype_call_quote()).await.unwrap_err();

    assert_eq!(err, RyskError::Api("invalid strike (code -32603)".into()));
}

// --- propose_sale ---

const PUT_QUOTE: &str = r#"{"jsonrpc":"2.0","id":"request","result":{"assetAddress":"0x5555555555555555555555555555555555555555",
  "chainId":999,"isPut":true,"strike":"7000000000","expiry":1794556800,"maker":"0x66799BC69D7A7DAAF50B6AA03CD5C6A2DF1A9454",
  "nonce":"1794223645950579208","price":"640000000000000000","quantity":"25000000000000000000","isTakerBuy":false,
  "signature":"0xeb7f","usd":"0xb88339cb7199b77e23db6e890353e22632ba630f","collateralAsset":"0xb88339cb7199b77e23db6e890353e22632ba630f",
  "validUntil":1794223711,"premiumAsset":"","apy":85.123}}"#;

const HYPEREVM_MARGIN_POOL: &str = "0x24a44f1dc25540c62c1196ffc297dfc951c91ab4";

#[tokio::test]
async fn propose_sale_links_a_covered_call_to_the_rysk_app() {
    let transport = FakeTransport::new().rpc("request", QUOTE);
    let client = RyskClient::new(&transport);

    let proposal = client
        .propose_sale(&whype_call_quote(), None)
        .await
        .unwrap();

    // The same anonymous RFQ as rysk_quote: proposing signs nothing.
    let calls = transport.rpc_calls();
    let [(_, _, request)] = calls.as_slice() else {
        panic!("expected one RPC call, got {calls:?}");
    };
    assert_eq!(
        request["params"]["taker"],
        "0x0000000000000000000000000000000000000000"
    );

    assert_eq!(proposal.quote.apy, Some(142.44));
    assert_eq!(proposal.collateral_amount, 25.0);
    assert_eq!(
        proposal.approval_spender.as_deref(),
        Some(HYPEREVM_MARGIN_POOL)
    );
    assert_eq!(proposal.min_apy, None);
    assert_eq!(proposal.meets_min_apy, None);
    assert_eq!(
        proposal.app_url,
        "https://app.rysk.finance/earn?chainId=999&asset=WHYPE&collateral=WHYPE&strike=USDC&type=call&expiry=1794556800"
    );
    assert_eq!(
        proposal.steps,
        [
            "Open app_url in a browser with your wallet (the Rabby extension, or Rabby Mobile's \
             built-in browser).",
            "Check that WHYPE call with WHYPE collateral, expiring 2026-11-13, is selected; pick \
             strike 75 and enter 25 WHYPE.",
            "At proposal time the quote was 27.665302 USDC at 142.44% APR; the app shows a fresh \
             one before you sign.",
            "Your wallet asks to approve 25 WHYPE to Rysk's MarginPool \
             0x24a44f1dc25540c62c1196ffc297dfc951c91ab4 if the allowance is short, then to sign a \
             Rysk Confirmation message. Rysk submits the trade and the collateral stays locked \
             until expiry.",
        ]
    );

    let json = serde_json::to_value(&proposal).unwrap();
    assert!(json.get("min_apy").is_none(), "json: {json}");
    assert!(json.get("meets_min_apy").is_none(), "json: {json}");
}

#[tokio::test]
async fn propose_sale_asks_for_the_put_collateral_deposit() {
    let transport = FakeTransport::new().rpc("request", PUT_QUOTE);
    let client = RyskClient::new(&transport);

    let proposal = client
        .propose_sale(
            &QuoteRequest {
                option_type: OptionType::Put,
                strike: 70.0,
                collateral: Some("USDC".into()),
                ..whype_call_quote()
            },
            Some(80.0),
        )
        .await
        .unwrap();

    assert_eq!(proposal.collateral_amount, 1750.0);
    assert_eq!(proposal.meets_min_apy, Some(true));
    assert_eq!(
        proposal.app_url,
        "https://app.rysk.finance/earn?chainId=999&asset=WHYPE&collateral=USDC&strike=USDC&type=put&expiry=1794556800"
    );
    assert_eq!(
        proposal.steps[1],
        "Check that WHYPE put with USDC collateral, expiring 2026-11-13, is selected; pick strike \
         70 and deposit 1750 USDC."
    );
    assert_eq!(
        proposal.steps[2],
        "Accept only if the app's live quote shows at least 80% APR; at proposal time it was 16 \
         USDC at 85.12% APR."
    );
    assert!(proposal.steps[3].starts_with("Your wallet asks to approve 1750 USDC to Rysk's"));
}

#[tokio::test]
async fn propose_sale_flags_a_quote_below_min_apy_and_accepts_one_at_it() {
    let transport = FakeTransport::new().rpc("request", QUOTE);
    let client = RyskClient::new(&transport);

    let below = client
        .propose_sale(&whype_call_quote(), Some(142.45))
        .await
        .unwrap();
    let at = client
        .propose_sale(&whype_call_quote(), Some(142.44))
        .await
        .unwrap();

    assert_eq!(below.meets_min_apy, Some(false));
    assert_eq!(below.min_apy, Some(142.45));
    assert_eq!(at.meets_min_apy, Some(true));
}

#[tokio::test]
async fn propose_sale_surfaces_rysk_rejections() {
    let transport = FakeTransport::new().rpc(
        "request",
        r#"{"jsonrpc":"2.0","id":"request","error":{"code":-32603,"message":"invalid strike"}}"#,
    );
    let client = RyskClient::new(&transport);

    let err = client
        .propose_sale(&whype_call_quote(), Some(100.0))
        .await
        .unwrap_err();

    assert_eq!(err, RyskError::Api("invalid strike (code -32603)".into()));
}

// --- positions ---

#[tokio::test]
async fn positions_are_normalized_and_sorted_latest_expiry_first() {
    let transport = FakeTransport::new().rpc("positions", POSITIONS);
    let client = RyskClient::new(&transport);

    let positions = client
        .positions(
            " 0x418E14E4D34ADD3C23D2BE1C11E6B02E628AE6DD ",
            &PositionFilter::default(),
        )
        .await
        .unwrap();

    let (_, _, request) = &transport.rpc_calls()[0];
    assert_eq!(request["method"], "positions");
    assert_eq!(request["params"], json!({ "address": WALLET }));

    let order: Vec<(&str, i64)> = positions
        .iter()
        .map(|p| (p.symbol.as_str(), p.expiry_timestamp))
        .collect();
    // The open position has no symbol upstream; it is named after its asset.
    assert_eq!(
        order,
        [
            ("WHYPE", 1_794_556_800),
            ("UBTC", 1_790_500_000),
            ("WHYPE", 1_786_089_600),
            ("XYZ", 1_780_500_000),
        ]
    );

    let open = &positions[0];
    assert_eq!(open.status, "OPEN");
    assert_eq!(open.exercised, None);
    assert_eq!(open.payout, None);
    assert_eq!(open.collateral_returned, None);
    assert_eq!(open.tx_hash, None);
    assert_eq!(open.tx_url, None);
    // A collateral token Rysk does not list keeps its (lowercased) address.
    assert_eq!(
        open.collateral,
        "0x94e8396e0869c9f2200760af0621afd240e1cf38"
    );
}

#[tokio::test]
async fn settled_positions_report_whether_they_were_exercised() {
    let client = RyskClient::new(FakeTransport::new().rpc("positions", POSITIONS));

    let positions = client
        .positions(WALLET, &PositionFilter::default())
        .await
        .unwrap();

    let expired_call = &positions[2];
    assert_eq!(expired_call.option_type, OptionType::Call);
    assert_eq!(expired_call.strike, Some(56.5));
    assert_eq!(expired_call.quantity, Some(700.0));
    assert_eq!(expired_call.price_per_contract, Some(0.775012109602763));
    assert_eq!(expired_call.premium, Some(542.508476));
    assert_eq!(expired_call.premium_asset, "USDC");
    assert_eq!(expired_call.apr, Some(75.19));
    assert_eq!(expired_call.expiry, "2026-08-07");
    assert_eq!(expired_call.created_at, "2026-07-31T09:51:08Z");
    assert_eq!(expired_call.exercised, Some(false));
    assert_eq!(expired_call.payout, None);
    assert_eq!(
        expired_call.collateral_returned,
        Some(TokenAmount {
            asset: "WHYPE".into(),
            amount: Some(700.0),
            base_units: "700000000000000000000".into(),
        })
    );
    assert_eq!(
        expired_call.tx_url.as_deref(),
        Some("https://hyperevmscan.io/tx/0x1e5e")
    );

    // An assigned put pays out the asset, in the asset's own decimals (8 for UBTC).
    let assigned_put = &positions[1];
    assert_eq!(assigned_put.option_type, OptionType::Put);
    assert_eq!(assigned_put.strike, Some(100_000.0));
    assert_eq!(assigned_put.collateral, "USDT0");
    assert_eq!(assigned_put.exercised, Some(true));
    assert_eq!(
        assigned_put.payout,
        Some(TokenAmount {
            asset: "UBTC".into(),
            amount: Some(0.001),
            base_units: "100000".into(),
        })
    );
    assert_eq!(assigned_put.collateral_returned, None);

    // Unknown decimals leave the amount empty but keep the exact base units.
    let unknown = &positions[3];
    assert_eq!(
        unknown.payout,
        Some(TokenAmount {
            asset: "0x00000000000000000000000000000000000000bb".into(),
            amount: None,
            base_units: "4000000000".into(),
        })
    );
    assert_eq!(unknown.premium_asset, "USDT");
    assert_eq!(
        unknown.tx_url.as_deref(),
        Some("https://etherscan.io/tx/0xeeee")
    );
}

#[tokio::test]
async fn positions_filter_by_status_and_chain() {
    let client = RyskClient::new(FakeTransport::new().rpc("positions", POSITIONS));

    let settled_on_hyperevm = client
        .positions(
            WALLET,
            &PositionFilter {
                status: Some("settled".into()),
                chain_id: Some(999),
            },
        )
        .await
        .unwrap();

    let symbols: Vec<&str> = settled_on_hyperevm
        .iter()
        .map(|p| p.symbol.as_str())
        .collect();
    assert_eq!(symbols, ["UBTC", "WHYPE"]);
}

#[tokio::test]
async fn positions_reject_malformed_addresses_before_any_request() {
    let transport = FakeTransport::new();
    let client = RyskClient::new(&transport);

    let err = client
        .positions("0x1234", &PositionFilter::default())
        .await
        .unwrap_err();

    assert!(invalid_message(err).contains("20-byte hex address"));
    assert!(transport.get_calls().is_empty());
    assert!(transport.rpc_calls().is_empty());
}

#[tokio::test]
async fn positions_without_an_answer_are_transport_errors() {
    let client = RyskClient::new(FakeTransport::new());

    let err = client
        .positions(WALLET, &PositionFilter::default())
        .await
        .unwrap_err();

    assert_eq!(
        err,
        RyskError::Transport("positions: fake: no RPC answer for positions".into())
    );
}

// --- settlement prices ---

#[tokio::test]
async fn settlement_prices_are_scaled_latest_first_with_unsettled_as_null() {
    let transport = FakeTransport::new().get(
        "/api/expiry-prices/999/0x5555555555555555555555555555555555555555",
        r#"{"1792742400":"7841003462","1798185600":"0","1793952000":{"price":"7742283455","verified":true}}"#,
    );
    let client = RyskClient::new(&transport);

    let prices = client.settlement_prices("WHYPE", None).await.unwrap();

    assert_eq!(prices.asset, "WHYPE");
    assert_eq!(prices.chain_id, 999);
    let rows: Vec<(&str, Option<f64>, Option<bool>)> = prices
        .prices
        .iter()
        .map(|p| (p.expiry.as_str(), p.price, p.verified))
        .collect();
    assert_eq!(
        rows,
        [
            ("2026-12-25", None, None),
            ("2026-11-06", Some(77.42283455), Some(true)),
            ("2026-10-23", Some(78.41003462), None),
        ]
    );
}

#[tokio::test]
async fn settlement_prices_accept_an_asset_address() {
    let transport = FakeTransport::new().get(
        "/api/expiry-prices/999/0x5555555555555555555555555555555555555555",
        "{}",
    );
    let client = RyskClient::new(&transport);

    let prices = client
        .settlement_prices("0x5555555555555555555555555555555555555555", Some(999))
        .await
        .unwrap();

    assert_eq!(prices.asset, "WHYPE");
    assert!(prices.prices.is_empty());
}

#[tokio::test]
async fn settlement_prices_reject_unreadable_expiries() {
    let transport = FakeTransport::new().get(
        "/api/expiry-prices/999/0x5555555555555555555555555555555555555555",
        r#"{"soon":"1"}"#,
    );
    let client = RyskClient::new(&transport);

    let err = client.settlement_prices("WHYPE", None).await.unwrap_err();

    assert!(
        matches!(err, RyskError::Parse(ref m) if m.contains("\"soon\"")),
        "{err:?}"
    );
}

#[tokio::test]
async fn custom_urls_are_used_for_http_and_websocket() {
    let mut transport = FakeTransport::default().rpc(
        "positions",
        r#"{"jsonrpc":"2.0","id":"positions","result":[]}"#,
    );
    transport
        .gets
        .insert("http://local/api/assets".into(), Ok(ASSETS.into()));
    let client = RyskClient::new(&transport).with_urls("http://local", "ws://local/taker");

    let positions = client
        .positions(WALLET, &PositionFilter::default())
        .await
        .unwrap();

    assert!(positions.is_empty());
    assert_eq!(transport.get_calls(), ["http://local/api/assets"]);
    assert_eq!(transport.rpc_calls()[0].0, "ws://local/taker");
}
