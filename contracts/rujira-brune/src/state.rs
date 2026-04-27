use std::{
    cmp::Ordering,
    collections::BTreeSet,
    ops::{Add, Mul, Sub},
};

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Decimal, Deps, DepsMut, Env, Response, StdResult, Storage, Uint128};
use cw_storage_plus::Item;
use rujira_rs::brune::RevenueResponse;

use crate::{
    config::{Config, RUNE},
    events::event_warn,
    nodes::{Node, Nodes},
    revenue::Revenue,
    ContractError,
};

static STATE: Item<State> = Item::new("state");

#[cw_serde]
#[derive(Default)]
pub struct State {
    /// Total BRUNE minted.
    pub minted: Decimal,
    /// Whitelisted nodes
    nodes: BTreeSet<Addr>,

    pub revenue: Revenue,
}

impl State {
    pub fn init(storage: &mut dyn Storage) -> StdResult<()> {
        Self::default().save(storage)
    }

    pub fn load(storage: &dyn Storage) -> StdResult<Self> {
        STATE.load(storage)
    }

    pub fn save(&self, storage: &mut dyn Storage) -> StdResult<()> {
        STATE.save(storage, self)
    }

    pub fn deposit(&mut self, amount: Uint128) -> Result<Uint128, ContractError> {
        self.minted += Decimal::from_atomics(amount, 0)?;
        Ok(amount)
    }

    pub fn withdraw(&mut self, amount: Uint128) -> Result<Uint128, ContractError> {
        self.minted -= Decimal::from_atomics(amount, 0)?;
        Ok(amount)
    }

    pub fn register(
        &mut self,
        deps: Deps,
        env: &Env,
        config: &Config,
        addr: &Addr,
    ) -> Result<Option<Node>, ContractError> {
        match Node::load(deps.querier, env, config, addr)? {
            Some(v) => {
                self.nodes.insert(addr.clone());
                Ok(Some(v))
            }
            None => {
                self.nodes.remove(addr);
                Ok(None)
            }
        }
    }

    pub fn nodes(&self, deps: Deps, env: &Env, config: &Config) -> Result<Nodes, ContractError> {
        self.nodes.iter().try_fold(
            Nodes::default(),
            |agg, node| -> Result<Nodes, ContractError> {
                match Node::load(deps.querier, env, config, node)? {
                    Some(v) => Ok(agg.add(v)),
                    None => Ok(agg),
                }
            },
        )
    }

    /// Processes pending accounting allocation
    pub fn allocate(
        &mut self,
        deps: DepsMut,
        env: &Env,
        config: &Config,
        response: Response,
        received: Option<Decimal>,
    ) -> Result<Response, ContractError> {
        let mut response = response;
        let signer = deps.api.addr_canonicalize(env.contract.address.as_str())?;

        let mut nodes = self.nodes(deps.as_ref(), env, config)?;
        let bonded = nodes.bonded();
        let balance = Self::balance(deps.as_ref(), env, received)?;

        // Step 1: Calculate any new revenue and execute revenue distribution
        // 1. Calculate total RUNE ownership - balance + bonded
        let ownership = balance.add(bonded);
        // 2. Determine surplus: ownership - minted
        let surplus = match ownership.checked_sub(self.minted) {
            Ok(v) => v,
            Err(_) => {
                // Handle case where Nodes can be slashed.
                // Shortfall will be covered at next churn
                response = response.add_event(event_warn(&format!(
                    "ownership < minted; {ownership} < {}",
                    self.minted
                )));
                // Halt revenue streaming during deficit
                self.revenue.pending = Decimal::zero();
                self.revenue.fee_rate = Decimal::zero();
                Decimal::zero()
            }
        };

        // 3. Determine if any of that is new surplus: surplus - pending
        let delta = surplus.saturating_sub(self.revenue.pending);
        // 4. Distribute what revenue we can, given current balance, prior to allocating new delta
        let (response, distributed) =
            self.revenue
                .distribute(&env.block.time, &config.stake_contract, response, balance)?;

        // 5. Allocate new surplus for next iteration
        self.revenue
            .allocate(&env.block.time, config.revenue_smear, &delta);

        let response = response
            .add_event(crate::events::event_fee_allocate(deps.storage, env, delta)?)
            .add_event(crate::events::event_fee_distribute(
                deps.storage,
                env,
                distributed,
            )?);

        // Step 2: Rebalance bond
        let target = self.minted.mul(config.target_utilization);
        match target.cmp(&bonded) {
            // Correct bond, nothing to do
            Ordering::Equal => Ok(response),
            // Underbonded, pro-rate the excess across whitelisted nodes
            // NB. We can reach this having previously hit request_bond if eg we needed to request funds to pay revenue
            // The request_bond may have requested a churn out that we _could_ cancel here. However given the request_bond
            // function selects the most over-bonded node vs target allocation, we should leave these to churn out, and
            // ultimately re-bond with the correct amount on the first contract execution following churn
            Ordering::Greater => {
                nodes.allocate_bond(deps.storage, env, &signer, target.sub(bonded), response)
            }
            // Overbonded, select a node to request a churn out, if required and if possible
            Ordering::Less => nodes.request_bond(deps, env, &signer, &bonded.sub(target), response),
        }
    }

    fn balance(deps: Deps, env: &Env, received: Option<Decimal>) -> Result<Decimal, ContractError> {
        let bal = Decimal::from_atomics(
            deps.querier
                .query_balance(&env.contract.address, RUNE)?
                .amount,
            0,
        )?;
        Ok(bal.sub(received.unwrap_or_default()))
    }
    /// Returns liquid RUNE available to support bid-side liquidity.
    pub fn quote_liquidity(
        &self,
        deps: Deps,
        env: &Env,
        config: &Config,
        received: Option<Decimal>,
    ) -> Result<Decimal, ContractError> {
        let balance = Self::balance(deps, env, received)?;
        let bonded = self.nodes(deps, env, config)?.bonded();
        let ownership = balance + bonded;
        let surplus = ownership.saturating_sub(self.minted);
        let effective_pending = self.revenue.pending.max(surplus);

        // saturating_sub here as liquid balance can fall below pending revenue.
        // At this point, revenue takes priority and a request_bond should be hit
        Ok(balance.saturating_sub(effective_pending))
    }
}

impl From<Revenue> for RevenueResponse {
    fn from(value: Revenue) -> Self {
        Self::from(&value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::revenue::Revenue;
    use cosmwasm_std::Binary;
    use cosmwasm_std::{
        coin, from_json, testing::mock_env, Addr, ContractResult, Decimal, Empty, QueryRequest,
        SystemError, SystemResult, Uint128,
    };
    use cosmwasm_std::{
        testing::{BankQuerier, MockApi, MockStorage},
        OwnedDeps, Querier,
    };
    use proptest::prelude::*;
    use prost::Message;
    use rujira_rs::proto::types::{
        NodeBondProvider, NodeBondProviders, QueryMimirWithKeyResponse, QueryNodeRequest,
        QueryNodeResponse,
    };
    use std::{collections::BTreeMap, marker::PhantomData};

    #[derive(Default)]
    struct TestQuerier {
        bank: BankQuerier,
        nodes: BTreeMap<String, QueryNodeResponse>,
        mimir_value: i64,
    }

    impl Querier for TestQuerier {
        fn raw_query(&self, bin_request: &[u8]) -> SystemResult<ContractResult<Binary>> {
            let request: QueryRequest<Empty> = match from_json(bin_request) {
                Ok(v) => v,
                Err(e) => {
                    return SystemResult::Err(SystemError::InvalidRequest {
                        error: format!("Parsing query request: {e}"),
                        request: bin_request.into(),
                    })
                }
            };
            match request {
                QueryRequest::Bank(bank) => self.bank.query(&bank),
                QueryRequest::Grpc(grpc) => match grpc.path.as_str() {
                    "/types.Query/Node" => {
                        let req = QueryNodeRequest::decode(grpc.data.as_slice()).unwrap();
                        let response = match self.nodes.get(&req.address) {
                            Some(response) => response.clone(),
                            None => return SystemResult::Err(SystemError::Unknown {}),
                        };
                        let mut buf = Vec::new();
                        response.encode(&mut buf).unwrap();
                        SystemResult::Ok(ContractResult::Ok(buf.into()))
                    }
                    "/types.Query/MimirWithKey" => {
                        let response = QueryMimirWithKeyResponse {
                            value: self.mimir_value,
                        };
                        let mut buf = Vec::new();
                        response.encode(&mut buf).unwrap();
                        SystemResult::Ok(ContractResult::Ok(buf.into()))
                    }
                    _ => SystemResult::Err(SystemError::UnsupportedRequest {
                        kind: "GRPC".to_string(),
                    }),
                },
                _ => SystemResult::Err(SystemError::UnsupportedRequest {
                    kind: "query".to_string(),
                }),
            }
        }
    }

    fn deps_with_state(
        balance: Uint128,
        nodes: BTreeMap<String, QueryNodeResponse>,
        mimir_value: i64,
        api: MockApi,
        contract: Addr,
    ) -> OwnedDeps<MockStorage, MockApi, TestQuerier, Empty> {
        let bank = BankQuerier::new(&[(contract.as_str(), &[coin(balance.u128(), RUNE)])]);
        OwnedDeps {
            storage: MockStorage::new(),
            api,
            querier: TestQuerier {
                bank,
                nodes,
                mimir_value,
            },
            custom_query_type: PhantomData,
        }
    }

    fn status_from_idx(idx: u8) -> &'static str {
        match idx % 6 {
            0 => "Unknown",
            1 => "Whitelisted",
            2 => "Standby",
            3 => "Ready",
            4 => "Active",
            _ => "Disabled",
        }
    }

    proptest! {
        #[test]
        fn allocate_preserves_minted_bonded_liquid_invariant(
            balance in 0u128..100_000u128,
            pending in 0u128..100_000u128,
            nodes in prop::collection::vec((0u128..10_000u128, 0u8..30u8, any::<bool>(), any::<bool>(), 0u8..6u8), 1..6),
            target_utilization in 0u8..=100u8,
            mimir_value in -1i64..100i64,
        ) {
            prop_assume!(pending <= balance);

            let api = MockApi::default();
            let contract_addr = api.addr_make("contract");
            let mut node_map = BTreeMap::new();
            let mut node_addrs = Vec::new();
            let mut bonded = Decimal::zero();
            let contract_addr_str = contract_addr.to_string();

            for (idx, (bond, fee, requested, forced, status_idx)) in nodes.iter().enumerate() {
                let addr = api.addr_make(&format!("node-{}", idx));
                node_addrs.push(addr.clone());
                let addr_str = addr.to_string();
                bonded += Decimal::from_ratio(*bond, 1u128);
                let response = QueryNodeResponse {
                    node_address: addr_str.clone(),
                    status: status_from_idx(*status_idx).to_string(),
                    bond_providers: Some(NodeBondProviders {
                        node_operator_fee: ((*fee as u64) * 100).to_string(),
                        providers: vec![NodeBondProvider {
                            bond_address: contract_addr_str.clone(),
                            bond: bond.to_string(),
                        }],
                    }),
                    requested_to_leave: *requested,
                    forced_to_leave: *forced,
                    ..QueryNodeResponse::default()
                };
                node_map.insert(addr_str, response);
            }

            let pending_dec = Decimal::from_ratio(pending, 1u128);
            let balance_dec = Decimal::from_ratio(balance, 1u128);
            let liquid = balance_dec - pending_dec;
            let minted = bonded + liquid;

            let mut state = State {
                minted,
                nodes: node_addrs.into_iter().collect(),
                revenue: Revenue { pending: pending_dec, ..Revenue::default() },
            };

            let config = Config {
                target_utilization: Decimal::percent(target_utilization as u64),
                min_node_fee: Decimal::percent(1),
                range: rujira_rs::brune::RangeConfig {
                    high: Decimal::percent(99),
                    low: Decimal::percent(80),
                    skew: cosmwasm_std::SignedDecimal::zero(),
                    step: Decimal::percent(1),
                },
                permissionless: false,
                fin_contract: api.addr_make("fin"),
                stake_contract: api.addr_make("stake"),
                revenue_smear: 1,
                token_id: "brune".to_string(),
                max_bond: Uint128::new(1_000_000),
                mint_cap: Uint128::new(10_000_000),
            };

            let mut deps = deps_with_state(
                Uint128::new(balance),
                node_map,
                mimir_value,
                api,
                contract_addr.clone(),
            );
            let mut env = mock_env();
            env.contract.address = contract_addr;

            state.allocate(deps.as_mut(), &env, &config, Response::default(), None).unwrap();

            let nodes = state.nodes(deps.as_ref(), &env, &config).unwrap();
            let bonded_after = nodes.bonded();
            let liquid_after = balance_dec - state.revenue.pending;

            prop_assert_eq!(state.minted, bonded_after + liquid_after);
        }
    }
}
