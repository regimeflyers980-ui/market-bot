use rust_decimal::{Decimal, MathematicalOps};
use rust_decimal_macros::dec;

#[derive(Debug, Clone)]
pub struct Candle {
    pub timestamp: i64,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
}

impl Candle {
    pub fn new(
        timestamp: i64,
        open: Decimal,
        high: Decimal,
        low: Decimal,
        close: Decimal,
        volume: Decimal,
    ) -> Self {
        Self { timestamp, open, high, low, close, volume }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Buy,
    Sell,
    Hold,
}

/// Any trading strategy implements this trait.
pub trait Strategy {
    fn on_bar(&mut self, bar: &Candle) -> Signal;
}

// --- A concrete strategy: SMA crossover ---

pub struct SmaCross {
    fast: usize,
    slow: usize,
    closes: Vec<Decimal>,
}

impl SmaCross {
    pub fn new(fast: usize, slow: usize) -> Self {
        assert!(fast < slow, "fast period must be < slow period");
        Self { fast, slow, closes: Vec::new() }
    }

    fn sma(&self, period: usize) -> Option<Decimal> {
        if self.closes.len() < period {
            return None;
        }
        let start = self.closes.len() - period;
        let sum: Decimal = self.closes[start..].iter().sum();
        Some(sum / Decimal::from(period as u64))
    }
}

impl Strategy for SmaCross {
    fn on_bar(&mut self, bar: &Candle) -> Signal {
        self.closes.push(bar.close);

        if self.closes.len() < self.slow + 1 {
            return Signal::Hold;
        }

        let curr_fast = self.sma(self.fast).unwrap();
        let curr_slow = self.sma(self.slow).unwrap();

        let last = self.closes.pop().unwrap();
        let prev_fast = self.sma(self.fast);
        let prev_slow = self.sma(self.slow);
        self.closes.push(last);

        let (prev_fast, prev_slow) = match (prev_fast, prev_slow) {
            (Some(f), Some(s)) => (f, s),
            _ => return Signal::Hold,
        };

        if prev_fast <= prev_slow && curr_fast > curr_slow {
            Signal::Buy
        } else if prev_fast >= prev_slow && curr_fast < curr_slow {
            Signal::Sell
        } else {
            Signal::Hold
        }
    }
}

// --- Trades and results ---

#[derive(Debug, Clone)]
pub struct Trade {
    pub entry_price: Decimal,
    pub exit_price: Decimal,
    pub size: Decimal,
    pub pnl: Decimal,
}

impl Trade {
    pub fn is_win(&self) -> bool {
        self.pnl > Decimal::ZERO
    }
}

#[derive(Debug, Clone)]
pub struct BacktestResult {
    pub starting_equity: Decimal,
    pub final_equity: Decimal,
    pub peak_equity: Decimal,
    pub max_drawdown_pct: Decimal,
    pub trades: Vec<Trade>,
    pub equity_curve: Vec<Decimal>,
}

impl BacktestResult {
    pub fn total_pnl(&self) -> Decimal {
        self.final_equity - self.starting_equity
    }

    pub fn total_return_pct(&self) -> Decimal {
        if self.starting_equity.is_zero() {
            return Decimal::ZERO;
        }
        (self.total_pnl() / self.starting_equity) * dec!(100)
    }

    pub fn win_rate_pct(&self) -> Decimal {
        if self.trades.is_empty() {
            return Decimal::ZERO;
        }
        let wins = self.trades.iter().filter(|t| t.is_win()).count();
        (Decimal::from(wins) / Decimal::from(self.trades.len())) * dec!(100)
    }

    /// Gross profit / gross loss. Returns 999 as a sentinel when there are no losers.
    pub fn profit_factor(&self) -> Decimal {
        let gp: Decimal = self.trades.iter().filter(|t| t.is_win()).map(|t| t.pnl).sum();
        let gl: Decimal = self.trades.iter().filter(|t| !t.is_win()).map(|t| t.pnl.abs()).sum();
        if gl.is_zero() {
            if gp.is_zero() { dec!(0) } else { dec!(999) }
        } else {
            gp / gl
        }
    }

    /// Raw Sharpe ratio (mean / std-dev of per-bar returns). No annualization.
    pub fn sharpe(&self) -> Decimal {
        if self.equity_curve.len() < 2 {
            return Decimal::ZERO;
        }
        let mut returns = Vec::with_capacity(self.equity_curve.len() - 1);
        for w in self.equity_curve.windows(2) {
            if w[0].is_zero() {
                returns.push(Decimal::ZERO);
                continue;
            }
            returns.push((w[1] - w[0]) / w[0]);
        }
        let n = Decimal::from(returns.len() as u64);
        let mean: Decimal = returns.iter().sum::<Decimal>() / n;
        let variance: Decimal = returns
            .iter()
            .map(|r| { let d = *r - mean; d * d })
            .sum::<Decimal>() / n;
        let std_dev = variance.sqrt().unwrap_or(Decimal::ZERO);
        if std_dev.is_zero() {
            return Decimal::ZERO;
        }
        mean / std_dev
    }
}

// --- The engine ---

pub struct Backtest {
    starting_equity: Decimal,
    leverage: Decimal,
    fee_rate: Decimal,
}

impl Backtest {
    pub fn new(starting_equity: Decimal, leverage: Decimal) -> Self {
        Self { starting_equity, leverage, fee_rate: Decimal::ZERO }
    }

    pub fn with_fees(mut self, fee_rate: Decimal) -> Self {
        self.fee_rate = fee_rate;
        self
    }

    pub fn run<S: Strategy>(&self, candles: &[Candle], mut strategy: S) -> BacktestResult {
        let mut cash = self.starting_equity;
        let mut equity = self.starting_equity;
        let mut position: Option<(Decimal, Decimal)> = None;
        let mut peak = equity;
        let mut max_dd = Decimal::ZERO;
        let mut trades = Vec::new();
        let mut equity_curve = Vec::with_capacity(candles.len());

        for (i, bar) in candles.iter().enumerate() {
            let signal = strategy.on_bar(bar);

            match signal {
                Signal::Buy if position.is_none() => {
                    if bar.close.is_zero() { continue; }
                    let notional = equity * self.leverage;
                    let size = notional / bar.close;
                    cash -= notional * self.fee_rate;
                    position = Some((bar.close, size));
                }
                Signal::Sell if position.is_some() => {
                    let (entry, size) = position.take().unwrap();
                    let pnl = (bar.close - entry) * size;
                    let fee = bar.close * size * self.fee_rate;
                    cash += pnl - fee;
                    trades.push(Trade {
                        entry_price: entry,
                        exit_price: bar.close,
                        size,
                        pnl: pnl - fee,
                    });
                }
                _ => {}
            }

            // force-close on last bar so metrics include unrealized PnL
            if i == candles.len() - 1 {
                if let Some((entry, size)) = position.take() {
                    let pnl = (bar.close - entry) * size;
                    let fee = bar.close * size * self.fee_rate;
                    cash += pnl - fee;
                    trades.push(Trade {
                        entry_price: entry,
                        exit_price: bar.close,
                        size,
                        pnl: pnl - fee,
                    });
                }
            }

            equity = cash + position
                .map(|(e, s)| (bar.close - e) * s)
                .unwrap_or(Decimal::ZERO);
            equity_curve.push(equity);

            if equity > peak {
                peak = equity;
            }
            if !peak.is_zero() {
                let dd = (peak - equity) / peak * dec!(100);
                if dd > max_dd {
                    max_dd = dd;
                }
            }
        }

        BacktestResult {
            starting_equity: self.starting_equity,
            final_equity: equity,
            peak_equity: peak,
            max_drawdown_pct: max_dd,
            trades,
            equity_curve,
        }
    }
}

/// Deterministic price series for demos and tests — no external files needed.
/// Every 50 bars the trend flips so the SMA strategy gets multiple crossovers.
pub fn synthetic_candles(n: usize) -> Vec<Candle> {
    let mut candles = Vec::with_capacity(n);
    let mut price = dec!(100);
    for i in 0..n {
        let drift = if (i / 50) % 2 == 0 { dec!(0.5) } else { dec!(-0.3) };
        let open = price;
        let close = open + drift;
        let high = open.max(close) + dec!(0.2);
        let low = open.min(close) - dec!(0.2);
        candles.push(Candle::new(i as i64, open, high, low, close, dec!(1000)));
        price = close;
    }
    candles
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_produces_n_candles() {
        assert_eq!(synthetic_candles(100).len(), 100);
    }

    #[test]
    fn strategy_returns_hold_before_warmup() {
        let mut s = SmaCross::new(3, 5);
        for _ in 0..5 {
            let c = Candle::new(0, dec!(100), dec!(101), dec!(99), dec!(100), dec!(1));
            assert_eq!(s.on_bar(&c), Signal::Hold);
        }
    }

    #[test]
    fn strategy_emits_buy_on_uptrend_cross() {
        let mut s = SmaCross::new(2, 4);
        let prices = vec![
            dec!(10), dec!(10), dec!(10), dec!(9), dec!(8), dec!(7), dec!(15),
        ];
        let mut signals = Vec::new();
        for p in prices {
            let c = Candle::new(0, p, p, p, p, dec!(1));
            signals.push(s.on_bar(&c));
        }
        assert!(signals.contains(&Signal::Buy), "expected a buy, got {:?}", signals);
    }

    #[test]
    fn flat_market_produces_no_trades() {
        let candles: Vec<Candle> = (0..100)
            .map(|_| Candle::new(0, dec!(100), dec!(100), dec!(100), dec!(100), dec!(1)))
            .collect();
        let r = Backtest::new(dec!(1000), dec!(1)).run(&candles, SmaCross::new(3, 10));
        assert_eq!(r.trades.len(), 0);
        assert_eq!(r.final_equity, dec!(1000));
    }

    #[test]
    fn backtest_tracks_peak_drawdown() {
        struct BuyOnce { bought: bool }
        impl Strategy for BuyOnce {
            fn on_bar(&mut self, _: &Candle) -> Signal {
                if !self.bought { self.bought = true; Signal::Buy } else { Signal::Hold }
            }
        }
        let candles = vec![
            Candle::new(0, dec!(100), dec!(100), dec!(100), dec!(100), dec!(1)),
            Candle::new(1, dec!(110), dec!(110), dec!(110), dec!(110), dec!(1)),
            Candle::new(2, dec!(90),  dec!(90),  dec!(90),  dec!(90),  dec!(1)),
        ];
        let r = Backtest::new(dec!(1000), dec!(1)).run(&candles, BuyOnce { bought: false });
        // Entry at 100, size = 10. Peak equity 1100 at bar 1. At bar 2 equity 900.
        // Drawdown = (1100 - 900) / 1100 = 18.18%
        assert!(r.max_drawdown_pct > dec!(18));
        assert!(r.max_drawdown_pct < dec!(19));
    }

    #[test]
    fn profit_factor_sentinel_when_no_losses() {
        struct BuyOnce { bought: bool }
        impl Strategy for BuyOnce {
            fn on_bar(&mut self, _: &Candle) -> Signal {
                if !self.bought { self.bought = true; Signal::Buy } else { Signal::Hold }
            }
        }
        let candles = vec![
            Candle::new(0, dec!(100), dec!(100), dec!(100), dec!(100), dec!(1)),
            Candle::new(1, dec!(120), dec!(120), dec!(120), dec!(120), dec!(1)),
        ];
        let r = Backtest::new(dec!(1000), dec!(1)).run(&candles, BuyOnce { bought: false });
        assert_eq!(r.trades.len(), 1);
        assert_eq!(r.profit_factor(), dec!(999));
    }

    #[test]
    fn sharpe_is_zero_on_flat_equity() {
        struct NoOp;
        impl Strategy for NoOp {
            fn on_bar(&mut self, _: &Candle) -> Signal { Signal::Hold }
        }
        let candles: Vec<Candle> = (0..50)
            .map(|_| Candle::new(0, dec!(100), dec!(100), dec!(100), dec!(100), dec!(1)))
            .collect();
        let r = Backtest::new(dec!(1000), dec!(1)).run(&candles, NoOp);
        assert_eq!(r.sharpe(), dec!(0));
    }
}
