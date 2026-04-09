use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Uint128, Uint256};

#[cw_serde]
pub enum Route {
    // A -> RUNE (single hop)
    AR {
        a: Uint128,
        r: Uint128,
    },
    // RUNE -> B (single hop)
    RB {
        r: Uint128,
        b: Uint128,
    },
    // A -> R -> B (two hop)
    ARB {
        a: Uint128,
        r1: Uint128,
        r2: Uint128,
        b: Uint128,
    },
}

impl Route {
    pub fn return_balance(&self) -> Uint128 {
        match *self {
            Route::AR { r, .. } => r,
            Route::RB { b, .. } => b,
            Route::ARB { b, .. } => b,
        }
    }
    pub fn swap(&self, x: Uint128, min_slip_bps: u32) -> Uint128 {
        match *self {
            Route::AR { a, r } => calculate_return(x, a, r, min_slip_bps),
            Route::RB { r, b } => calculate_return(x, r, b, min_slip_bps),
            Route::ARB { a, r1, r2, b } => calculate_return(
                calculate_return(x, a, r1, min_slip_bps),
                r2,
                b,
                min_slip_bps,
            ),
        }
    }

    pub fn size(&self, s: u32) -> Uint128 {
        match *self {
            Route::AR { a, .. } => size_single(a, s),
            Route::RB { r, .. } => size_single(r, s),
            Route::ARB { a, r1, r2, .. } => size_dual(a, r1, r2, s),
        }
    }
}

fn size_single(xx: Uint128, s_bps: u32) -> Uint128 {
    if s_bps == 0 {
        return Uint128::zero();
    }
    xx.multiply_ratio(s_bps, 10_000u128)
}

// See https://gitlab.com/thorchain/thornode/-/blob/develop/x/thorchain/helpers.go#L212-221
fn size_dual(
    oa: Uint128, // asset balance in offer pool (input side of hop1)
    or: Uint128, // rune balance in offer pool (output side of hop1)
    ar: Uint128, // rune balance in ask pool (input side of hop2)
    s_bps: u32,
) -> Uint128 {
    if s_bps == 0 {
        return Uint128::zero();
    }
    // Find smallest value in rune and convert to offer asset
    size_single(or, s_bps)
        .min(size_single(ar, s_bps))
        .multiply_ratio(oa, or)
}

/// CLP swap with min slip floor matching thorchain's GetSwapCalc.
///
/// Standard CLP: emit = x * X * Y / (x + X)^2
///
/// When slip < min_slip_bps, thorchain uses an artificial floor:
///   fee  = min_slip_bps * x * Y / (10000 * (x + X))
///   emit = x * Y / (x + X) - fee
///
/// See thornode swap_current.go GetSwapCalc / CalcMinLiquidityFee / CalcMaxAssetEmission
fn calculate_return(x: Uint128, xx: Uint128, yy: Uint128, min_slip_bps: u32) -> Uint128 {
    if x.is_zero() || xx.is_zero() || yy.is_zero() {
        return Uint128::zero();
    }
    let x = Uint256::from(x);
    let xx = Uint256::from(xx);
    let yy = Uint256::from(yy);
    let denom = x + xx;

    // Standard CLP emission
    let clp_emit = x * xx * yy / denom.pow(2);

    if min_slip_bps == 0 {
        return clp_emit.try_into().unwrap_or(Uint128::zero());
    }

    // slip_bps = x * 10000 / (x + X)
    let slip_bps = x * Uint256::from(10_000u128) / denom;

    if slip_bps >= Uint256::from(min_slip_bps as u128) {
        // Standard CLP applies
        return clp_emit.try_into().unwrap_or(Uint128::zero());
    }

    // Min slip floor: CalcMinLiquidityFee / CalcMaxAssetEmission
    // fee  = GetSafeShare(minSlipBps, 10000, x*Y) / (x+X)
    // emit = x*Y/(x+X) - fee
    let xy = x * yy;
    let min_fee_num =
        (Uint256::from(min_slip_bps as u128) * xy / Uint256::from(10_000u128)).min(xy);
    let min_fee = min_fee_num / denom;
    let gross = xy / denom;
    gross
        .checked_sub(min_fee)
        .unwrap_or(Uint256::zero())
        .try_into()
        .unwrap_or(Uint128::zero())
}

/// Mirrors thornode x/thorchain/swap_current_test.go TestSwap_GetSwapCalc
#[cfg(test)]
mod tests {
    use super::*;

    // "no-min-slip": x=1e7, X=1e8, Y=1e8, minSlipBps=100
    // slip=909 >= 100 → standard CLP, expectedEmit=8264462
    #[test]
    fn thorchain_get_swap_calc_no_min_slip() {
        let emit = calculate_return(
            Uint128::new(10_000_000),
            Uint128::new(100_000_000),
            Uint128::new(100_000_000),
            100,
        );
        assert_eq!(emit.u128(), 8_264_462);
    }

    // "min-slip": x=1e5, X=1e8, Y=1e8, minSlipBps=1000
    // slip=9 < 1000 → min floor, expectedEmit=89910
    #[test]
    fn thorchain_get_swap_calc_min_slip() {
        let emit = calculate_return(
            Uint128::new(100_000),
            Uint128::new(100_000_000),
            Uint128::new(100_000_000),
            1000,
        );
        assert_eq!(emit.u128(), 89_910);
    }
}
