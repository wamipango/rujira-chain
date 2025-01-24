use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Binary, Coin, Decimal, Uint128};

#[cw_serde]
pub struct InstantiateMsg {
    pub registry: String,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Create a new, update or close an existing position
    /// Set the target amounts. If both set to zero, position will be closed
    /// Surplus is returned to the sender
    Manage {
        // Optionally do something with some collateral before actions are executed.
        // Returned funds are added to the funds sent in the tx and used to fund the rebalance
        // request of the tx
        // Can be used to eg sell a portion of collateral and use the proceeds to repay debt
        // without requiring spare funds
        // The `amount` value of the Action will be deducted from the existing position
        collateral_action: Option<Action>,

        /// Target borrow amount
        debt_target: Uint128,

        /// Target collateral amount
        collateral_target: Uint128,

        // Optionally do something with the borrowed funds after the position is adjusted, using the proceeds
        // to fund the rebalance request
        // Can be used to open a margin position without "looping"
        // eg with 0.3 BTC dpoesit @ 100k and max LTV 70%, borrow 70k USDC and swap for BTC collateral,
        // before the position health is checked
        // The `amount` value of the Action will be borrowed from the Vault in addition to the delta between
        // the current position debt and `debt_target`
        debt_action: Option<Action>,
    },

    /// Transfer debt to another borrow position, optionally swapping collateral and moving it at the same time
    Transfer {
        /// Amount of the Vault's debt shares to transfer.
        debt_shares: Uint128,
        /// ghost-borrow contract address destination
        /// this address must be registered on ghost-registry for the transfer to be valid
        destination: String,
        /// An optional execution step that will deduct collateral from the current position,
        /// exchange it for the collateral of the destination, and transfer them all together
        collateral: Option<Action>,
    },

    // Privileged actions used when executing Action's
    Callback(CallbackMsg),
}

#[cw_serde]
pub enum CallbackMsg {
    ManageCollateral {
        funds: Vec<Coin>,
        debt_target: Uint128,
        collateral_target: Uint128,
        debt_action: Option<Action>,
    },

    ManageDebt {
        funds: Vec<Coin>,
        debt_target: Uint128,
        collateral_target: Uint128,
    },

    TransferCollateral {
        debt_shares: Uint128,
        destination: String,
    },
}

#[cw_serde]
pub struct Action {
    contract: String,
    msg: Binary,
    amount: Uint128,
}

#[cw_serde]
pub enum SudoMsg {}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(ConfigResponse)]
    Config {},
    #[returns(StatusResponse)]
    Status {},
    #[returns(PositionResponse)]
    Position(String),
}

#[cw_serde]
pub struct ConfigResponse {
    pub collateral_denom: String,
    pub debt_denom: String,
    pub vault_addr: String,
    pub orca_addr: String,
}

#[cw_serde]
pub struct StatusResponse {
    pub collateral_denom: String,

    pub debt_denom: String,

    /// Total amount of collateral deposited
    pub collateral_amount: Uint128,

    /// Total amount of debt issued
    pub debt_amount: Uint128,

    /// Current debt rate
    pub debt_rate: Decimal,

    /// Current debt limit
    pub debt_limit: Uint128,

    /// Maximum LTV value before the contract is allowed to begin liqudiating collateral
    pub max_ltv: Decimal,

    /// Current collateral price denmincated in debt token
    /// Adjusted for decimal delta between tokens
    pub price: Decimal,
}

#[cw_serde]
pub struct PositionResponse {
    pub owner: String,
    pub collateral: Uint128,
    pub debt_shares: Uint128,
    pub debt_amount: Uint128,
    pub ltv: Decimal,
}
