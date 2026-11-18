mod backtest;
mod pnl;
mod risk;

use backtest::{synthetic_candles, Backtest, SmaCross};
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
        manager.current_drawdown_pct(dec!(1100)).round_dp(2)
    );

    let decision = manager.evaluate(&position, current_price, 0, dec!(1100));
    match decision {
        RiskDecision::Accept => println!("Decision: ACCEPT"),
        RiskDecision::Resize(new_size) => println!("Decision: RESIZE to {}", new_size),
        RiskDecision::Reject(reason) => println!("Decision: REJECT — {}", reason),
    }

    // ---- Backtest ----
    println!();
    println!("=== Backtest ===");
    let candles = synthetic_candles(500);
    let strategy = SmaCross::new(10, 30);
    let backtest = Backtest::new(dec!(1000), dec!(1)).with_fees(dec!(0.001));
    let result = backtest.run(&candles, strategy);

    println!("Bars:            {}", candles.len());
    println!("Trades:          {}", result.trades.len());
    println!("Starting equity: {}", result.starting_equity);
    println!("Final equity:    {}", result.final_equity.round_dp(2));
    println!("Total PnL:       {}", result.total_pnl().round_dp(2));
    println!("Return:          {}%", result.total_return_pct().round_dp(2));
    println!("Peak equity:     {}", result.peak_equity.round_dp(2));
    println!("Max drawdown:    {}%", result.max_drawdown_pct.round_dp(2));
    println!("Win rate:        {}%", result.win_rate_pct().round_dp(2));
    println!("Profit factor:   {}", result.profit_factor().round_dp(2));
    println!("Sharpe (raw):    {}", result.sharpe().round_dp(3));
}
