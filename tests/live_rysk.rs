//! Live integration tests against the real Rysk V12 API.
//!
//! The API is public, so these need no credentials, but they hit the network and
//! `live_quote` sends an RFQ to Rysk's makers, so they are `#[ignore]`d by default:
//!
//!     cargo test --test live_rysk -- --ignored --nocapture
//!
//! `live_positions` reads RYSK_ADDRESS when set, otherwise the zero address.

use rysk_mcp::rysk::transport::NetTransport;
use rysk_mcp::rysk::{ChainFilter, OptionType, PositionFilter, QuoteRequest, RyskClient};

fn live_client() -> RyskClient<NetTransport> {
    RyskClient::new(NetTransport::new().expect("build transport"))
}

#[tokio::test]
#[ignore = "hits the live Rysk API"]
async fn live_assets() {
    let assets = live_client().assets(Some(999), None).await.unwrap();
    println!("{}", serde_json::to_string_pretty(&assets).unwrap());

    let whype = assets
        .iter()
        .find(|a| a.symbol == "WHYPE")
        .expect("WHYPE on HyperEVM");
    assert!(whype.price_usd.is_some_and(|p| p > 0.0));
    assert!(whype.min_trade_size.is_some_and(|s| s > 0.0));
    assert!(!whype.call_collateral.is_empty());
}

#[tokio::test]
#[ignore = "hits the live Rysk API"]
async fn live_option_chain() {
    let chain = live_client()
        .option_chain(&ChainFilter {
            underlying: "HYPE".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    println!("{}", serde_json::to_string_pretty(&chain).unwrap());

    assert!(!chain.options.is_empty());
    assert!(chain.index_price.is_some_and(|p| p > 0.0));
    assert!(
        chain
            .options
            .iter()
            .all(|o| o.strike > 0.0 && o.apy.is_finite())
    );
}

#[tokio::test]
#[ignore = "hits the live Rysk API and sends an RFQ"]
async fn live_quote() {
    let client = live_client();
    let chain = client
        .option_chain(&ChainFilter {
            underlying: "HYPE".into(),
            option_type: Some(OptionType::Call),
            ..Default::default()
        })
        .await
        .unwrap();
    let option = chain
        .options
        .iter()
        .find(|o| o.products.iter().any(|p| p.asset == "WHYPE"))
        .expect("a WHYPE call");
    let whype = client
        .assets(Some(999), Some("HYPE"))
        .await
        .unwrap()
        .into_iter()
        .find(|a| a.symbol == "WHYPE")
        .unwrap();

    let quote = client
        .quote(&QuoteRequest {
            asset: "WHYPE".into(),
            chain_id: Some(999),
            option_type: OptionType::Call,
            strike: option.strike,
            expiry: option.expiry.clone(),
            quantity: whype.min_trade_size.unwrap(),
            collateral: None,
        })
        .await
        .unwrap();
    println!("{}", serde_json::to_string_pretty(&quote).unwrap());

    assert_eq!(quote.strike, Some(option.strike));
    assert_eq!(quote.expiry_timestamp, option.expiry_timestamp);
    assert!(quote.price_per_contract.is_some_and(|p| p > 0.0));
    assert_eq!(quote.collateral, "WHYPE");
}

#[tokio::test]
#[ignore = "hits the live Rysk API"]
async fn live_positions() {
    let _ = dotenvy::dotenv();
    let address = std::env::var("RYSK_ADDRESS")
        .unwrap_or_else(|_| "0x0000000000000000000000000000000000000000".into());

    let positions = live_client()
        .positions(&address, &PositionFilter::default())
        .await
        .unwrap();
    println!("{}", serde_json::to_string_pretty(&positions).unwrap());

    assert!(positions.iter().all(|p| p.quantity.is_some()));
}

#[tokio::test]
#[ignore = "hits the live Rysk API"]
async fn live_settlement_prices() {
    let prices = live_client()
        .settlement_prices("WHYPE", Some(999))
        .await
        .unwrap();
    println!("{}", serde_json::to_string_pretty(&prices).unwrap());

    assert!(prices.prices.iter().any(|p| p.price.is_some()));
}
