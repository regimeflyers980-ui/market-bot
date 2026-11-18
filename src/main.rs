mod pnl;
mod risk;

use pnl::{Position, PositionSide};
use risk::{RiskDecision, RiskLimits, RiskManager};
use rust_decimal_macros::dec;

fn main() {
    let position = Position::new(
        PositionSide::Long,
        dec!(100),
        dec!(1),
        dec!(10),
        dec!(0.005),
    );

    let current_price = dec!(105);

    println!("=== Position ===");
    println!("Side: {:?}", position.side);
    println!(
        "Entry: {} | Size: {} | Leverage: {}x",
        position.entry_price, position.size, position.leverage
    );
    println!(
        "Notional: {} | Margin: {}",
        position.notional(),
        position.margin()
    );
    println!("Liquidation Price: {}", position.liquidation_price());
    println!();
    println!("=== @ {} ===", current_price);
    println!("PnL: {}", position.calculate_pnl(current_price));
    println!("ROE: {}%", position.calculate_roe(current_price));
    println!("Liquidated? {}", position.is_liquidated(current_price));
    println!();

    let limits = RiskLimits {
        max_position_notional: dec!(500),
        max_leverage: dec!(20),
        max_drawdown_pct: dec!(20),
        max_open_positions: 5,
    };
    let mut manager = RiskManager::new(limits, dec!(1000));
    manager.update_equity(dec!(1200));

    println!("=== Risk check @ {} ===", current_price);
    println!(
        "Drawdown: {}%",
        manager.current_drawdown_pct(dec!(1100))
    );

    let decision = manager.evaluate(&position, current_price, 0, dec!(1100));
    match decision {
        RiskDecision::Accept => println!("Decision: ACCEPT"),
        RiskDecision::Resize(new_size) => println!("Decision: RESIZE to {}", new_size),
        RiskDecision::Reject(reason) => println!("Decision: REJECT — {}", reason),
    }
}
