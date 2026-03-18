use std::{ops::Add, str::FromStr};

use super::*;
use cosmwasm_std::{
    coin, coins, from_json,
    testing::{mock_dependencies, MockQuerier},
    to_json_binary, Addr, ContractResult, CosmosMsg, Decimal, QuerierWrapper, Response, SubMsg,
    SystemError, SystemResult, Uint128, WasmMsg, WasmQuery,
};
use rujira_rs::{
    bow,
    exchange::{SwapContext, SwapItem},
    fin::{Denoms, Side, Tick},
};

fn mock_querier() -> MockQuerier {
    let mut q = MockQuerier::default();
    q.update_wasm(|x| match x {
        WasmQuery::Smart { msg, .. } => {
            let query: bow::QueryMsg = from_json(msg).unwrap();
            match query {
                bow::QueryMsg::Quote(query) => {
                    let ask_total: Uint128 = query
                        .data
                        .map(from_json)
                        .transpose()
                        .unwrap()
                        .unwrap_or_default();
                    // Double the order size until we run out of funds
                    if ask_total.gt(&Uint128::from(1000000u128)) {
                        return SystemResult::Err(SystemError::Unknown {});
                    }

                    let size = if ask_total.is_zero() {
                        Uint128::from(100u128)
                    } else {
                        ask_total * Uint128::from(2u128)
                    };

                    SystemResult::Ok(ContractResult::Ok(
                        to_json_binary(&bow::QuoteResponse {
                            size,
                            data: to_json_binary(&size.add(ask_total)).ok(),
                            price: query
                                .min_price
                                .unwrap_or(Decimal::one())
                                .add(Decimal::from_ratio(1u128, 4u128)),
                        })
                        .unwrap(),
                    ))
                }
                bow::QueryMsg::Strategy {} => todo!(),
            }
        }
        _ => SystemResult::Err(SystemError::Unknown {}),
    });
    q
}

#[test]
fn test_iterator() {
    let q: MockQuerier = mock_querier();
    let contract = Addr::unchecked("bow");
    let denoms = Denoms::new("btc", "usdc");
    let mut iter = MarketMakerIter::new(
        QuerierWrapper::new(&q),
        denoms.clone(),
        Side::Quote,
        contract.clone(),
        Tick::new(3),
    );

    let items: Vec<MarketMakerOffer> = iter.by_ref().take(5).collect();

    assert_eq!(
        items,
        vec![
            MarketMakerOffer {
                contract: contract.clone(),
                price: Decimal::from_str("1.25").unwrap(),
                rate: Decimal::from_str("1.25").unwrap(),
                total: Uint128::from(100u128),
                side: Side::Quote,
                denoms: denoms.clone(),
            },
            MarketMakerOffer {
                contract: contract.clone(),
                price: Decimal::from_str("1.5").unwrap(),
                rate: Decimal::from_str("1.5").unwrap(),
                total: Uint128::from(200u128),
                side: Side::Quote,
                denoms: denoms.clone(),
            },
            MarketMakerOffer {
                contract: contract.clone(),
                price: Decimal::from_str("1.75").unwrap(),
                rate: Decimal::from_str("1.75").unwrap(),
                total: Uint128::from(600u128),
                side: Side::Quote,
                denoms: denoms.clone(),
            },
            MarketMakerOffer {
                contract: contract.clone(),
                price: Decimal::from_str("2").unwrap(),
                rate: Decimal::from_str("2").unwrap(),
                total: Uint128::from(1800u128),
                side: Side::Quote,
                denoms: denoms.clone(),
            },
            MarketMakerOffer {
                contract: contract.clone(),
                price: Decimal::from_str("2.25").unwrap(),
                rate: Decimal::from_str("2.25").unwrap(),
                total: Uint128::from(5400u128),
                side: Side::Quote,
                denoms: denoms.clone(),
            }
        ]
    );
}

#[test]
fn test_market_market_item_swap() {
    let contract = Addr::unchecked("bow");
    let denoms = Denoms::new("btc", "usdc");

    // Quote partial fill
    let mut o = MarketMakerOffer {
        contract: contract.clone(),
        price: Decimal::from_str("1.25").unwrap(),
        rate: Decimal::from_str("1.25").unwrap(),
        total: Uint128::from(10000u128),
        side: Side::Quote,
        denoms: denoms.clone(),
    };
    let (bid_returned, offer_consumed, _) = o
        .swap(&mut MarketMakerContext::new(), Uint128::from(500u128))
        .unwrap();
    assert_eq!(bid_returned, Uint128::from(500u128));
    assert_eq!(offer_consumed, Uint128::from(625u128));

    // Quote full fill
    let mut o = MarketMakerOffer {
        contract: contract.clone(),
        price: Decimal::from_str("1.25").unwrap(),
        rate: Decimal::from_str("1.25").unwrap(),
        total: Uint128::from(10000u128),
        side: Side::Quote,
        denoms: denoms.clone(),
    };
    let (bid_returned, offer_consumed, _) = o
        .swap(&mut MarketMakerContext::new(), Uint128::from(500000u128))
        .unwrap();
    assert_eq!(bid_returned, Uint128::from(8000u128));
    assert_eq!(offer_consumed, Uint128::from(10000u128));

    // Base partial fill
    let mut o = MarketMakerOffer {
        contract: contract.clone(),
        price: Decimal::from_str("1.25").unwrap(),
        rate: Decimal::from_str("0.8").unwrap(),
        total: Uint128::from(10000u128),
        side: Side::Base,
        denoms: denoms.clone(),
    };
    let (bid_returned, offer_consumed, _) = o
        .swap(&mut MarketMakerContext::new(), Uint128::from(500u128))
        .unwrap();
    assert_eq!(bid_returned, Uint128::from(500u128));
    assert_eq!(offer_consumed, Uint128::from(625u128));

    // Base full fill
    let mut o = MarketMakerOffer {
        contract: contract.clone(),
        price: Decimal::from_str("1.25").unwrap(),
        rate: Decimal::from_str("0.8").unwrap(),
        total: Uint128::from(10000u128),
        side: Side::Base,
        denoms: denoms.clone(),
    };
    let (bid_returned, offer_consumed, _) = o
        .swap(&mut MarketMakerContext::new(), Uint128::from(500000u128))
        .unwrap();
    assert_eq!(bid_returned, Uint128::from(8000u128));
    assert_eq!(offer_consumed, Uint128::from(10000u128));

    let mut o = MarketMakerOffer {
        contract: contract.clone(),
        price: Decimal::from_str("0.00000999").unwrap(),
        total: Uint128::from(19980u128),
        side: Side::Base,
        rate: Decimal::from_str("100100.11").unwrap(),
        denoms: denoms.clone(),
    };
    let (bid_returned, offer_consumed, _) = o
        .swap(&mut MarketMakerContext::new(), Uint128::from(50000000u128))
        .unwrap();
    assert_eq!(bid_returned, Uint128::from(50000000u128));
    assert_eq!(offer_consumed, Uint128::from(499u128));
}

#[test]
fn test_market_maker_commit_no_fee() {
    let mut deps = mock_dependencies();
    let contract = Addr::unchecked("bow");
    let denoms = Denoms::new("btc", "usdc");
    let mut ctx = MarketMakerContext::new();

    MarketMakerOffer {
        contract: contract.clone(),
        price: Decimal::from_str("1.25").unwrap(),
        rate: Decimal::from_str("1.25").unwrap(),
        total: Uint128::from(100000u128),
        side: Side::Quote,
        denoms: denoms.clone(),
    }
    .swap(&mut ctx, Uint128::new(1000))
    .unwrap();
    MarketMakerOffer {
        contract: contract.clone(),
        price: Decimal::from_str("1.5").unwrap(),
        rate: Decimal::from_str("1.5").unwrap(),
        total: Uint128::from(200000u128),
        side: Side::Quote,
        denoms: denoms.clone(),
    }
    .swap(&mut ctx, Uint128::new(1000))
    .unwrap();

    let res = ctx
        .commit(deps.as_mut().storage, Response::default())
        .unwrap();
    // Full offer sent to BOW, full ask as min_return, no fees collected
    // 1000 @ 1.25 = 1250 return
    // 1000 @ 1.5 = 1500 return
    // Total 2000 in, 2750 min_return

    assert_eq!(
        res.messages,
        vec![SubMsg::reply_never(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: "bow".to_string(),
            msg: to_json_binary(&bow::ExecuteMsg::Swap {
                min_return: coin(2750, "usdc"),
                to: None,
                callback: None
            })
            .unwrap(),
            funds: coins(2000, "btc")
        }))]
    );

    assert!(res.events.is_empty());
}
