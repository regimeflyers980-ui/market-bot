use crate::pnl::Position;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

/// Hard caps the bot will never cross.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskLimits {
    pub max_position_notional: Decimal,
    pub max_leverage: Decimal,
    pub max_drawdown_pct: Decimal,
    pub max_open_positions: usize,
}

/// What the risk manager decides about a proposed position.
#[derive(Debug, Clone, PartialEq)]
pub enum RiskDecision {
    Accept,
    Resize(Decimal),
    Reject(&'static str),
}

/// Tracks equity history and enforces RiskLimits.
pub struct RiskManager {
    limits: RiskLimits,
    peak_equity: Decimal,
}

impl RiskManager {
    pub fn new(limits: RiskLimits, starting_equity: Decimal) -> Self {
        Self {
            limits,
            peak_equity: starting_equity,
        }
    }

    /// Call this every time account equity changes (mark-to-market).
    pub fn update_equity(&mut self, equity: Decimal) {
        if equity > self.peak_equity {
            self.peak_equity = equity;
        }
    }

    /// Current drawdown as a percentage of peak equity.
    pub fn current_drawdown_pct(&self, equity: Decimal) -> Decimal {
        if self.peak_equity.is_zero() {
            return Decimal::ZERO;
        }
        ((self.peak_equity - equity) / self.peak_equity) * dec!(100)
    }

    /// Evaluate a proposed position against the rules.
    /// Order matters: cheapest checks first, cheapest failures short-circuit.
    pub fn evaluate(
        &self,
        proposed: &Position,
        current_price: Decimal,
        open_count: usize,
        equity: Decimal,
    ) -> RiskDecision {
        if proposed.is_liquidated(current_price) {
            return RiskDecision::Reject("already liquidated");
        }

        if proposed.leverage > self.limits.max_leverage {
            return RiskDecision::Reject("leverage exceeds cap");
        }

        if open_count >= self.limits.max_open_positions {
            return RiskDecision::Reject("max open positions reached");
        }

        if self.current_drawdown_pct(equity) >= self.limits.max_drawdown_pct {
            return RiskDecision::Reject("drawdown limit breached");
        }

        let notional = proposed.notional();
        if notional > self.limits.max_position_notional {
            let scale = self.limits.max_position_notional / notional;
            return RiskDecision::Resize(proposed.size * scale);
        }

        RiskDecision::Accept
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pnl::PositionSide;

    fn limits() -> RiskLimits {
        RiskLimits {
            max_position_notional: dec!(500),
            max_leverage: dec!(20),
            max_drawdown_pct: dec!(20),
            max_open_positions: 5,
        }
    }

    fn manager() -> RiskManager {
        RiskManager::new(limits(), dec!(1000))
    }

    fn pos(leverage: Decimal, size: Decimal) -> Position {
        Position::new(PositionSide::Long, dec!(100), size, leverage, dec!(0.005))
    }

    #[test]
    fn accepts_position_within_all_limits() {
        let m = manager();
        let p = pos(dec!(10), dec!(1));
        assert_eq!(
            m.evaluate(&p, dec!(100), 0, dec!(1000)),
            RiskDecision::Accept
        );
    }

    #[test]
    fn rejects_when_leverage_too_high() {
        let m = manager();
        let p = pos(dec!(50), dec!(1));
        assert_eq!(
            m.evaluate(&p, dec!(100), 0, dec!(1000)),
            RiskDecision::Reject("leverage exceeds cap")
        );
    }

    #[test]
    fn rejects_when_max_open_positions_reached() {
        let m = manager();
        let p = pos(dec!(10), dec!(1));
        assert_eq!(
            m.evaluate(&p, dec!(100), 5, dec!(1000)),
            RiskDecision::Reject("max open positions reached")
        );
    }

    #[test]
    fn resizes_when_notional_too_large() {
        let m = manager();
        // notional = 100 * 10 = 1000, cap = 500 -> resize to half
        let p = pos(dec!(10), dec!(10));
        match m.evaluate(&p, dec!(100), 0, dec!(1000)) {
            RiskDecision::Resize(size) => assert_eq!(size, dec!(5)),
            other => panic!("expected Resize, got {:?}", other),
        }
    }

    #[test]
    fn rejects_when_drawdown_breached() {
        let mut m = manager();
        m.update_equity(dec!(1000)); // peak = 1000
        let p = pos(dec!(10), dec!(1));
        // equity 750 = 25% drawdown, above 20% cap
        assert_eq!(
            m.evaluate(&p, dec!(100), 0, dec!(750)),
            RiskDecision::Reject("drawdown limit breached")
        );
    }

    #[test]
    fn drawdown_uses_peak_not_starting_balance() {
        let mut m = manager(); // starts at 1000
        m.update_equity(dec!(1500)); // peak = 1500
        // 1500 -> 1200 is 20% drawdown, NOT a 20% gain
        assert_eq!(m.current_drawdown_pct(dec!(1200)), dec!(20));
    }

    #[test]
    fn rejects_when_already_liquidated() {
        let m = manager();
        // 20x long at 100: liquidation ~= 100*(1-0.05+0.005) = 95.5
        let p = pos(dec!(20), dec!(1));
        // at price 95 the position is dead
        assert_eq!(
            m.evaluate(&p, dec!(95), 0, dec!(1000)),
            RiskDecision::Reject("already liquidated")
        );
    }
}
