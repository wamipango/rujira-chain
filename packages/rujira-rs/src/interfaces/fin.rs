use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Coin, Decimal, Fraction, StdError, Timestamp, Uint128};
use cw_storage_plus::{IntKey, Key, KeyDeserialize, Prefixer, PrimaryKey};

use crate::{tick::Tick, CallbackData, Layer1Asset};

#[cw_serde]
pub struct InstantiateMsg {
    /// The denoms of the pair. The second denom is the quote denom
    pub denoms: [String; 2],

    pub oracles: [Layer1Asset; 2],

    /// Ticked truncates Decimal to significant figures.
    /// This accommodates prices decreasing and adding zeroes,
    /// but may need adjusting for better UX as prices increase
    /// (e.g. 4sf is plenty for most tokens < $10 - 1 cent, but above $1000 it creates a $1 tick)  
    pub tick: Tick,

    /// The fee charged on swaps, and instantly filled limit orders
    pub fee_taker: Decimal,

    /// The fee charged on withdrawals from filled limit orders
    pub fee_maker: Decimal,

    /// The destination address for fees collected
    pub fee_address: String,
}

/// Callable interfaces
#[cw_serde]
pub enum ExecuteMsg {
    /// Executes a market trade based on current order book.
    Swap {
        min_return: Option<Uint128>,
        to: Option<String>,

        /// An optional callback that FIN will execute with the funds from the swap.
        /// The callback is executed on the sender's address.
        #[serde(skip_serializing_if = "Option::is_none")]
        callback: Option<CallbackData>,
    },

    /// Manage all orders
    /// Submit a list of price and target offer amounts
    /// 0. All filled orders will be withdrawn
    /// For each entry:
    /// 1. If no order exists at that price, one will be created
    /// 2. If an order exists, and the `offer_amount` is greater than the target amount, it will be reduced
    /// 3. If the `offer_amount` is less than the target amount, it will be increased
    ///
    /// Funds sent must be equal to the net change of balances. Funds withdrawn in step 0 and retracted in 1's,
    /// can be reused to fund orders in 1 and 3  
    Order(Vec<(Side, Price, Uint128)>),
}

#[cw_serde]
pub enum Price {
    Fixed(Decimal),
    Oracle(i16),
}

impl Price {
    pub fn to_rate(&self, oracle: &Decimal) -> Decimal {
        match self {
            Price::Fixed(fixed) => *fixed,
            Price::Oracle(deviation) => match deviation {
                i16::MIN..=-1 => {
                    oracle * Decimal::from_ratio(10000u16 - deviation.unsigned_abs(), 10000u16)
                }
                0 => oracle.clone(),
                1..=i16::MAX => {
                    oracle * Decimal::from_ratio(deviation.unsigned_abs() + 10000u16, 10000u16)
                }
            },
        }
    }
}

impl PrimaryKey<'_> for Price {
    type Prefix = ();
    type SubPrefix = ();
    type Suffix = ();
    type SuperSuffix = ();

    fn key(&self) -> std::vec::Vec<Key<'_>> {
        match self {
            Price::Fixed(fixed) => vec![Key::Val128(fixed.numerator().to_be_bytes())],
            Price::Oracle(deviation) => vec![Key::Val16(deviation.to_cw_bytes())],
        }
    }
}

impl KeyDeserialize for Price {
    type Output = Self;
    const KEY_ELEMS: u16 = 1;

    fn from_vec(value: Vec<u8>) -> cosmwasm_std::StdResult<Self::Output> {
        match value.len() {
            16 => Ok(Self::Fixed(Decimal::new(Uint128::from(
                u128::from_be_bytes(value.try_into().unwrap()),
            )))),
            2 => Ok(Self::Oracle(i16::from_cw_bytes(value.try_into().unwrap()))),
            _ => Err(StdError::generic_err("invalid Price key")),
        }
    }
}

#[cw_serde]
pub enum Side {
    Ask,
    Bid,
}

impl PrimaryKey<'_> for Side {
    type Prefix = ();
    type SubPrefix = ();
    type Suffix = ();
    type SuperSuffix = ();

    fn key(&self) -> std::vec::Vec<Key<'_>> {
        match self {
            Side::Ask => vec![Key::Val8([0])],
            Side::Bid => vec![Key::Val8([1])],
        }
    }
}

impl<'a> Prefixer<'a> for Side {
    fn prefix(&self) -> Vec<Key> {
        self.key()
    }
}

impl KeyDeserialize for Side {
    type Output = Self;
    const KEY_ELEMS: u16 = 1;

    fn from_vec(value: Vec<u8>) -> cosmwasm_std::StdResult<Self::Output> {
        match value.first() {
            Some(0u8) => Ok(Self::Ask),
            Some(1u8) => Ok(Self::Bid),
            _ => Err(StdError::generic_err("invalid Side key")),
        }
    }
}

#[cw_serde]
pub enum SudoMsg {
    Pause {},
    Resume {},
    UpdateConfig {
        tick: Option<u8>,
        fee_taker: Option<Decimal>,
        fee_maker: Option<Decimal>,
    },
}

/// Standard interface to query contract state
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(ConfigResponse)]
    Config {},

    #[returns(StatusResponse)]
    Status {},

    #[returns(SimulationResponse)]
    Simulate(Coin),

    /// Find a specific order for a user at a price
    #[returns(OrderResponse)]
    Order((String, Side, Price)),

    /// Paginate user orders. Upper limit of 30 per page
    #[returns(OrdersResponse)]
    Orders {
        owner: String,
        offset: Option<u8>,
        limit: Option<u8>,
    },

    #[returns(BookResponse)]
    Book {
        limit: Option<u8>,
        offset: Option<u8>,
    },
}

#[cw_serde]
pub struct ConfigResponse {
    /// See [InstantiateMsg::denoms]
    pub denoms: [String; 2],

    /// See [InstantiateMsg::oracles]
    pub oracles: [Layer1Asset; 2],

    /// See [InstantiateMsg::tick]
    pub tick: Tick,

    /// See [InstantiateMsg::fee_taker]    
    pub fee_taker: Decimal,

    /// See [InstantiateMsg::fee_maker]
    pub fee_maker: Decimal,

    /// See [InstantiateMsg::fee_address]
    pub fee_address: String,
}

#[cw_serde]
pub enum StatusResponse {
    Active { price: Decimal },
    Paused,
}

#[cw_serde]
pub struct OrderResponse {
    /// The account which placed the order
    pub owner: String,

    /// The side of the order
    pub side: Side,

    /// The quote price of this order
    pub price: Price,

    /// The rate at which this order would execute at the current moment in time
    pub rate: Decimal,

    /// The last time this order was touched (created, incremented or reduced) in an Order execution
    pub updated_at: Timestamp,

    /// Offer amount at updated_at time
    pub offer: Uint128,

    /// The remaining offer amount
    pub remaining: Uint128,

    /// Amount of filled order awaiting withdrawal
    pub filled: Uint128,
}

#[cw_serde]
pub struct OrdersResponse {
    pub orders: Vec<OrderResponse>,
}

#[cw_serde]
pub struct BookResponse {
    pub base: Vec<BookItemResponse>,
    pub quote: Vec<BookItemResponse>,
}

#[cw_serde]
pub struct BookItemResponse {
    pub price: Decimal,
    pub total: Uint128,
}

#[cw_serde]
pub struct SimulationResponse {
    pub returned: Uint128,
    pub fee: Uint128,
}
