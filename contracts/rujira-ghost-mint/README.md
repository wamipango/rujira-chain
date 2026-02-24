# Rujira Ghost Mint

Rujira Ghost Mint is the mint-based credit leg of the Rujira markets stack. It is designed to provide a capped, interest-bearing debt source for minted assets (e.g., perp synths), while remaining fully compatible with the Rujira Credit Account and Rujira FIN orderbook primitives.

This README is written as a reference for integrators and LLMs that need a concise but complete view of how the perps system works, how Ghost Mint fits into it, and where the integration opportunities are.

## System Overview

Rujira’s spot + perps architecture is a composable set of on-chain contracts:

- `rujira-fin` is the orderbook primitive. It merges on-chain limit order pools (fixed price levels), oracle-referenced pools (price bands around an oracle), and CCL Ranges (a native Range primitive that maps a skewed, spread-adjusted curve over a high/low price band into on-orderbook bids/asks).
- `rujira-thorchain-swap` connects FIN liquidity to THORChain base layer pools by routing quotes and swaps through THORChain and borrowing the returned funds from Ghost Vaults. This makes base-layer liquidity available inside the orderbook.
- `rujira-ghost-credit` is the generalized credit account system. It supports multi-collateral positions, enforces risk limits, and provides transparent liquidations with user-defined liquidation preferences.
- `rujira-ghost-vault` is the spot lending pool (deposits + borrows, receipt tokens, interest distribution).
- `rujira-ghost-mint` is the mint-side credit pool. It provides borrowable capacity for minted assets, subject to a global mint cap and per-borrower limits.

Together, this enables a perps market that is:

- **Transparent**: all debt, collateral, and liquidation logic is on-chain.
- **Composable**: orders, swaps, liquidations, and funding routes are composed via standard CosmWasm calls.
- **Fair**: FIN guarantees on-chain matching with deterministic execution, and liquidations are constrained by explicit rules and user preferences.

## Perps Market Design (How Ghost Mint Enables It)

Rujira perps are created by pairing:

1. A **minted perp asset** (issued via `rujira-ghost-mint`), and
2. A **spot asset** (borrowed from `rujira-ghost-vault` or held as collateral),

then connecting the two with **Funding Markets** on FIN:

- Funding is a FIN trading pair between the spot asset (e.g., `USDC`) and the perp asset (e.g., `perpUSDC`).
- This produces a continuous, market-driven funding mechanism, where arbitrage is democratized by the orderbook + market maker primitives rather than limited to privileged actors.
- Custom liquidity profiles (e.g., tight ranges around 1.0 with skew/spread control) are expressed via FIN’s CCL Range primitive. A Range defines a high/low band, skew, and spread, then distributes base/quote across that band on the orderbook with a best bid/ask derived from the center price.

### Funding Markets in Practice (Open, Win, Close, Lose)

Funding Markets are where the **spot asset** and the **minted perp asset** trade against each other. This has direct implications for how positions are opened and closed:

- **Opening a position (spot asset margin)**:
  - A trader borrows or posts spot collateral (e.g., `USDC`) via `rujira-ghost-credit`.
  - That spot is swapped on the Funding Market (`USDC -> perpUSDC`), and **combined with freshly minted `perpUSDC`** from Ghost Mint.
  - The resulting perp balance is then swapped on a perp pair (e.g., `perpUSDC/perpBTC`) to obtain `perpBTC`, which can be held as collateral inside the Credit Account.
  - This creates a synthetic exposure while keeping the debt and collateral tracked by the Credit Account.
  - **Funding Market impact**: opening positions creates **buy pressure** on `perpUSDC` against `USDC`, tightening the funding band and rewarding CCL Range liquidity on the bid side.
- **Closing a position (“winning”)**:
  - The trader unwinds by swapping `perpBTC -> perpUSDC`, then **selling `perpUSDC -> USDC`** on the Funding Market to exit spot.
  - The proceeds repay debt or increase collateral, reducing LTV and realizing PnL.
  - **Funding Market impact**: closing positions creates **sell pressure** on `perpUSDC` against `USDC`, restoring balance toward 1.0 and rewarding liquidity on the ask side.
- **Losing a position (“liquidate”)**:
  - If LTV breaches the liquidation threshold, `rujira-ghost-credit` triggers a liquidation route.
  - The route can unwind `perpBTC -> perpUSDC` and then repay debt directly in `perpUSDC` (no Funding Market sale to `USDC`).
  - **Funding Market impact**: liquidations do **not** force `perpUSDC` sell flow into `USDC`; impact is minimal or indirect compared to voluntary closes.

### The “Cheap Perp” Opportunity (USDC/perpUSDC Example)

Because funding is expressed as **spot-perp orderflow**, the Funding Market itself becomes the arb venue:

- If `perpUSDC` trades **below 1.0** vs `USDC`, the perp is “cheap”.
- A trader can **buy cheap perpUSDC with USDC**, then later close the position when the perp returns toward 1.0.
- This trade is fully on-chain and uses the same FIN orderbook mechanics as spot trading.

**How you’d use it (high-level flow):**

1. Open/adjust a Credit Account with spot collateral.
2. Borrow spot via Ghost Vault (if needed).
3. Swap `USDC -> perpUSDC` on the Funding Market (FIN).
4. Wait for the perp to converge upward toward 1.0.
5. Swap `perpUSDC -> USDC` on the Funding Market.
6. Repay debt / realize profit; LTV improves.

This is the perps equivalent of capturing a favorable funding basis, but it is **democratized**: any participant can take the trade by using FIN’s orderbook and CCL Ranges.

### Why Settlement Is Different From Typical Perps

Traditional perps typically force a position to settle into the quote asset (e.g., USDC) on close or liquidation. In Rujira:

- The perp asset (`perpUSDC`) is a first-class on-chain token.
- You can **hold the perp token** after closing and use it as input for another position, rather than being forced to settle entirely in spot.
- You can **exit more orderly** by placing a Funding Market limit order (or providing a Range) instead of accepting immediate market impact.

This shifts settlement from a centralized, forced conversion into an open, orderbook-native flow where you control timing, price, and reuse of capital.

## Why Rujira Is a Superior Trading Venue

Compared to traditional perps venues, Rujira:

- **Eliminates black-box liquidation engines**: `rujira-ghost-credit` enforces liquidation rules on-chain and lets account owners define liquidation preferences.
- **Provides on-chain orderbook + liquidity routing**: FIN can merge native limit liquidity, oracle-priced liquidity, and THORChain-backed liquidity into a single deterministic execution path.
- **Democratizes funding arbitrage**: funding pressure is expressed as spot-perp orderflow, enabling any participant to provide liquidity or arb using CCL Ranges.
- **Reduces trust surface**: all prices used for collateral valuation and liquidation are oracle-based and verifiable on-chain.

## Stablecoin Model (perpUSD)

Rujira supports a stablecoin-style asset (e.g., `perpUSD`) as a **minted perp token** that is **valued at $1** by the Credit Account module. This creates a hybrid model that combines:

- **MakerDAO-style overcollateralized issuance**: users mint `perpUSD` against collateral under explicit LTV limits and liquidation rules.
- **Ethena-style market-based stability**: the peg is reinforced by market incentives and basis trades on the Funding Markets.

### Funding Markets as Decentralized Stability

The `SPOT/perpUSD` orderbooks are the **decentralized funding markets** for the stablecoin:

- If `perpUSD` trades below $1, arbitrageurs buy `perpUSD` with spot dollars, pushing it up toward peg.
- If `perpUSD` trades above $1, arbitrageurs sell `perpUSD` into spot, pushing it down toward peg.

Because these flows are executed on FIN’s orderbook (with CCL Ranges and limit liquidity), the stability mechanism is transparent, on-chain, and permissionless.

### Credit Accounts as Issuance + Stability Engine

The Credit Account system ties together issuance and stability:

- **Issuance**: users borrow/mint `perpUSD` against collateral in a fully on-chain account.
- **Market discipline**: positions are continuously mark-to-oracle and liquidations are enforced transparently.
- **Stability**: the Funding Market provides the primary venue for peg maintenance and deleveraging flows.

Together, overcollateralized borrowing plus the perp market’s funding dynamics turn `perpUSD` into a stablecoin with an explicit, on-chain issuance and stability engine.

## Ghost Mint: Contract Dynamics

Ghost Mint is a specialized pool for mint-side debt. It shares structural patterns with Ghost Vaults, but **does not accept deposits**. Instead, it:

- Tracks total minted debt and debt shares.
- Enforces a global `mint_cap`.
- Allows only **whitelisted borrowers** (set by sudo) to borrow.
- Accrues interest over time and charges a protocol fee.

### Core Data Model

- **Config**: `denom`, interest model, `fee`, `fee_address`, `mint_cap`. See `contracts/rujira-ghost-mint/src/config.rs`.
- **State**: `minted`, `debt_pool` (SharePool), interest accrual fields. See `contracts/rujira-ghost-mint/src/state.rs`.
- **Borrower**: address, `limit`, `shares`. See `contracts/rujira-ghost-mint/src/borrowers.rs`.

### Interest & Utilization

- Interest accrues in `State::distribute_interest` whenever `execute` or `query` is called.
- Debt interest is charged to the **debt pool**, increasing liabilities for all borrowers.
- Utilization is computed as `1 - (minted / mint_cap)` in `State::utilization`. This value drives the interest curve.
- Protocol fees are minted as **receipt tokens** (token-factory subdenom `ghost-mint/{denom}`) to `fee_address`.

### Borrow / Repay Flow

Borrowing is restricted to whitelisted borrowers and capped by:

- `mint_cap - debt_pool.size()` (global capacity), and
- per-borrower `limit` (stored in `Borrower`).

Repayment burns debt shares proportional to the amount repaid, with automatic refund if excess funds are sent.

## Execute & Query Reference (Ghost Mint)

This contract exposes **only** `Execute` (via `market`) and `Query` entry points. There are no public `Instantiate` or `Sudo` examples here by request.

### JSON Encoding Notes (Execute)

Execute messages are nested enums. In JSON:

- The top-level key is the enum variant name in `snake_case`.
- `ExecuteMsg::Market(MarketMsg)` becomes `{ "market": { ... } }`.
- The inner `MarketMsg` variant becomes the next key: `borrow` or `repay`.
- Numeric values (amounts, limits, etc.) are encoded as strings.

In other words, the JSON shape mirrors:

```rust
ExecuteMsg::Market(MarketMsg::Borrow { ... })
```

as:

```json
{ "market": { "borrow": { ... } } }
```

### Execute: Borrow (All Variants)

Borrow with no callback:

```json
{
  "market": {
    "borrow": {
      "amount": "1000000",
      "callback": null
    }
  }
}
```

Borrow with a callback (send funds to a contract with a payload):

```json
{
  "market": {
    "borrow": {
      "amount": "1000000",
      "callback": {
        "contract": "rujira1callback...",
        "msg": "eyJzdGF0dXMiOiAiY2FsbGJhY2sifQ=="
      }
    }
  }
}
```

### Execute: Repay (All Variants)

Repay your own debt (must send `config.denom` as funds):

```json
{
  "market": {
    "repay": {}
  }
}
```

### Queries (All Variants)

Config:

```json
{ "config": {} }
```

Status:

```json
{ "status": {} }
```

Borrower:

```json
{ "borrower": { "addr": "rujira1..." } }
```

Borrowers (paged):

```json
{ "borrowers": { "limit": 50, "start_after": "rujira1..." } }
```

## Integration Opportunities

If you are integrating or trading with Rujira, Ghost Mint enables several high-leverage primitives:

- **Funding Arbitrage**: trade spot vs perp in FIN to capture funding pressure.
- **Market Making**: deploy CCL Ranges (skew/spread across a defined band) to shape spreads around 1.0 for funding markets.
- **Liquidation Solvers**: route collateral swaps through FIN and/or THORChain swap to satisfy Ghost Credit liquidations.
- **Risk-Aware Credit Products**: use Ghost Mint debt inside Credit Accounts with user-defined liquidation preferences.

## How It Connects to Credit Accounts

Credit Accounts (`rujira-ghost-credit` + `rujira-account`) can:

- hold both spot collateral and minted assets,
- borrow from Ghost Vaults (spot) and Ghost Mint (perp),
- execute arbitrary swaps on FIN, and
- specify liquidation preferences to constrain liquidation routes.

Ghost Mint’s delegation mechanism makes it straightforward to allocate minted debt to a Credit Account while keeping mint limits enforced at the borrower level.

## Implementation Notes (For Auditors & LLMs)

- **State updates are interest-aware**: `execute` and `query` both call `State::distribute_interest` before returning results.
- **Borrow capacity**: `capacity = mint_cap - debt_pool.size()` (see `State::capacity`).
- **Per-borrower limit enforcement**: `Borrower::borrow` checks pool ownership against `limit`.
- **Repay share math**: repayment burns debt shares proportional to the outstanding pool size.
- **Asset delivery**: borrow execution transfers `config.denom` from the contract balance via `BankMsg::Send`. Production deployments should ensure the denom is available to the contract (e.g., pre-funded inventory or mint authority external to this contract).
- **Events**:
  - `rujira-ghost-mint/borrow`
  - `rujira-ghost-mint/repay`

## Related Contracts

- `contracts/rujira-fin`
- `contracts/rujira-ghost-credit`
- `contracts/rujira-ghost-vault`
- `contracts/rujira-thorchain-swap`
