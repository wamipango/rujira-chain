use std::{
    cmp::{max, min},
    ops::Add,
    str::FromStr,
};

use cosmwasm_std::{
    Addr, CanonicalAddr, Decimal, DepsMut, Env, QuerierWrapper, Response, StdResult, Storage,
    SubMsg, Uint128,
};
use rujira_rs::{
    brune::{NodeResponse, NodeStatus, NodesResponse},
    msg::bond::{MsgBond, MsgLeave, MsgUnbond},
    proto::types::{
        QueryMimirWithKeyRequest, QueryMimirWithKeyResponse, QueryNodeRequest, QueryNodeResponse,
    },
    query::grpc::Queryable,
};

use crate::{
    config::Config,
    events::{event_bond, event_leave, event_unbond},
    ContractError,
};

/// Convenience struct to load all whitelisted nodes and perform aggregate queries over them
#[derive(Default, Debug)]
pub struct Nodes {
    bond: Decimal,
    weight: Decimal,
    capacity: Decimal,
    nodes: Vec<Node>,
}

impl Nodes {
    pub fn bonded(&self) -> Decimal {
        self.bond
    }

    pub fn capacity(&self) -> Decimal {
        self.capacity
    }

    /// Unbonds `target` from `self.nodes`, returning the total amount successfully unbonded
    fn unbond(
        &self,
        storage: &mut dyn Storage,
        env: &Env,
        signer: &CanonicalAddr,
        response: &mut Response,
        target: &Decimal,
    ) -> StdResult<Decimal> {
        let mut remaining = *target;
        // We're already sorted by overbond, allocate available and add the MsgUnbond to the response for the amount
        for n in &self.nodes {
            match n.status {
                NodeStatus::Unknown => continue,
                NodeStatus::Ready => continue,
                NodeStatus::Active => continue,
                _ => {
                    if remaining.is_zero() {
                        break;
                    }

                    let alloc = min(remaining, n.bond);
                    if alloc.lt(&Decimal::one()) {
                        continue;
                    }

                    remaining -= alloc;

                    // NB we emit these as fire-and-forget messages as if a node is a member of a retiring vault then
                    // unbond will fail until the vault is retired. Querying currnetly requires a cross reference of pubkey
                    // and vaults list
                    // The impact of this will be that the total returned as successfully unbonded will be higher than actual,
                    // potentially skipping a churn out request
                    response.messages.push(SubMsg::reply_on_error(
                        MsgUnbond::new(n.addr.clone(), alloc.to_uint_floor(), None, signer),
                        1,
                    ));
                    response.events.push(event_unbond(
                        storage,
                        env,
                        &n.addr,
                        alloc.to_uint_floor(),
                    )?);
                }
            }
        }
        Ok(target - remaining)
    }

    /// Select the first node with sufficient bond to allow us to request && cover the required amount
    fn churn_out_candidate(&self, min_bond: &Decimal, required: &Decimal) -> Option<&Node> {
        self.nodes.iter().find(|v| {
            v.status == NodeStatus::Active && !v.is_leaving && v.bond >= max(*min_bond, *required)
        })
    }

    /// Sort nodes based on their over-bonded status. We want to prefer unbond & churn out from
    /// more over-bonded nodes
    fn sort_nodes_over(&mut self) {
        if self.bond.is_zero() || self.weight.is_zero() {
            return;
        }
        self.nodes.sort_by(|x, y| {
            // Compare the actual vs target weights, favour unbonding of the overbonded node
            // Correctly bonded nodes should have a value of 1 where their bond ratio is the correct weight ratio
            // Over-bonded will grow > 1 as their share of bond is greater than their share of weight
            let a = x
                .bond
                .checked_div(self.bond)
                .unwrap_or_default()
                .checked_div(x.weight.checked_div(self.weight).unwrap_or_default())
                .unwrap_or_default();
            let b = y
                .bond
                .checked_div(self.bond)
                .unwrap_or_default()
                .checked_div(y.weight.checked_div(self.weight).unwrap_or_default())
                .unwrap_or_default();
            b.cmp(&a)
        });
    }

    /// As sort_nodes_over but sorted underbonded first
    fn sort_nodes_under(&mut self) {
        if self.bond.is_zero() || self.weight.is_zero() {
            return;
        }
        self.nodes.sort_by(|x, y| {
            let a = x
                .bond
                .checked_div(self.bond)
                .unwrap_or_default()
                .checked_div(x.weight.checked_div(self.weight).unwrap_or_default())
                .unwrap_or_default();
            let b = y
                .bond
                .checked_div(self.bond)
                .unwrap_or_default()
                .checked_div(y.weight.checked_div(self.weight).unwrap_or_default())
                .unwrap_or_default();
            a.cmp(&b)
        });
    }

    /// Total amount of bond that is already scheduled to churn out
    fn churning_out(&self) -> Decimal {
        self.nodes.iter().fold(Decimal::zero(), |agg, v| {
            if v.is_leaving {
                agg + v.bond
            } else {
                agg
            }
        })
    }

    /// Allocates `amount` RUNE across Nodes
    /// We maximally allocate `amount` to nodes in order underbonded to overbonded
    pub fn allocate_bond(
        &mut self,
        storage: &mut dyn Storage,
        env: &Env,
        signer: &CanonicalAddr,
        amount: Decimal,
        response: Response,
    ) -> Result<Response, ContractError> {
        let mut response = response;
        self.sort_nodes_under();
        let mut remaining = min(amount, self.capacity);
        for n in &self.nodes {
            if n.status != NodeStatus::Active || n.is_leaving {
                continue;
            }

            if remaining.lt(&Decimal::one()) {
                break;
            }

            let allocation = min(remaining, n.capacity).to_uint_floor();
            if allocation.is_zero() {
                continue;
            }
            remaining -= Decimal::from_ratio(allocation, 1u128);
            response = response
                .add_message(MsgBond::new(n.addr.clone(), allocation, signer.clone()))
                .add_event(event_bond(storage, env, &n.addr, allocation)?);
        }
        Ok(response)
    }

    /// Inverse of allocate_bond. Attempts to free up funds
    pub fn request_bond(
        &mut self,
        deps: DepsMut,
        env: &Env,
        signer: &CanonicalAddr,
        amount: &Decimal,
        response: Response,
    ) -> Result<Response, ContractError> {
        let min = QueryMimirWithKeyResponse::get(
            deps.querier,
            QueryMimirWithKeyRequest {
                key: "MinimumBondInRune".to_string(),
                height: "".to_string(),
            },
        )?
        .value;
        // We shouldn't hit this, but check for safety
        if min < 0i64 {
            return Ok(response);
        }
        let min = Decimal::from_atomics(min.abs_diff(0), 0)?;
        self.sort_nodes_over();
        let mut remaining = *amount;
        let mut response = response;
        remaining -= self.unbond(deps.storage, env, signer, &mut response, amount)?;
        remaining -= self.churning_out().min(remaining);
        // Finally, select a node where we have > min bond threshold such that we can request a churn out

        if remaining.lt(&Decimal::one()) {
            return Ok(response);
        }
        match self.churn_out_candidate(&min, &remaining) {
            Some(node) => Ok(response
                .add_message(MsgLeave::new(node.addr.clone(), signer, Uint128::one()))
                .add_event(event_leave(deps.storage, env, &node.addr)?)),
            None => Ok(response),
        }
    }
}

impl From<Nodes> for NodesResponse {
    fn from(value: Nodes) -> Self {
        Self {
            bond: value.bond,
            weight: value.weight,
            capacity: value.capacity,
            nodes: value.nodes.iter().map(NodeResponse::from).collect(),
        }
    }
}

impl Add<Node> for Nodes {
    type Output = Self;

    fn add(self, rhs: Node) -> Self::Output {
        let mut nodes = self.nodes;
        let bond = self.bond + rhs.bond;
        let weight = self.weight + rhs.weight;
        let capacity = self.capacity + rhs.capacity;
        nodes.push(rhs);
        Self {
            bond,
            weight,
            nodes,
            capacity,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Node {
    pub addr: Addr,
    /// The fee charged by the node operator
    pub fee: Decimal,
    /// The amount of bond on this Node that we own
    pub bond: Decimal,
    /// The allocation weight
    pub weight: Decimal,
    /// The remaining bond capacity
    pub capacity: Decimal,
    pub is_leaving: bool,
    pub status: NodeStatus,

    pub res: QueryNodeResponse,
}

impl Node {
    pub fn load(
        q: QuerierWrapper,
        env: &Env,
        config: &Config,
        addr: &Addr,
    ) -> Result<Option<Self>, ContractError> {
        let response = QueryNodeResponse::get(
            q,
            QueryNodeRequest {
                address: addr.to_string(),
                height: "".to_string(),
            },
        )?;

        Ok(Self::from_response(env, config, addr, response))
    }

    /// Calculates the weight allocated to this Node to determine
    /// the proportion of total bond it will receive
    fn weight(config: &Config, fee: &Decimal) -> Decimal {
        Decimal::one() - max(config.min_node_fee, *fee).pow(3)
    }

    fn from_response(
        env: &Env,
        config: &Config,
        addr: &Addr,
        res: QueryNodeResponse,
    ) -> Option<Self> {
        let p = res.bond_providers.clone()?;
        let provider = p
            .providers
            .iter()
            .find(|x| x.bond_address == env.contract.address.to_string())?;
        let fee = Decimal::from_ratio(u64::from_str(&p.node_operator_fee).ok()?, 10_000u64);
        let bond = Decimal::from_str(&provider.bond).ok()?;
        let max_bond = Decimal::from_ratio(config.max_bond.u128(), 1u128);
        Some(Self {
            addr: addr.clone(),
            weight: Self::weight(config, &fee),
            fee,
            capacity: max_bond.saturating_sub(bond),
            bond,
            is_leaving: res.forced_to_leave || res.requested_to_leave,
            status: NodeStatus::from_str(&res.status).unwrap_or(NodeStatus::Unknown),
            res,
        })
    }
}

impl From<&Node> for NodeResponse {
    fn from(value: &Node) -> Self {
        Self {
            addr: value.addr.to_string(),
            fee: value.fee,
            bond: value.bond,
            weight: value.weight,
            capacity: value.capacity,
            is_leaving: value.is_leaving,
            status: value.status.clone(),
            node: value.res.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use anybuf::Bufany;
    use cosmwasm_std::testing::mock_dependencies;
    use cosmwasm_std::{
        from_json, testing::mock_env, Addr, CanonicalAddr, ContractResult, Decimal, Empty,
        QueryRequest, Response, SystemError, SystemResult, Uint128,
    };
    use cosmwasm_std::{
        testing::{MockApi, MockStorage},
        AnyMsg, Binary, CosmosMsg, OwnedDeps, Querier,
    };
    use proptest::prelude::*;
    use prost::Message;
    use rujira_rs::proto::types::{NodeBondProvider, NodeBondProviders, QueryMimirWithKeyResponse};
    use std::marker::PhantomData;
    use std::str::FromStr;

    use super::*;

    fn node(addr: &str, bond: Decimal, weight: Decimal, is_leaving: bool) -> Node {
        Node {
            addr: Addr::unchecked(addr),
            fee: Decimal::zero(),
            bond,
            weight,
            capacity: Decimal::from_ratio(1000u128, 1u128),
            is_leaving,
            status: NodeStatus::Active,
            res: Default::default(),
        }
    }

    fn deps_with_mimir(value: i64) -> OwnedDeps<MockStorage, MockApi, GrpcMockQuerier, Empty> {
        OwnedDeps {
            storage: MockStorage::new(),
            api: MockApi::default(),
            querier: GrpcMockQuerier { mimir_value: value },
            custom_query_type: PhantomData,
        }
    }

    #[derive(Default)]
    struct GrpcMockQuerier {
        mimir_value: i64,
    }

    impl Querier for GrpcMockQuerier {
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
                QueryRequest::Grpc(grpc) => match grpc.path.as_str() {
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

    #[test]
    fn sort_nodes_orders_by_overbond() {
        // Target 6 / 20 = 0.3
        // Actual 120 / 200 = 0.6
        // 2x
        let n1 = node(
            "node-1",
            Decimal::from_ratio(120u128, 1u128),
            Decimal::from_ratio(6u128, 1u128),
            false,
        );
        // Target 8 / 20 = 0.4
        // Actual 60 / 200 = 0.3
        // 0.75x
        let n2 = node(
            "node-2",
            Decimal::from_ratio(60u128, 1u128),
            Decimal::from_ratio(8u128, 1u128),
            false,
        );
        // Target 6 / 20 = 0.3
        // Actual 20 / 200 = 0.1
        // 0.33x
        let n3 = node(
            "node-3",
            Decimal::from_ratio(20u128, 1u128),
            Decimal::from_ratio(6u128, 1u128),
            false,
        );

        let mut nodes = Nodes {
            bond: Decimal::from_ratio(200u128, 1u128),
            weight: Decimal::from_ratio(20u128, 1u128),
            capacity: Decimal::MAX,
            nodes: vec![n2.clone(), n3.clone(), n1.clone()],
        };

        nodes.sort_nodes_over();

        let ordered: Vec<String> = nodes.nodes.iter().map(|n| n.addr.to_string()).collect();
        assert_eq!(ordered, vec!["node-1", "node-2", "node-3"]);
    }

    #[test]
    fn bonded_capacity_weight_and_list() {
        let n1 = node(
            "node-1",
            Decimal::from_ratio(10u128, 1u128),
            Decimal::from_ratio(2u128, 1u128),
            false,
        );
        let n2 = node(
            "node-2",
            Decimal::from_ratio(30u128, 1u128),
            Decimal::from_ratio(5u128, 1u128),
            true,
        );
        let nodes = Nodes {
            bond: n1.bond + n2.bond,
            weight: n1.weight + n2.weight,
            capacity: Decimal::from_ratio(7u128, 1u128),
            nodes: vec![n1.clone(), n2.clone()],
        };

        assert_eq!(nodes.bonded(), Decimal::from_ratio(40u128, 1u128));
        assert_eq!(nodes.capacity(), Decimal::from_ratio(7u128, 1u128));
    }

    #[test]
    fn add_node_and_nodes_response() {
        let n1 = node(
            "node-1",
            Decimal::from_ratio(10u128, 1u128),
            Decimal::from_ratio(2u128, 1u128),
            false,
        );
        let n2 = node(
            "node-2",
            Decimal::from_ratio(30u128, 1u128),
            Decimal::from_ratio(5u128, 1u128),
            true,
        );
        let nodes = Nodes::default() + n1.clone() + n2.clone();
        let response: NodesResponse = nodes.into();
        assert_eq!(response.nodes.len(), 2);
        assert_eq!(response.nodes[0].addr, n1.addr.to_string());
        assert_eq!(response.nodes[1].addr, n2.addr.to_string());
        assert!(response.nodes[1].is_leaving);
    }

    #[test]
    fn weight_uses_max_of_min_fee_and_node_fee() {
        let config = Config {
            target_utilization: Decimal::one(),
            min_node_fee: Decimal::percent(5),
            spread: Decimal::percent(1),
            permissionless: false,
            fin_contract: Addr::unchecked("fin"),
            stake_contract: Addr::unchecked("stake"),
            revenue_smear: 1,
            token_id: "brune".to_string(),
            max_bond: Uint128::new(100),
            mint_cap: Uint128::new(1_000),
        };
        let fee_low = Decimal::percent(1);
        let fee_high = Decimal::percent(10);

        let low_weight = Node::weight(&config, &fee_low);
        let high_weight = Node::weight(&config, &fee_high);
        let expected_low = Decimal::one() - config.min_node_fee.pow(3);
        let expected_high = Decimal::one() - fee_high.pow(3);

        assert_eq!(low_weight, expected_low);
        assert_eq!(high_weight, expected_high);
    }

    #[test]
    fn from_response_requires_provider() {
        let env = mock_env();
        let config = Config {
            target_utilization: Decimal::one(),
            min_node_fee: Decimal::percent(5),
            spread: Decimal::percent(1),
            permissionless: false,
            fin_contract: Addr::unchecked("fin"),
            stake_contract: Addr::unchecked("stake"),
            revenue_smear: 1,
            token_id: "brune".to_string(),
            max_bond: Uint128::new(100),
            mint_cap: Uint128::new(1_000),
        };
        let addr = Addr::unchecked("node");
        let res = QueryNodeResponse {
            node_address: addr.to_string(),
            ..QueryNodeResponse::default()
        };
        assert!(Node::from_response(&env, &config, &addr, res).is_none());
    }

    #[test]
    fn from_response_builds_node() {
        let mut env = mock_env();
        env.contract.address = Addr::unchecked("contract");
        let config = Config {
            target_utilization: Decimal::one(),
            min_node_fee: Decimal::percent(5),
            spread: Decimal::percent(1),
            permissionless: false,
            fin_contract: Addr::unchecked("fin"),
            stake_contract: Addr::unchecked("stake"),
            revenue_smear: 1,
            token_id: "brune".to_string(),
            max_bond: Uint128::new(100),
            mint_cap: Uint128::new(1_000),
        };
        let addr = Addr::unchecked("node");
        let res = QueryNodeResponse {
            node_address: addr.to_string(),
            status: "Active".to_string(),
            bond_providers: Some(NodeBondProviders {
                node_operator_fee: "500".to_string(),
                providers: vec![NodeBondProvider {
                    bond_address: env.contract.address.to_string(),
                    bond: "25".to_string(),
                }],
            }),
            requested_to_leave: true,
            forced_to_leave: false,
            ..QueryNodeResponse::default()
        };

        let node = Node::from_response(&env, &config, &addr, res).unwrap();
        assert_eq!(node.addr, addr);
        assert_eq!(node.bond, Decimal::from_ratio(25u128, 1u128));
        assert!(node.is_leaving);
        assert_eq!(node.status, NodeStatus::Active);
    }

    #[test]
    fn churning_out_sums_leaving_bonds() {
        let n1 = node(
            "node-1",
            Decimal::from_ratio(10u128, 1u128),
            Decimal::from_ratio(2u128, 1u128),
            true,
        );
        let n2 = node(
            "node-2",
            Decimal::from_ratio(5u128, 1u128),
            Decimal::from_ratio(2u128, 1u128),
            true,
        );
        let n3 = node(
            "node-3",
            Decimal::from_ratio(7u128, 1u128),
            Decimal::from_ratio(2u128, 1u128),
            false,
        );
        let nodes = Nodes {
            bond: n1.bond + n2.bond + n3.bond,
            weight: n1.weight + n2.weight + n3.weight,
            capacity: Decimal::MAX,
            nodes: vec![n1, n2, n3],
        };
        assert_eq!(nodes.churning_out(), Decimal::from_ratio(15u128, 1u128));
    }

    #[test]
    fn churn_out_candidate_respects_thresholds() {
        let n1 = node(
            "node-1",
            Decimal::from_ratio(10u128, 1u128),
            Decimal::one(),
            false,
        );
        let n2 = node(
            "node-2",
            Decimal::from_ratio(30u128, 1u128),
            Decimal::one(),
            false,
        );
        let nodes = Nodes {
            bond: n1.bond + n2.bond,
            weight: n1.weight + n2.weight,
            capacity: Decimal::MAX,
            nodes: vec![n1, n2],
        };
        assert_eq!(
            nodes
                .churn_out_candidate(&Decimal::from_ratio(20u128, 1u128), &Decimal::one())
                .unwrap()
                .addr
                .to_string(),
            "node-2"
        );
        assert!(nodes
            .churn_out_candidate(
                &Decimal::from_ratio(40u128, 1u128),
                &Decimal::from_ratio(50u128, 1u128)
            )
            .is_none());
    }

    #[test]
    fn allocate_bond_emits_messages() {
        let n1 = node(
            "node-1",
            Decimal::from_ratio(10u128, 1u128),
            Decimal::from_ratio(2u128, 1u128),
            false,
        );
        let n2 = node(
            "node-2",
            Decimal::from_ratio(30u128, 1u128),
            Decimal::from_ratio(5u128, 1u128),
            false,
        );
        let mut nodes = Nodes {
            bond: n1.bond + n2.bond,
            weight: n1.weight + n2.weight,
            capacity: Decimal::MAX,
            nodes: vec![n1, n2],
        };
        let signer = CanonicalAddr::from(vec![1u8; 32]);
        let mut storage = MockStorage::new();
        let response = nodes
            .allocate_bond(
                &mut storage,
                &mock_env(),
                &signer,
                Decimal::from_ratio(10u128, 1u128),
                Response::default(),
            )
            .unwrap();
        assert_eq!(response.messages.len(), 1);
    }

    #[test]
    fn allocate_bond_skips_fully_bonded_nodes() {
        let n1 = node(
            "node-1",
            Decimal::from_ratio(10u128, 1u128),
            Decimal::from_ratio(2u128, 1u128),
            false,
        );
        let mut nodes = Nodes {
            bond: n1.bond,
            weight: n1.weight,
            capacity: Decimal::MAX,
            nodes: vec![n1],
        };
        let signer = CanonicalAddr::from(vec![1u8; 32]);
        let mut storage = MockStorage::new();
        let response = nodes
            .allocate_bond(
                &mut storage,
                &mock_env(),
                &signer,
                Decimal::zero(),
                Response::default(),
            )
            .unwrap();
        assert!(response.messages.is_empty());
    }

    #[test]
    fn unbond_ignores_non_unbond_statuses() {
        let nodes = Nodes {
            bond: Decimal::from_ratio(10u128, 1u128),
            weight: Decimal::one(),
            capacity: Decimal::MAX,
            nodes: vec![
                Node {
                    status: NodeStatus::Unknown,
                    ..node(
                        "node-1",
                        Decimal::from_ratio(10u128, 1u128),
                        Decimal::one(),
                        false,
                    )
                },
                Node {
                    status: NodeStatus::Ready,
                    ..node(
                        "node-2",
                        Decimal::from_ratio(10u128, 1u128),
                        Decimal::one(),
                        false,
                    )
                },
                Node {
                    status: NodeStatus::Active,
                    ..node(
                        "node-3",
                        Decimal::from_ratio(10u128, 1u128),
                        Decimal::one(),
                        false,
                    )
                },
            ],
        };
        let signer = CanonicalAddr::from(vec![1u8; 32]);
        let mut response = Response::default();
        let target = Decimal::from_ratio(10u128, 1u128);
        let total = nodes
            .unbond(
                mock_dependencies().as_mut().storage,
                &mock_env(),
                &signer,
                &mut response,
                &target,
            )
            .unwrap();
        assert_eq!(total, Decimal::zero());
        assert!(response.messages.is_empty());
    }

    #[test]
    fn request_bond_returns_empty_when_min_negative() {
        let mut deps = deps_with_mimir(-1);
        let n1 = node(
            "node-1",
            Decimal::from_ratio(10u128, 1u128),
            Decimal::one(),
            false,
        );
        let mut nodes = Nodes {
            bond: n1.bond,
            weight: n1.weight,
            capacity: Decimal::MAX,
            nodes: vec![n1],
        };
        let signer = CanonicalAddr::from(vec![1u8; 32]);
        let response = nodes
            .request_bond(
                deps.as_mut(),
                &mock_env(),
                &signer,
                &Decimal::one(),
                Response::default(),
            )
            .unwrap();
        assert!(response.messages.is_empty());
    }

    #[test]
    fn request_bond_returns_empty_when_no_candidate() {
        let mut deps = deps_with_mimir(100);
        let n1 = node(
            "node-1",
            Decimal::from_ratio(10u128, 1u128),
            Decimal::one(),
            false,
        );
        let mut nodes = Nodes {
            bond: n1.bond,
            weight: n1.weight,
            capacity: Decimal::MAX,
            nodes: vec![n1],
        };
        let signer = CanonicalAddr::from(vec![1u8; 32]);
        let response = nodes
            .request_bond(
                deps.as_mut(),
                &mock_env(),
                &signer,
                &Decimal::from_ratio(5u128, 1u128),
                Response::default(),
            )
            .unwrap();
        assert!(response.messages.is_empty());
    }

    #[test]
    fn request_bond_selects_churn_out_candidate() {
        let mut deps = deps_with_mimir(1);
        let n1 = node(
            "node-1",
            Decimal::from_ratio(10u128, 1u128),
            Decimal::one(),
            false,
        );
        let n2 = node(
            "node-2",
            Decimal::from_ratio(50u128, 1u128),
            Decimal::one(),
            false,
        );
        let mut nodes = Nodes {
            bond: n1.bond + n2.bond,
            weight: n1.weight + n2.weight,
            capacity: Decimal::MAX,
            nodes: vec![n1, n2],
        };
        let signer = CanonicalAddr::from(vec![1u8; 32]);
        let response = nodes
            .request_bond(
                deps.as_mut(),
                &mock_env(),
                &signer,
                &Decimal::from_ratio(20u128, 1u128),
                Response::default(),
            )
            .unwrap();
        assert_eq!(response.messages.len(), 1);
    }

    fn sum_msg_deposit_coins(response: &Response) -> Uint128 {
        let mut total = Uint128::zero();
        for msg in &response.messages {
            let CosmosMsg::Any(AnyMsg { value, .. }) = &msg.msg else {
                continue;
            };
            let buf = Bufany::deserialize(value.as_slice()).unwrap();
            let coins = buf.repeated_bytes(1).unwrap_or_default();
            for coin_bytes in coins {
                let coin_buf = Bufany::deserialize(&coin_bytes).unwrap();
                let amount_str = coin_buf.string(2).unwrap();
                let amount = u128::from_str(&amount_str).unwrap();
                total += Uint128::new(amount);
            }
        }
        total
    }

    proptest! {
        #[test]
        fn allocate_bond_allocates_floor_amount(
            weights in prop::collection::vec(1u128..1000u128, 1..6),
            bonds in prop::collection::vec(0u128..1_000_000u128, 1..6),
            capacities in prop::collection::vec(0u128..1_000_000u128, 1..6),
            amount in 0u128..1_000_000u128,
        ) {
            let len = weights.len().min(bonds.len()).min(capacities.len());
            let mut nodes_vec = Vec::with_capacity(len);
            let mut total_weight = Decimal::zero();
            let mut total_bond = Decimal::zero();
            let mut total_capacity = Decimal::zero();

            for idx in 0..len {
                let weight = Decimal::from_ratio(weights[idx], 1u128);
                let bond = Decimal::from_ratio(bonds[idx], 1u128);
                let capacity = Decimal::from_ratio(capacities[idx], 1u128);
                total_capacity += capacity;
                total_weight += weight;
                total_bond += bond;
                nodes_vec.push(Node {
                    addr: Addr::unchecked(format!("node-{idx}")),
                    fee: Decimal::zero(),
                    bond,
                    weight,
                    capacity,
                    is_leaving: false,
                    status: NodeStatus::Active,
                    res: Default::default(),
                });
            }

            prop_assume!(!total_weight.is_zero());

            let mut nodes = Nodes {
                bond: total_bond,
                weight: total_weight,
                capacity: total_capacity,
                nodes: nodes_vec,
            };

            nodes.sort_nodes_over();


            let mut storage = MockStorage::new();
            let env = mock_env();
            let signer= CanonicalAddr::from(vec![1u8; 32]);

            let response = nodes.allocate_bond(
                &mut storage,
                &env,
                &signer,
                Decimal::from_ratio(amount, 1u128),
                Response::default(),
            ).unwrap();

            let summed = sum_msg_deposit_coins(&response);
            let expected = Uint128::new(amount).min(total_capacity.to_uint_floor());
            prop_assert_eq!(summed, expected);
        }
    }
}
