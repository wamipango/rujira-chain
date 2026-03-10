use std::str::FromStr;

use cosmwasm_std::{coins, Addr, Coin, Decimal, SignedDecimal, Uint128};
use cw_multi_test::{ContractWrapper, Executor};

use rujira_rs::{
    fin::{
        BookResponse, Denoms, ExecuteMsg, InstantiateMsg, OrderResponse, Price, QueryMsg,
        RangeConfig, RangeMsg, Side, SwapRequest, Tick,
    },
    OraclePair,
};
use rujira_rs_testing::RujiraApp;

use crate::contract::{execute, instantiate, query, sudo};

/// Wrapper struct for FIN contract with convenience methods
#[derive(Debug, Clone)]
pub struct Fin(pub Addr);

impl Fin {
    /// Get the contract address
    pub fn addr(&self) -> &Addr {
        &self.0
    }

    /// Execute a swap through FIN
    pub fn swap(
        &self,
        app: &mut RujiraApp,
        sender: &Addr,
        offer_amount: u128,
        offer_denom: &str,
        min_return: Option<Uint128>,
    ) -> anyhow::Result<cw_multi_test::AppResponse> {
        app.execute_contract(
            sender.clone(),
            self.0.clone(),
            &ExecuteMsg::Swap(match min_return {
                Some(min_return) => SwapRequest::Min {
                    to: None,
                    callback: None,
                    min_return,
                },
                None => SwapRequest::Yolo {
                    to: None,
                    callback: None,
                },
            }),
            &coins(offer_amount, offer_denom),
        )
    }

    /// Place an order on FIN
    pub fn place_order(
        &self,
        app: &mut RujiraApp,
        sender: &Addr,
        side: Side,
        price: Price,
        size: Option<Uint128>,
        funds: &[Coin],
    ) -> anyhow::Result<cw_multi_test::AppResponse> {
        app.execute_contract(
            sender.clone(),
            self.0.clone(),
            &ExecuteMsg::Order((vec![(side, price, size)], None)),
            funds,
        )
    }

    /// Place an order on FIN
    pub fn place_orders(
        &self,
        app: &mut RujiraApp,
        sender: &Addr,
        orders: Vec<(Side, Price, Option<Uint128>)>,
        funds: &[Coin],
    ) -> anyhow::Result<cw_multi_test::AppResponse> {
        app.execute_contract(
            sender.clone(),
            self.0.clone(),
            &ExecuteMsg::Order((orders, None)),
            funds,
        )
    }

    /// Query the order book
    pub fn query_book(
        &self,
        app: &RujiraApp,
        limit: Option<u8>,
        offset: Option<u8>,
    ) -> anyhow::Result<BookResponse> {
        Ok(app
            .wrap()
            .query_wasm_smart(self.0.clone(), &QueryMsg::Book { limit, offset })?)
    }

    /// Query a specific order
    pub fn query_order(
        &self,
        app: &RujiraApp,
        owner: &str,
        side: Side,
        price: Price,
    ) -> anyhow::Result<OrderResponse> {
        Ok(app.wrap().query_wasm_smart(
            self.0.clone(),
            &QueryMsg::Order((owner.to_string(), side, price)),
        )?)
    }

    /// Query orders for an owner
    pub fn query_orders(
        &self,
        app: &RujiraApp,
        owner: &str,
        side: Option<Side>,
        limit: Option<u8>,
        offset: Option<u8>,
    ) -> anyhow::Result<rujira_rs::fin::OrdersResponse> {
        Ok(app.wrap().query_wasm_smart(
            self.0.clone(),
            &QueryMsg::Orders {
                owner: owner.to_string(),
                side,
                offset,
                limit,
            },
        )?)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_range(
        &self,
        app: &mut RujiraApp,
        sender: &Addr,
        (high, low): (&str, &str),
        spread: &str,
        fee: &str,
        skew: &str,
        funds: &[Coin],
    ) -> anyhow::Result<cw_multi_test::AppResponse> {
        app.execute_contract(
            sender.clone(),
            self.0.clone(),
            &ExecuteMsg::Range(RangeMsg::Create {
                config: RangeConfig {
                    high: Decimal::from_str(high).unwrap(),
                    low: Decimal::from_str(low).unwrap(),
                    skew: SignedDecimal::from_str(skew).unwrap(),
                    spread: Decimal::from_str(spread).unwrap(),
                    fee: Decimal::from_str(fee).unwrap(),
                },
                slippage: None,
            }),
            funds,
        )
    }

    pub fn claim_range(
        &self,
        app: &mut RujiraApp,
        sender: &Addr,
        idx: &Uint128,
    ) -> anyhow::Result<cw_multi_test::AppResponse> {
        app.execute_contract(
            sender.clone(),
            self.0.clone(),
            &ExecuteMsg::Range(RangeMsg::Claim { idx: *idx }),
            &[],
        )
    }

    pub fn close_range(
        &self,
        app: &mut RujiraApp,
        sender: &Addr,
        idx: &Uint128,
    ) -> anyhow::Result<cw_multi_test::AppResponse> {
        app.execute_contract(
            sender.clone(),
            self.0.clone(),
            &ExecuteMsg::Range(RangeMsg::Close { idx: *idx }),
            &[],
        )
    }

    pub fn query_range(
        &self,
        app: &RujiraApp,
        idx: &Uint128,
    ) -> anyhow::Result<rujira_rs::fin::RangeResponse> {
        Ok(app
            .wrap()
            .query_wasm_smart(self.0.clone(), &QueryMsg::Range(*idx))?)
    }
    /// Setup FIN contract
    pub fn create(
        app: &mut RujiraApp,
        owner: &Addr,
        fees: &Addr,
        market_makers: &[Addr],
        denoms: Denoms,
        oracles: Option<OraclePair>,
        fee_amm: Decimal,
    ) -> Self {
        let fin_code = Box::new(ContractWrapper::new(execute, instantiate, query).with_sudo(sudo));
        let fin_code_id = app.store_code(fin_code);

        let fin_addr = app
            .instantiate_contract(
                fin_code_id,
                owner.clone(),
                &InstantiateMsg {
                    denoms,
                    oracles,
                    market_makers: market_makers.iter().map(|x| x.to_string()).collect(),
                    tick: Tick::new(8),
                    fee_taker: Decimal::permille(15), // 1.5%
                    fee_maker: Decimal::permille(30), // 3.0%
                    fee_amm,
                    fee_address: fees.to_string(),
                    range_delta: Decimal::permille(50),
                },
                &[],
                "fin",
                None,
            )
            .unwrap();

        Fin(fin_addr)
    }
}

#[cfg(test)]
mod tests {
    use rujira_rs_testing::mock_rujira_app;

    use super::*;

    #[test]
    fn test_fin_setup() {
        let mut app = mock_rujira_app();
        let owner = app.api().addr_make("owner");
        let fees = app.api().addr_make("fees");

        let fin = Fin::create(
            &mut app,
            &owner.clone(),
            &fees,
            &[owner],
            Denoms::new("btc", "usdc"),
            None,
            Decimal::permille(0),
        );

        // Verify both contracts were created
        assert!(!fin.addr().as_str().is_empty());
    }
}
