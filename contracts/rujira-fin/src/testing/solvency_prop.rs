use std::str::FromStr;

use cosmwasm_std::{coin, Addr, Decimal, SignedDecimal, Uint128};
use cw_multi_test::Executor;
use proptest::prelude::*;
use proptest::test_runner::TestCaseResult;
use rujira_rs::fin::{
    Denoms, ExecuteMsg, OrderResponse, OrdersResponse, Price, QueryMsg, RangeConfig, RangeMsg,
    RangeResponse, RangesResponse, Side, SwapRequest,
};
use rujira_rs_testing::{mock_rujira_app, RujiraApp};

use crate::mock::Fin;

const BASE: &str = "rune";
const QUOTE: &str = "btc-btc";
const USER_COUNT: usize = 6;
const START_BALANCE: u128 = 2_000_000_000_000;

#[derive(Clone, Debug)]
enum Action {
    Swap {
        actor: u8,
        offer_base: bool,
        amount: u64,
        mode: u8,
        hint: u32,
    },
    Order {
        actor: u8,
        side_base: bool,
        price_raw: u32,
        target: Option<u64>,
    },
    RangeCreate {
        actor: u8,
        low: u32,
        width: u32,
        spread_permille: u16,
        fee_share_percent: u8,
        skew_milli: i16,
        base: u64,
        quote: u64,
    },
    RangeDeposit {
        actor: u8,
        idx: u16,
        base: u64,
        quote: u64,
    },
    RangeWithdraw {
        actor: u8,
        idx: u16,
        amount_permille: u16,
    },
    RangeClaim {
        actor: u8,
        idx: u16,
    },
    RangeClose {
        actor: u8,
        idx: u16,
    },
    RangeTransfer {
        actor: u8,
        idx: u16,
        to: u8,
    },
}

fn action_strategy() -> impl Strategy<Value = Action> {
    prop_oneof![
        (
            any::<u8>(),
            any::<bool>(),
            1u64..2_000_000u64,
            0u8..=3u8,
            1u32..200_000u32
        )
            .prop_map(|(actor, offer_base, amount, mode, hint)| Action::Swap {
                actor,
                offer_base,
                amount,
                mode,
                hint,
            }),
        (
            any::<u8>(),
            any::<bool>(),
            1u32..500_000u32,
            prop::option::of(1u64..2_000_000u64)
        )
            .prop_map(|(actor, side_base, price_raw, target)| Action::Order {
                actor,
                side_base,
                price_raw,
                target,
            }),
        (
            any::<u8>(),
            1u32..200_000u32,
            1u32..200_000u32,
            1u16..=850u16,
            0u8..=100u8,
            -900i16..=900i16,
            1u64..4_000_000u64,
            1u64..4_000_000u64
        )
            .prop_map(
                |(
                    actor,
                    low,
                    width,
                    spread_permille,
                    fee_share_percent,
                    skew_milli,
                    base,
                    quote,
                )| {
                    Action::RangeCreate {
                        actor,
                        low,
                        width,
                        spread_permille,
                        fee_share_percent,
                        skew_milli,
                        base,
                        quote,
                    }
                }
            ),
        (
            any::<u8>(),
            1u16..=600u16,
            1u64..1_000_000u64,
            1u64..1_000_000u64
        )
            .prop_map(|(actor, idx, base, quote)| Action::RangeDeposit {
                actor,
                idx,
                base,
                quote,
            }),
        (any::<u8>(), 1u16..=600u16, 1u16..=1000u16).prop_map(|(actor, idx, amount_permille)| {
            Action::RangeWithdraw {
                actor,
                idx,
                amount_permille,
            }
        }),
        (any::<u8>(), 1u16..=600u16).prop_map(|(actor, idx)| Action::RangeClaim { actor, idx }),
        (any::<u8>(), 1u16..=600u16).prop_map(|(actor, idx)| Action::RangeClose { actor, idx }),
        (any::<u8>(), 1u16..=600u16, any::<u8>())
            .prop_map(|(actor, idx, to)| Action::RangeTransfer { actor, idx, to }),
    ]
}

fn setup() -> (RujiraApp, Fin, Vec<Addr>) {
    let mut app = mock_rujira_app();
    let owner = app.api().addr_make("owner");
    let fees = app.api().addr_make("fees");
    let users: Vec<Addr> = (0..USER_COUNT)
        .map(|i| app.api().addr_make(&format!("user_{i}")))
        .collect();

    app.init_modules(|router, _, storage| {
        router
            .bank
            .init_balance(
                storage,
                &owner,
                vec![
                    coin(START_BALANCE * 10, BASE),
                    coin(START_BALANCE * 10, QUOTE),
                ],
            )
            .unwrap();
        for user in &users {
            router
                .bank
                .init_balance(
                    storage,
                    user,
                    vec![coin(START_BALANCE, BASE), coin(START_BALANCE, QUOTE)],
                )
                .unwrap();
        }
    });

    let fin = Fin::create(
        &mut app,
        &owner,
        &fees,
        &[],
        Denoms::new(BASE, QUOTE),
        None,
        Decimal::zero(),
    );

    (app, fin, users)
}

fn select_user(users: &[Addr], idx: u8) -> Addr {
    users[(idx as usize) % users.len()].clone()
}

fn side(side_base: bool) -> Side {
    if side_base {
        Side::Base
    } else {
        Side::Quote
    }
}

fn fixed_price(price_raw: u32) -> Price {
    Price::Fixed(Decimal::from_ratio((price_raw as u128) + 1, 100_000u128))
}

fn apply_action(app: &mut RujiraApp, fin: &Fin, users: &[Addr], action: &Action) {
    match action {
        Action::Swap {
            actor,
            offer_base,
            amount,
            mode,
            hint,
        } => {
            let sender = select_user(users, *actor);
            let denom = if *offer_base { BASE } else { QUOTE };
            let offer = Uint128::new(*amount as u128);

            let msg = match mode % 4 {
                0 => ExecuteMsg::Swap(SwapRequest::Yolo {
                    to: None,
                    callback: None,
                }),
                1 => {
                    let min_return = Uint128::new((offer.u128() * (*hint as u128) / 10_000) + 1);
                    ExecuteMsg::Swap(SwapRequest::Min {
                        min_return,
                        to: None,
                        callback: None,
                    })
                }
                2 => {
                    let exact_return = Uint128::new((offer.u128() * (*hint as u128) / 10_000) + 1);
                    ExecuteMsg::Swap(SwapRequest::Exact {
                        exact_return,
                        to: None,
                        callback: None,
                    })
                }
                _ => {
                    let limit = Decimal::from_ratio((*hint as u128) + 1, 100_000u128);
                    ExecuteMsg::Swap(SwapRequest::Limit {
                        price: limit,
                        to: None,
                        callback: None,
                    })
                }
            };

            let _ = app.execute_contract(
                sender,
                fin.addr().clone(),
                &msg,
                &[coin(*amount as u128, denom)],
            );
        }
        Action::Order {
            actor,
            side_base,
            price_raw,
            target,
        } => {
            let sender = select_user(users, *actor);
            let side = side(*side_base);
            let price = fixed_price(*price_raw);
            let funds = match target {
                Some(v) => vec![coin(*v as u128, if *side_base { BASE } else { QUOTE })],
                None => vec![],
            };
            let orders = vec![(side, price, target.map(|v| Uint128::new(v as u128)))];
            let _ = app.execute_contract(
                sender,
                fin.addr().clone(),
                &ExecuteMsg::Order((orders, None)),
                &funds,
            );
        }
        Action::RangeCreate {
            actor,
            low,
            width,
            spread_permille,
            fee_share_percent,
            skew_milli,
            base,
            quote,
        } => {
            let sender = select_user(users, *actor);
            let low_u128 = (*low as u128) + 1;
            let high_u128 = low_u128 + (*width as u128) + 1;
            let low = Decimal::from_atomics(low_u128, 0).unwrap();
            let high = Decimal::from_atomics(high_u128, 0).unwrap();
            let spread = Decimal::permille(*spread_permille as u64);
            let fee = Decimal::permille(
                ((*spread_permille as u64) * (*fee_share_percent as u64) / 100).max(1),
            );
            // Clamp to (-2, 2) exclusive: map i16 range to -1999..1999 milli
            let clamped = (*skew_milli).clamp(-1999, 1999);
            let abs = clamped.unsigned_abs();
            let skew_str = format!(
                "{}{}.{:03}",
                if clamped < 0 { "-" } else { "" },
                abs / 1000,
                abs % 1000
            );
            let skew = SignedDecimal::from_str(&skew_str).unwrap();

            let _ = app.execute_contract(
                sender,
                fin.addr().clone(),
                &ExecuteMsg::Range(RangeMsg::Create {
                    config: RangeConfig {
                        high,
                        low,
                        skew,
                        spread,
                        fee,
                    },
                    slippage: None,
                }),
                &[coin(*base as u128, BASE), coin(*quote as u128, QUOTE)],
            );
        }
        Action::RangeDeposit {
            actor,
            idx,
            base,
            quote,
        } => {
            let sender = select_user(users, *actor);
            let _ = app.execute_contract(
                sender,
                fin.addr().clone(),
                &ExecuteMsg::Range(RangeMsg::Deposit {
                    idx: Uint128::new(*idx as u128),
                }),
                &[coin(*base as u128, BASE), coin(*quote as u128, QUOTE)],
            );
        }
        Action::RangeWithdraw {
            actor,
            idx,
            amount_permille,
        } => {
            let sender = select_user(users, *actor);
            let _ = app.execute_contract(
                sender,
                fin.addr().clone(),
                &ExecuteMsg::Range(RangeMsg::Withdraw {
                    idx: Uint128::new(*idx as u128),
                    amount: Decimal::permille(*amount_permille as u64),
                }),
                &[],
            );
        }
        Action::RangeClaim { actor, idx } => {
            let sender = select_user(users, *actor);
            let _ = app.execute_contract(
                sender,
                fin.addr().clone(),
                &ExecuteMsg::Range(RangeMsg::Claim {
                    idx: Uint128::new(*idx as u128),
                }),
                &[],
            );
        }
        Action::RangeClose { actor, idx } => {
            let sender = select_user(users, *actor);
            let _ = app.execute_contract(
                sender,
                fin.addr().clone(),
                &ExecuteMsg::Range(RangeMsg::Close {
                    idx: Uint128::new(*idx as u128),
                }),
                &[],
            );
        }
        Action::RangeTransfer { actor, idx, to } => {
            let sender = select_user(users, *actor);
            let to = select_user(users, *to);
            let _ = app.execute_contract(
                sender,
                fin.addr().clone(),
                &ExecuteMsg::Range(RangeMsg::Transfer {
                    idx: Uint128::new(*idx as u128),
                    to: to.to_string(),
                }),
                &[],
            );
        }
    }
}

fn query_all_orders(app: &RujiraApp, fin: &Fin) -> Vec<OrderResponse> {
    let mut out = Vec::new();
    let mut cursor: Option<(String, Side, Price)> = None;

    loop {
        let OrdersResponse { orders } = app
            .wrap()
            .query_wasm_smart(
                fin.addr().clone(),
                &QueryMsg::Orders {
                    owner: None,
                    side: None,
                    start_after: cursor.clone(),
                    limit: Some(100),
                },
            )
            .unwrap();

        if orders.is_empty() {
            break;
        }

        cursor = orders
            .last()
            .map(|o| (o.owner.clone(), o.side.clone(), o.price.clone()));
        out.extend(orders);
    }

    out
}

fn query_all_ranges(app: &RujiraApp, fin: &Fin) -> Vec<RangeResponse> {
    let mut out = Vec::new();
    let mut cursor: Option<Uint128> = None;

    loop {
        let RangesResponse { ranges } = app
            .wrap()
            .query_wasm_smart(
                fin.addr().clone(),
                &QueryMsg::Ranges {
                    owner: None,
                    cursor,
                    limit: Some(100),
                },
            )
            .unwrap();

        if ranges.is_empty() {
            break;
        }

        cursor = ranges.last().map(|r| r.idx);
        out.extend(ranges);
    }

    out
}

fn solvency_snapshot(
    app: &RujiraApp,
    fin: &Fin,
) -> (
    Decimal,
    Decimal,
    Vec<RangeResponse>,
    Vec<OrderResponse>,
    Decimal,
    Decimal,
) {
    let balance_base = app
        .wrap()
        .query_balance(fin.addr().to_string(), BASE)
        .unwrap()
        .amount;
    let balance_quote = app
        .wrap()
        .query_balance(fin.addr().to_string(), QUOTE)
        .unwrap()
        .amount;

    let ranges = query_all_ranges(app, fin);
    let orders = query_all_orders(app, fin);

    let mut owed_base = ranges.iter().fold(Decimal::zero(), |acc, r| {
        acc + r.base.floor() + r.fees[0].floor()
    });
    let mut owed_quote = ranges.iter().fold(Decimal::zero(), |acc, r| {
        acc + r.quote.floor() + r.fees[1].floor()
    });

    for order in &orders {
        let remaining = Decimal::from_atomics(order.remaining, 0).unwrap();
        let filled = Decimal::from_atomics(order.filled, 0).unwrap();
        match order.side {
            Side::Base => {
                owed_base += remaining;
                owed_quote += filled;
            }
            Side::Quote => {
                owed_quote += remaining;
                owed_base += filled;
            }
        }
    }

    (
        Decimal::from_atomics(balance_base, 0).unwrap(),
        Decimal::from_atomics(balance_quote, 0).unwrap(),
        ranges,
        orders,
        owed_base,
        owed_quote,
    )
}

fn check_solvency(
    app: &RujiraApp,
    fin: &Fin,
    step: usize,
    action: Option<&Action>,
) -> TestCaseResult {
    // Range state keeps fractional dust in Decimal fields, but payouts are always
    // floored per bucket (`withdraw`, `claim`, `close`). Solvency should therefore
    // compare contract balances against the currently claimable integer liabilities.
    let (balance_base_dec, balance_quote_dec, _ranges, _orders, owed_base, owed_quote) =
        solvency_snapshot(app, fin);

    prop_assert!(
        balance_base_dec >= owed_base,
        "base insolvency at step {step}, action={action:?}, balance={balance_base_dec}, owed={owed_base}"
    );
    prop_assert!(
        balance_quote_dec >= owed_quote,
        "quote insolvency at step {step}, action={action:?}, balance={balance_quote_dec}, owed={owed_quote}"
    );

    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 96,
        max_shrink_iters: 50_000,
        ..ProptestConfig::default()
    })]

    #[test]
    fn e2e_solvency(
        actions in prop::collection::vec(action_strategy(), 5..100)
    ) {
        let (mut app, fin,  users) = setup();

        check_solvency(&app, &fin, 0, None)?;

        for (step, action) in actions.iter().enumerate() {
            apply_action(&mut app, &fin, &users, action);
            check_solvency(&app, &fin, step + 1, Some(action))?;
        }
    }
}
