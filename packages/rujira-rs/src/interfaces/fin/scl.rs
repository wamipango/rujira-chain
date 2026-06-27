//! Linear skew concentrated liquidity (SCL)
//!
//! Extends the XY=K concentrated liquidity model with a skew parameter σ ∈ (-2, 2)
//! that applies a linear, non-symmetric redistribution across the price range.
//!
//! Avoid using `skew` for now until the symmetric quadratic formula has been added.
//!
//! Weight function: w(p) = 1 + σ·(p - p_m)/Δp
//!
//! - σ > 0: linearly tilts weight toward higher prices
//! - σ < 0: linearly tilts weight toward lower prices
//! - σ = 0: uniform distribution (reduces to standard CL)
//!
//! Reserve equations:
//! - y = L₀[α(s - s_a) + (β/3)(s³ - s_a³)]
//! - x = L₀(s_b - s)[α/(s·s_b) + β]
//!
//! where:
//! - s = √price, s_a = √low, s_b = √high
//! - β = σ/Δp, α = 1 - β·p_m
//! - p_m = (high + low)/2, Δp = high - low

use cosmwasm_std::{Decimal, Decimal256, SignedDecimal};

use crate::{
    fin::{Side, Tick},
    DecimalDyn, SignedDecimal256,
};
/// Linear skew concentrated liquidity calculator
#[derive(Clone, Debug, PartialEq)]
pub struct Scl {
    /// sqrt(low)
    s_a: Decimal256,
    /// sqrt(high)
    s_b: Decimal256,
    /// Skew coefficient: α = 1 - β·p_m
    alpha: SignedDecimal256,
    /// Skew coefficient: β = σ/Δp
    beta: SignedDecimal256,
}

impl Scl {
    /// Create a new linear SCL calculator from range bounds and skew.
    /// Avoid using `skew` for now until the symmetric quadratic formula has been added.
    pub fn new(high: Decimal, low: Decimal, skew: SignedDecimal) -> Self {
        let s_a = Decimal256::from(low).sqrt();
        let s_b = Decimal256::from(high).sqrt();
        let p_m = Decimal256::from((high + low) / Decimal::from_ratio(2u64, 1u64));
        let delta_p = Decimal256::from(high - low);

        // β = σ / Δp  (σ constrained to (-2, 2) at validation)
        let beta = if delta_p.is_zero() {
            SignedDecimal256::zero()
        } else {
            SignedDecimal256::from(skew) / SignedDecimal256::from(delta_p)
        };

        // α = 1 - β·p_m
        let alpha = SignedDecimal256::one() - beta * SignedDecimal256::from(p_m);

        Self {
            s_a,
            s_b,
            alpha,
            beta,
        }
    }

    /// Y(s) = α(s - s_a) + (β/3)(s³ - s_a³)
    fn y_at(&self, s: Decimal256) -> SignedDecimal256 {
        let s_sd = SignedDecimal256::from(s);
        let s_a_sd = SignedDecimal256::from(self.s_a);
        let diff = s_sd - s_a_sd;
        let s_cubed = SignedDecimal256::from(s * s * s);
        let s_a_cubed = SignedDecimal256::from(self.s_a * self.s_a * self.s_a);
        let cubic = s_cubed - s_a_cubed;
        let three = SignedDecimal256::from(3u64);

        self.alpha * diff + self.beta * cubic / three
    }

    /// X(s) = (s_b - s)(α/(s·s_b) + β)
    fn x_at(&self, s: Decimal256) -> SignedDecimal256 {
        let diff = SignedDecimal256::from(self.s_b) - SignedDecimal256::from(s);
        let denom = SignedDecimal256::from(s * self.s_b);

        if denom.is_zero() {
            return SignedDecimal256::zero();
        }

        diff * (self.alpha / denom + self.beta)
    }

    /// Y'(s) = α + β·s²
    fn y_prime(&self, s: Decimal256) -> SignedDecimal256 {
        self.alpha + self.beta * SignedDecimal256::from(s * s)
    }

    /// X'(s) = -α/s² - β
    fn x_prime(&self, s: Decimal256) -> SignedDecimal256 {
        let s_sq = SignedDecimal256::from(s * s);
        if s_sq.is_zero() {
            return SignedDecimal256::zero();
        }
        -self.alpha / s_sq - self.beta
    }

    /// Solve for price from reserves using Newton-Raphson.
    /// Finds s such that F(s) = x·Y(s) - y·X(s) = 0.
    pub fn price(&self, base: Decimal, quote: Decimal) -> Decimal {
        if quote.is_zero() {
            return Decimal::try_from(self.s_a * self.s_a).unwrap_or(Decimal::zero());
        }
        if base.is_zero() {
            return Decimal::try_from(self.s_b * self.s_b).unwrap_or(Decimal::MAX);
        }

        let x = SignedDecimal256::from(base);
        let y = SignedDecimal256::from(quote);

        let mut s = (self.s_a * self.s_b).sqrt();

        for _ in 0..6 {
            let f = x * self.y_at(s) - y * self.x_at(s);
            let f_prime = x * self.y_prime(s) - y * self.x_prime(s);

            if f_prime.is_zero() {
                break;
            }

            let delta = f / f_prime;
            s = if delta.is_negative() {
                s + delta.abs()
            } else if delta.abs() > s {
                self.s_a
            } else {
                s - delta.abs()
            };

            s = s.max(self.s_a).min(self.s_b);
        }

        Decimal::try_from(s * s).unwrap_or(Decimal::zero())
    }

    /// Compute ask price with offset, truncated to tick.
    pub fn ask(&self, price: Decimal, offset: Decimal, tick: &Tick) -> Decimal {
        tick.truncate_ceil(&(price + price * offset))
    }

    /// Compute bid price with offset, truncated to tick.
    pub fn bid(&self, price: Decimal, offset: Decimal, tick: &Tick) -> Decimal {
        tick.truncate_floor(&price.saturating_sub(price * offset))
    }

    /// Compute the quote/base balance ratio (Y(s)/X(s)) at a given price.
    /// Used to determine how to split funds between base and quote when creating a range.
    pub fn balance_ratio(&self, price: Decimal) -> Option<Decimal> {
        let s = Decimal256::from(price).sqrt();
        let x_s = self.x_at(s);

        if x_s.is_zero() {
            return None;
        }

        (self.y_at(s) / x_s).to_decimal()
    }

    /// Compute the curve-effective price (ΔY/ΔX) for a price slice [p_start, p_end].
    /// L₀ cancels out, so this depends only on curve shape.
    pub fn effective_price(&self, p_start: Decimal, p_end: Decimal) -> Option<Decimal256> {
        let s_start = Decimal256::from(p_start).sqrt().max(self.s_a).min(self.s_b);
        let s_end = Decimal256::from(p_end).sqrt().max(self.s_a).min(self.s_b);

        if s_start == s_end {
            return None;
        }

        let dy = self.y_at(s_end) - self.y_at(s_start);
        let dx = self.x_at(s_start) - self.x_at(s_end);

        if dx.is_zero() {
            return None;
        }

        (dy / dx).to_decimal256()
    }

    /// Compute the position liquidity from its current reserves and price.
    ///
    /// This can be cached until the reserves change. Keeping it separate from
    /// [`amount_in_slice`](Self::amount_in_slice) avoids recalculating the same
    /// curve position for every price slice emitted by an order-book iterator.
    pub fn liquidity(&self, base: Decimal, quote: Decimal, price: Decimal) -> DecimalDyn {
        let s_price = Decimal256::from(price).sqrt();
        let y_s = self.y_at(s_price);
        let x_s = self.x_at(s_price);

        let liquidity = if !quote.is_zero() && !y_s.is_zero() {
            (SignedDecimal256::from(quote) / y_s).to_decimal256_or_zero()
        } else if !base.is_zero() && !x_s.is_zero() {
            (SignedDecimal256::from(base) / x_s).to_decimal256_or_zero()
        } else {
            Decimal256::zero()
        };
        DecimalDyn::from(liquidity)
    }

    /// Compute the amount per unit of liquidity and effective price for one
    /// slice. Both values share the same square roots and curve evaluations.
    pub fn slice(
        &self,
        side: &Side,
        p_start: Decimal,
        p_end: Decimal,
    ) -> Option<(DecimalDyn, Decimal256)> {
        let s_start = Decimal256::from(p_start).sqrt().max(self.s_a).min(self.s_b);
        let s_end = Decimal256::from(p_end).sqrt().max(self.s_a).min(self.s_b);

        if s_start == s_end {
            return None;
        }

        let y_start = self.y_at(s_start);
        let y_end = self.y_at(s_end);
        let x_start = self.x_at(s_start);
        let x_end = self.x_at(s_end);
        let dy = y_end - y_start;
        let dx = x_start - x_end;

        if dx.is_zero() {
            return None;
        }

        let amount_per_liquidity = match side {
            Side::Base => dx,
            Side::Quote => -dy,
        }
        .to_decimal256()?;
        let effective_price = (dy / dx).to_decimal256()?;

        Some((DecimalDyn::from(amount_per_liquidity), effective_price))
    }

    /// Compute amount of tokens available in a price slice [p_start, p_end]
    pub fn amount_in_slice(
        &self,
        side: &Side,
        base: Decimal,
        quote: Decimal,
        price: Decimal,
        p_start: Decimal,
        p_end: Decimal,
    ) -> Decimal {
        let s_start = Decimal256::from(p_start).sqrt().max(self.s_a).min(self.s_b);
        let s_end = Decimal256::from(p_end).sqrt().max(self.s_a).min(self.s_b);

        if s_start == s_end {
            return Decimal::zero();
        }

        let s_price = Decimal256::from(price).sqrt();

        // Compute L₀ from reserves.
        // Use quote / Y(s_price) when quote is available, otherwise base / X(s_price).
        let y_s = self.y_at(s_price);
        let x_s = self.x_at(s_price);
        let l0 = if !quote.is_zero() && !y_s.is_zero() {
            SignedDecimal256::from(quote) / y_s
        } else if !base.is_zero() && !x_s.is_zero() {
            SignedDecimal256::from(base) / x_s
        } else {
            return Decimal::zero();
        };

        match side {
            Side::Base => {
                // Asks: s_start < s_end, X decreasing → X(s_start) > X(s_end)
                (l0 * (self.x_at(s_start) - self.x_at(s_end))).to_decimal_or_zero()
            }
            Side::Quote => {
                // Bids: s_start > s_end, Y increasing → Y(s_start) > Y(s_end)
                (l0 * (self.y_at(s_start) - self.y_at(s_end))).to_decimal_or_zero()
            }
        }
    }
}
