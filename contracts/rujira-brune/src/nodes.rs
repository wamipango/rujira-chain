use std::{
    cmp::{max, min},
    ops::Add,
    str::FromStr,
};

use cosmwasm_schema::cw_serde;
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
#[cw_serde]
#[derive(Default)]
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

    fn queue_unbond(
        storage: &mut dyn Storage,
        env: &Env,
        signer: &CanonicalAddr,
        response: &mut Response,
        addr: &Addr,
        amount: Decimal,
    ) -> StdResult<()> {
        let amount = amount.to_uint_floor();
        response.messages.push(SubMsg::reply_on_error(
            MsgUnbond::new(addr.clone(), amount, None, signer),
            1,
        ));
        response
            .events
            .push(event_unbond(storage, env, addr, amount)?);
        Ok(())
    }

    /// Unbond from standby nodes that are no longer bondable because their preflight failed.
    ///
    /// This covers nodes that initially looked bondable only because bond size was the first
    /// preflight failure, but reveal a later failure after BRUNE meets the minimum bond.
    pub fn unbond_failed_preflight(
        &mut self,
        storage: &mut dyn Storage,
        env: &Env,
        signer: &CanonicalAddr,
        response: &mut Response,
    ) -> StdResult<(Decimal, Vec<Addr>)> {
        let mut total = Decimal::zero();
        let mut quarantined = vec![];
        for n in &mut self.nodes {
            if !n.failed_preflight() {
                continue;
            }
            quarantined.push(n.addr.clone());

            let alloc = n.bond;
            if alloc.lt(&Decimal::one()) {
                continue;
            }

            Self::queue_unbond(storage, env, signer, response, &n.addr, alloc)?;
            total += alloc;
            n.bond = Decimal::zero();
        }
        self.bond = self.bond.saturating_sub(total);
        Ok((total, quarantined))
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
                    Self::queue_unbond(storage, env, signer, response, &n.addr, alloc)?;
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
            if !matches!(n.status, NodeStatus::Active | NodeStatus::Standby) || n.is_leaving {
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
        min_bond: &Decimal,
        signer: &CanonicalAddr,
        amount: &Decimal,
        response: Response,
    ) -> Result<Response, ContractError> {
        self.sort_nodes_over();
        let mut remaining = *amount;
        let mut response = response;
        remaining -= self.unbond(deps.storage, env, signer, &mut response, amount)?;
        remaining -= self.churning_out().min(remaining);
        // Finally, select a node where we have > min bond threshold such that we can request a churn out

        if remaining.lt(&Decimal::one()) {
            return Ok(response);
        }
        match self.churn_out_candidate(min_bond, &remaining) {
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

#[cw_serde]
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
    pub preflight_status: Option<NodeStatus>,
    pub preflight_reason: Option<String>,
    pub status: NodeStatus,

    pub res: QueryNodeResponse,
}

impl Node {
    pub fn minimum_bond(q: QuerierWrapper) -> Result<Decimal, ContractError> {
        let min = QueryMimirWithKeyResponse::get(
            q,
            QueryMimirWithKeyRequest {
                key: "MinimumBondInRune".to_string(),
                height: "".to_string(),
            },
        )?
        .value;
        if min < 0 {
            return Err(ContractError::InvalidMinimumBond);
        }
        Ok(Decimal::from_atomics(min.abs_diff(0), 0)?)
    }
    pub fn load(
        q: QuerierWrapper,
        env: &Env,
        config: &Config,
        min_bond: &Decimal,
        addr: &Addr,
    ) -> Result<Option<Self>, ContractError> {
        let response = QueryNodeResponse::get(
            q,
            QueryNodeRequest {
                address: addr.to_string(),
                height: "".to_string(),
            },
        )?;

        Ok(Self::from_response(env, config, min_bond, addr, response))
    }

    /// Calculates the weight allocated to this Node to determine
    /// the proportion of total bond it will receive
    fn weight(config: &Config, fee: &Decimal) -> Decimal {
        Decimal::one() - max(config.min_node_fee, *fee).pow(3)
    }

    fn from_response(
        env: &Env,
        config: &Config,
        min_bond: &Decimal,
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
        let total_bond = Decimal::from_str(&res.total_bond).unwrap_or_default();
        let max_bond = Decimal::from_ratio(config.max_bond.u128(), 1u128);
        let max_effective_bond = Decimal::from_ratio(config.max_effective_bond.u128(), 1u128);
        let status = NodeStatus::from_str(&res.status).unwrap_or(NodeStatus::Unknown);
        let preflight_status = res
            .preflight_status
            .as_ref()
            .and_then(|x| NodeStatus::from_str(&x.status).ok());
        let preflight_reason = res.preflight_status.as_ref().map(|x| x.reason.clone());
        let maximum = min(
            max_bond.saturating_sub(bond),
            max_effective_bond.saturating_sub(total_bond),
        );
        let capacity = Self::capacity(
            status.clone(),
            preflight_status.clone(),
            preflight_reason.as_deref(),
            &total_bond,
            &maximum,
            min_bond,
        );
        Some(Self {
            addr: addr.clone(),
            weight: Self::weight(config, &fee),
            fee,
            capacity,
            bond,
            is_leaving: res.forced_to_leave || res.requested_to_leave,
            preflight_status,
            preflight_reason,
            status,
            res,
        })
    }

    fn capacity(
        status: NodeStatus,
        preflight_status: Option<NodeStatus>,
        preflight_reason: Option<&str>,
        total_bond: &Decimal,
        maximum: &Decimal,
        min_bond: &Decimal,
    ) -> Decimal {
        match status {
            NodeStatus::Active => *maximum,
            NodeStatus::Standby => match preflight_status {
                Some(NodeStatus::Ready) => *maximum,
                Some(NodeStatus::Standby) => {
                    // Add capacity iff bond size is the limiting factor, and we'd also push it over threshold
                    if preflight_reason
                        .map(|reason| reason.to_ascii_lowercase().contains("minimum bond"))
                        .unwrap_or(false)
                        && total_bond.add(*maximum).ge(min_bond)
                    {
                        *maximum
                    } else {
                        Decimal::zero()
                    }
                }
                _ => Decimal::zero(),
            },
            _ => Decimal::zero(),
        }
    }

    fn failed_preflight(&self) -> bool {
        self.status == NodeStatus::Standby
            && self.preflight_status == Some(NodeStatus::Standby)
            && !self
                .preflight_reason
                .as_ref()
                .map(|reason| reason.to_ascii_lowercase().contains("minimum bond"))
                .unwrap_or(false)
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
    use cosmwasm_std::{testing::mock_env, Addr, CanonicalAddr, Decimal, Env, Response, Uint128};
    use cosmwasm_std::{testing::MockStorage, AnyMsg, CosmosMsg};
    use cw_utils::Duration;
    use proptest::prelude::*;
    use rujira_rs::proto::types::{NodeBondProvider, NodeBondProviders, NodePreflightStatus};
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
            preflight_status: None,
            preflight_reason: None,
            status: NodeStatus::Active,
            res: Default::default(),
        }
    }

    fn config(max_bond: u128, max_effective_bond: u128) -> Config {
        Config {
            target_utilization: Decimal::one(),
            min_node_fee: Decimal::percent(5),
            range: rujira_rs::brune::RangeConfig {
                high: Decimal::percent(99),
                low: Decimal::percent(80),
                skew: cosmwasm_std::SignedDecimal::zero(),
                step: Decimal::percent(1),
            },
            permissionless: false,
            fin_contract: Addr::unchecked("fin"),
            stake_contract: Addr::unchecked("stake"),
            revenue_smear: 1,
            quarantine: Duration::Height(10),
            token_id: "brune".to_string(),
            max_bond: Uint128::new(max_bond),
            max_effective_bond: Uint128::new(max_effective_bond),
            mint_cap: Uint128::new(1_000),
        }
    }

    fn response(
        env: &Env,
        status: &str,
        total_bond: &str,
        provider_bond: &str,
        preflight: Option<(&str, &str)>,
    ) -> QueryNodeResponse {
        QueryNodeResponse {
            node_address: "node".to_string(),
            status: status.to_string(),
            total_bond: total_bond.to_string(),
            bond_providers: Some(NodeBondProviders {
                node_operator_fee: "500".to_string(),
                providers: vec![NodeBondProvider {
                    bond_address: env.contract.address.to_string(),
                    bond: provider_bond.to_string(),
                }],
            }),
            preflight_status: preflight.map(|(status, reason)| NodePreflightStatus {
                status: status.to_string(),
                reason: reason.to_string(),
                code: if status == "Ready" { 0 } else { 1 },
            }),
            ..QueryNodeResponse::default()
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
            range: rujira_rs::brune::RangeConfig {
                high: Decimal::percent(99),
                low: Decimal::percent(80),
                skew: cosmwasm_std::SignedDecimal::zero(),
                step: Decimal::percent(1),
            },
            permissionless: false,
            fin_contract: Addr::unchecked("fin"),
            stake_contract: Addr::unchecked("stake"),
            revenue_smear: 1,
            quarantine: Duration::Height(10),
            token_id: "brune".to_string(),
            max_bond: Uint128::new(100),
            max_effective_bond: Uint128::new(1_000_000),
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
            range: rujira_rs::brune::RangeConfig {
                high: Decimal::percent(99),
                low: Decimal::percent(80),
                skew: cosmwasm_std::SignedDecimal::zero(),
                step: Decimal::percent(1),
            },
            permissionless: false,
            fin_contract: Addr::unchecked("fin"),
            stake_contract: Addr::unchecked("stake"),
            revenue_smear: 1,
            quarantine: Duration::Height(10),
            token_id: "brune".to_string(),
            max_bond: Uint128::new(100),
            max_effective_bond: Uint128::new(1_000_000),
            mint_cap: Uint128::new(1_000),
        };
        let addr = Addr::unchecked("node");
        let res = QueryNodeResponse {
            node_address: addr.to_string(),
            ..QueryNodeResponse::default()
        };
        assert!(Node::from_response(&env, &config, &Decimal::zero(), &addr, res).is_none());
    }

    #[test]
    fn from_response_builds_node() {
        let mut env = mock_env();
        env.contract.address = Addr::unchecked("contract");
        let config = Config {
            target_utilization: Decimal::one(),
            min_node_fee: Decimal::percent(5),
            range: rujira_rs::brune::RangeConfig {
                high: Decimal::percent(99),
                low: Decimal::percent(80),
                skew: cosmwasm_std::SignedDecimal::zero(),
                step: Decimal::percent(1),
            },
            permissionless: false,
            fin_contract: Addr::unchecked("fin"),
            stake_contract: Addr::unchecked("stake"),
            revenue_smear: 1,
            quarantine: Duration::Height(10),
            token_id: "brune".to_string(),
            max_bond: Uint128::new(100),
            max_effective_bond: Uint128::new(1_000_000),
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

        let node = Node::from_response(&env, &config, &Decimal::zero(), &addr, res).unwrap();
        assert_eq!(node.addr, addr);
        assert_eq!(node.bond, Decimal::from_ratio(25u128, 1u128));
        assert!(node.is_leaving);
        assert_eq!(node.status, NodeStatus::Active);
    }

    #[test]
    fn from_response_keeps_current_status_separate_from_preflight() {
        let mut env = mock_env();
        env.contract.address = Addr::unchecked("contract");
        let config = config(100, 1_000_000);
        let addr = Addr::unchecked("node");

        let node = Node::from_response(
            &env,
            &config,
            &Decimal::from_ratio(50u128, 1u128),
            &addr,
            response(
                &env,
                "Active",
                "10",
                "10",
                Some(("Standby", "node account has invalid registered IP address")),
            ),
        )
        .unwrap();

        assert_eq!(node.status, NodeStatus::Active);
        assert_eq!(node.preflight_status, Some(NodeStatus::Standby));
        assert_eq!(node.capacity, Decimal::from_ratio(90u128, 1u128));
    }

    #[test]
    fn from_response_standby_capacity_requires_ready_or_min_bond_only_preflight() {
        let mut env = mock_env();
        env.contract.address = Addr::unchecked("contract");
        let config = config(100, 1_000_000);
        let min_bond = Decimal::from_ratio(50u128, 1u128);
        let addr = Addr::unchecked("node");

        let ready = Node::from_response(
            &env,
            &config,
            &min_bond,
            &addr,
            response(&env, "Standby", "10", "10", Some(("Ready", "OK"))),
        )
        .unwrap();
        assert_eq!(ready.capacity, Decimal::from_ratio(90u128, 1u128));

        let min_only = Node::from_response(
            &env,
            &config,
            &min_bond,
            &addr,
            response(
                &env,
                "Standby",
                "10",
                "10",
                Some((
                    "Standby",
                    "node account does not have minimum bond requirement: 10/50",
                )),
            ),
        )
        .unwrap();
        assert_eq!(min_only.capacity, Decimal::from_ratio(90u128, 1u128));

        let invalid_ip = Node::from_response(
            &env,
            &config,
            &min_bond,
            &addr,
            response(
                &env,
                "Standby",
                "10",
                "10",
                Some(("Standby", "node account has invalid registered IP address")),
            ),
        )
        .unwrap();
        assert_eq!(invalid_ip.capacity, Decimal::zero());
    }

    #[test]
    fn from_response_ready_status_is_not_counted_as_bondable_capacity() {
        let mut env = mock_env();
        env.contract.address = Addr::unchecked("contract");
        let config = config(100, 1_000_000);
        let addr = Addr::unchecked("node");
        let node = Node::from_response(
            &env,
            &config,
            &Decimal::from_ratio(50u128, 1u128),
            &addr,
            response(&env, "Ready", "10", "10", Some(("Ready", "OK"))),
        )
        .unwrap();
        assert_eq!(node.capacity, Decimal::zero());
    }

    #[test]
    fn from_response_caps_capacity_by_max_effective_bond() {
        let mut env = mock_env();
        env.contract.address = Addr::unchecked("contract");
        let config = Config {
            target_utilization: Decimal::one(),
            min_node_fee: Decimal::percent(5),
            range: rujira_rs::brune::RangeConfig {
                high: Decimal::percent(99),
                low: Decimal::percent(80),
                skew: cosmwasm_std::SignedDecimal::zero(),
                step: Decimal::percent(1),
            },
            permissionless: false,
            fin_contract: Addr::unchecked("fin"),
            stake_contract: Addr::unchecked("stake"),
            revenue_smear: 1,
            quarantine: Duration::Height(10),
            token_id: "brune".to_string(),
            // High per-node cap so max_effective_bond is the binding constraint.
            max_bond: Uint128::new(10_000),
            max_effective_bond: Uint128::new(1_000),
            mint_cap: Uint128::new(1_000),
        };
        let addr = Addr::unchecked("node");
        let res = QueryNodeResponse {
            node_address: addr.to_string(),
            status: "Active".to_string(),
            total_bond: "800".to_string(),
            bond_providers: Some(NodeBondProviders {
                node_operator_fee: "500".to_string(),
                providers: vec![NodeBondProvider {
                    bond_address: env.contract.address.to_string(),
                    bond: "100".to_string(),
                }],
            }),
            ..QueryNodeResponse::default()
        };

        let node = Node::from_response(&env, &config, &Decimal::zero(), &addr, res).unwrap();
        // headroom = max_effective_bond - total_bond = 1000 - 800 = 200
        // (would be 9_900 from max_bond - bond, so max_effective is the binding cap)
        assert_eq!(node.capacity, Decimal::from_ratio(200u128, 1u128));
    }

    #[test]
    fn from_response_capacity_zero_when_node_saturated() {
        let mut env = mock_env();
        env.contract.address = Addr::unchecked("contract");
        let config = Config {
            target_utilization: Decimal::one(),
            min_node_fee: Decimal::percent(5),
            range: rujira_rs::brune::RangeConfig {
                high: Decimal::percent(99),
                low: Decimal::percent(80),
                skew: cosmwasm_std::SignedDecimal::zero(),
                step: Decimal::percent(1),
            },
            permissionless: false,
            fin_contract: Addr::unchecked("fin"),
            stake_contract: Addr::unchecked("stake"),
            revenue_smear: 1,
            quarantine: Duration::Height(10),
            token_id: "brune".to_string(),
            max_bond: Uint128::new(10_000),
            max_effective_bond: Uint128::new(1_000),
            mint_cap: Uint128::new(1_000),
        };
        let addr = Addr::unchecked("node");
        let res = QueryNodeResponse {
            node_address: addr.to_string(),
            status: "Active".to_string(),
            // total_bond exceeds max_effective_bond — no headroom.
            total_bond: "1500".to_string(),
            bond_providers: Some(NodeBondProviders {
                node_operator_fee: "500".to_string(),
                providers: vec![NodeBondProvider {
                    bond_address: env.contract.address.to_string(),
                    bond: "0".to_string(),
                }],
            }),
            ..QueryNodeResponse::default()
        };

        let node = Node::from_response(&env, &config, &Decimal::zero(), &addr, res).unwrap();
        assert_eq!(node.capacity, Decimal::zero());
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
    fn unbond_failed_preflight_releases_stranded_standby_bond() {
        let failed = Node {
            status: NodeStatus::Standby,
            preflight_status: Some(NodeStatus::Standby),
            preflight_reason: Some(
                "node account does not meet min version requirement: 1.0.0 vs 1.1.0".to_string(),
            ),
            capacity: Decimal::zero(),
            ..node(
                "node-1",
                Decimal::from_ratio(10u128, 1u128),
                Decimal::one(),
                false,
            )
        };
        let min_bond = Node {
            status: NodeStatus::Standby,
            preflight_status: Some(NodeStatus::Standby),
            preflight_reason: Some(
                "node account does not have minimum bond requirement: 10/50".to_string(),
            ),
            capacity: Decimal::zero(),
            ..node(
                "node-2",
                Decimal::from_ratio(5u128, 1u128),
                Decimal::one(),
                false,
            )
        };
        let ready_without_headroom = Node {
            status: NodeStatus::Standby,
            preflight_status: Some(NodeStatus::Ready),
            capacity: Decimal::zero(),
            ..node(
                "node-3",
                Decimal::from_ratio(15u128, 1u128),
                Decimal::one(),
                false,
            )
        };
        let zero_bond_failed = Node {
            status: NodeStatus::Standby,
            preflight_status: Some(NodeStatus::Standby),
            preflight_reason: Some("node account is jailed until block 1234: reason".to_string()),
            capacity: Decimal::zero(),
            ..node("node-4", Decimal::zero(), Decimal::one(), false)
        };
        let mut nodes = Nodes {
            bond: Decimal::from_ratio(30u128, 1u128),
            weight: Decimal::from_ratio(4u128, 1u128),
            capacity: Decimal::zero(),
            nodes: vec![failed, min_bond, ready_without_headroom, zero_bond_failed],
        };
        let signer = CanonicalAddr::from(vec![1u8; 32]);
        let mut response = Response::default();
        let (total, quarantined) = nodes
            .unbond_failed_preflight(
                mock_dependencies().as_mut().storage,
                &mock_env(),
                &signer,
                &mut response,
            )
            .unwrap();

        assert_eq!(total, Decimal::from_ratio(10u128, 1u128));
        assert_eq!(nodes.bonded(), Decimal::from_ratio(20u128, 1u128));
        assert_eq!(nodes.nodes[0].bond, Decimal::zero());
        assert_eq!(nodes.nodes[1].bond, Decimal::from_ratio(5u128, 1u128));
        assert_eq!(nodes.nodes[2].bond, Decimal::from_ratio(15u128, 1u128));
        assert_eq!(
            quarantined,
            vec![Addr::unchecked("node-1"), Addr::unchecked("node-4")]
        );
        assert_eq!(response.messages.len(), 1);
    }

    #[test]
    fn request_bond_returns_empty_when_no_candidate() {
        let mut deps = mock_dependencies();
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
                &Decimal::percent(10000),
                &signer,
                &Decimal::from_ratio(5u128, 1u128),
                Response::default(),
            )
            .unwrap();
        assert!(response.messages.is_empty());
    }

    #[test]
    fn request_bond_selects_churn_out_candidate() {
        let mut deps = mock_dependencies();
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
                &Decimal::one(),
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
                    preflight_status: None,
                    preflight_reason: None,
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
