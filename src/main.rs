mod pnl;

use pnl::{Position, PositionSide};
use rust_decimal_macros::dec;

fn main() {
    let position = Position::new(
        PositionSide::Long,
        dec!(100),        // entry price
        dec!(1),          // size (e.g. 1 BTC)
        dec!(10),         // 10x leverage
        dec!(0.005),      // 0.5% maintenance margin
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
}
