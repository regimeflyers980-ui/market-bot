use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

/// Which direction the position is betting on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PositionSide {
    Long,
    Short,
}

/// A single open position with everything needed to price it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub side: PositionSide,
    pub entry_price: Decimal,
    pub size: Decimal,
    pub leverage: Decimal,
    pub maintenance_margin_rate: Decimal,
}

impl Position {
    /// Build a new position. Caller is responsible for sane inputs.
    pub fn new(
        side: PositionSide,
        entry_price: Decimal,
        size: Decimal,
        leverage: Decimal,
        maintenance_margin_rate: Decimal,
    ) -> Self {
        Self {
            side,
            entry_price,
            size,
            leverage,
            maintenance_margin_rate,
        }
    }

    /// Total contract value: entry * size (in quote currency).
    pub fn notional(&self) -> Decimal {
        self.entry_price * self.size
    }

    /// Collateral locked up. With leverage L, margin = notional / L.
    pub fn margin(&self) -> Decimal {
        self.notional() / self.leverage
    }

    /// Profit/loss at a given mark price, before fees.
    pub fn calculate_pnl(&self, current_price: Decimal) -> Decimal {
        match self.side {
            PositionSide::Long => (current_price - self.entry_price) * self.size,
            PositionSide::Short => (self.entry_price - current_price) * self.size,
        }
    }

    /// Return on equity, expressed as a percentage (e.g. `50.0` = 50%).
    pub fn calculate_roe(&self, current_price: Decimal) -> Decimal {
        let margin = self.margin();
        if margin.is_zero() {
            return Decimal::ZERO;
        }
        (self.calculate_pnl(current_price) / margin) * dec!(100)
    }

    /// Isolated-margin liquidation price (Binance/Bybit convention).
    pub fn liquidation_price(&self) -> Decimal {
        let inv_leverage = Decimal::ONE / self.leverage;
        match self.side {
            PositionSide::Long => {
                self.entry_price * (Decimal::ONE - inv_leverage + self.maintenance_margin_rate)
            }
            PositionSide::Short => {
                self.entry_price * (Decimal::ONE + inv_leverage - self.maintenance_margin_rate)
            }
        }
    }

    /// True when the mark price has crossed the liquidation line.
    pub fn is_liquidated(&self, current_price: Decimal) -> bool {
        match self.side {
            PositionSide::Long => current_price <= self.liquidation_price(),
            PositionSide::Short => current_price >= self.liquidation_price(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn long_10x() -> Position {
        Position::new(PositionSide::Long, dec!(100), dec!(1), dec!(10), dec!(0.005))
    }

    fn short_10x() -> Position {
        Position::new(PositionSide::Short, dec!(100), dec!(1), dec!(10), dec!(0.005))
    }

    #[test]
    fn margin_is_notional_over_leverage() {
        assert_eq!(long_10x().margin(), dec!(10));
    }

    #[test]
    fn long_pnl_positive_when_price_rises() {
        assert_eq!(long_10x().calculate_pnl(dec!(105)), dec!(5));
    }

    #[test]
    fn long_pnl_negative_when_price_falls() {
        assert_eq!(long_10x().calculate_pnl(dec!(95)), dec!(-5));
    }

    #[test]
    fn short_pnl_positive_when_price_falls() {
        assert_eq!(short_10x().calculate_pnl(dec!(95)), dec!(5));
    }

    #[test]
    fn short_pnl_negative_when_price_rises() {
        assert_eq!(short_10x().calculate_pnl(dec!(105)), dec!(-5));
    }

    #[test]
    fn roe_reflects_leverage() {
        // +5% price move on 10x = 50% ROE
        assert_eq!(long_10x().calculate_roe(dec!(105)), dec!(50));
    }

    #[test]
    fn long_liquidation_price_matches_isolated_formula() {
        // 100 * (1 - 0.1 + 0.005) = 90.5
        assert_eq!(long_10x().liquidation_price(), dec!(90.5));
    }

    #[test]
    fn short_liquidation_price_matches_isolated_formula() {
        // 100 * (1 + 0.1 - 0.005) = 109.5
        assert_eq!(short_10x().liquidation_price(), dec!(109.5));
    }

    #[test]
    fn detects_long_liquidation() {
        assert!(long_10x().is_liquidated(dec!(90)));
        assert!(!long_10x().is_liquidated(dec!(91)));
    }

    #[test]
    fn detects_short_liquidation() {
        assert!(short_10x().is_liquidated(dec!(110)));
        assert!(!short_10x().is_liquidated(dec!(109)));
    }

    #[test]
    fn no_panic_on_zero_size() {
        let p = Position::new(PositionSide::Long, dec!(100), dec!(0), dec!(10), dec!(0.005));
        assert_eq!(p.calculate_roe(dec!(105)), dec!(0));
    }
}
