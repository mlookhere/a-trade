use crate::{
    EmaStackState, MarketSnapshot, OhlcvObservationSeries, OhlcvTimeframe, RequiredOhlcvCoverage,
    RuleProvenance, TemporalStatus, classify_ema_stack,
};

/// AI-native §34 requires explicit, version-controlled EMA windows. The source
/// supplies no recursive EMA calculation, warmup policy or timeframe default.
/// The choice here is a RESEARCH mathematical formalization, not a canonical
/// auction rule, live direction gate, or production feature threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchEmaSeed {
    FirstCompletedClose,
    InitialSimpleAverage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResearchEmaConfig {
    pub timeframe: OhlcvTimeframe,
    pub fast_window: usize,
    pub middle_window: usize,
    pub slow_window: usize,
    pub seed: ResearchEmaSeed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResearchEmaError {
    InvalidWindows,
    UnsupportedTimeframe,
    SnapshotMismatch,
    SymbolMismatch,
    SourceMismatch,
    MissingOrStaleCoverage,
    CoverageBarMismatch,
    InsufficientCompletedHistory,
    NonFiniteCalculation,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResearchEmaObservation {
    pub snapshot_id: String,
    pub symbol: String,
    pub source_id: String,
    pub timeframe: OhlcvTimeframe,
    pub fast_window: usize,
    pub middle_window: usize,
    pub slow_window: usize,
    pub seed: ResearchEmaSeed,
    pub fast_value: f64,
    pub middle_value: f64,
    pub slow_value: f64,
    pub baseline_stack: EmaStackState,
    pub provenance: RuleProvenance,
}

/// Explicit seed choice + fixed recurrence:
/// alpha = 2/(window+1);
/// EMA[k] = EMA[k-1] + alpha * (close[k]-EMA[k-1]).
/// No source document specifies this exact arithmetic or initial seed.
/// Sufficient historical bars are required; missing bars are never zero-filled.
fn calculate_ema(
    bars: &[crate::ObservedOhlcvBar],
    window: usize,
    seed: ResearchEmaSeed,
) -> Result<f64, ResearchEmaError> {
    if bars.len() < window || window == 0 {
        return Err(ResearchEmaError::InsufficientCompletedHistory);
    }

    let (mut ema, offset) = match seed {
        ResearchEmaSeed::FirstCompletedClose => (bars[0].close(), 1),
        ResearchEmaSeed::InitialSimpleAverage => {
            let divisor = window as f64;
            let anchor = bars[0].close();
            let mut centered_average = 0.0_f64;
            for bar in bars.iter().take(window) {
                centered_average += (bar.close() - anchor) / divisor;
            }
            (anchor + centered_average, window)
        }
    };
    let alpha = 2.0 / (window as f64 + 1.0);
    for bar in bars.iter().skip(offset) {
        ema += alpha * (bar.close() - ema);
        if !ema.is_finite() {
            return Err(ResearchEmaError::NonFiniteCalculation);
        }
    }
    if !ema.is_finite() {
        return Err(ResearchEmaError::NonFiniteCalculation);
    }
    Ok(ema)
}

/// Research-only observation of three EMAs from one verified same-snapshot,
/// source-matched and within-configured-age OHLCV scope. This does not prove
/// gap-free provider history, source authenticity, a trend signal, complete
/// market DATA_VALID, or hard-veto clearance.
pub fn calculate_research_ema_stack(
    snapshot: &MarketSnapshot,
    series: &OhlcvObservationSeries,
    coverage: &RequiredOhlcvCoverage,
    config: &ResearchEmaConfig,
) -> Result<ResearchEmaObservation, ResearchEmaError> {
    if config.fast_window == 0
        || config.fast_window >= config.middle_window
        || config.middle_window >= config.slow_window
    {
        return Err(ResearchEmaError::InvalidWindows);
    }
    if matches!(config.timeframe, OhlcvTimeframe::Daily) {
        // RequiredOhlcvCoverage does not validate Daily source freshness.
        return Err(ResearchEmaError::UnsupportedTimeframe);
    }
    if series.snapshot_id() != snapshot.snapshot_id()
        || coverage.snapshot_id() != snapshot.snapshot_id()
    {
        return Err(ResearchEmaError::SnapshotMismatch);
    }
    if series.symbol() != snapshot.symbol() || coverage.symbol() != snapshot.symbol() {
        return Err(ResearchEmaError::SymbolMismatch);
    }
    if coverage.source_id() != Some(series.source_id())
        || coverage.source_quality() != Some(series.quality())
    {
        return Err(ResearchEmaError::SourceMismatch);
    }

    let bars = series.completed_bars(config.timeframe);
    let Some(last) = bars.last() else {
        return Err(ResearchEmaError::InsufficientCompletedHistory);
    };
    let Some(observation) = coverage
        .observations()
        .iter()
        .find(|observation| observation.timeframe == config.timeframe)
    else {
        return Err(ResearchEmaError::MissingOrStaleCoverage);
    };
    if observation.status != TemporalStatus::WithinConfiguredAge {
        return Err(ResearchEmaError::MissingOrStaleCoverage);
    }
    if observation.source_timestamp_millis != Some(last.source_timestamp_millis()) {
        return Err(ResearchEmaError::CoverageBarMismatch);
    }
    if bars.len() < config.slow_window {
        return Err(ResearchEmaError::InsufficientCompletedHistory);
    }

    let fast = calculate_ema(bars, config.fast_window, config.seed)?;
    let middle = calculate_ema(bars, config.middle_window, config.seed)?;
    let slow = calculate_ema(bars, config.slow_window, config.seed)?;
    let stack = classify_ema_stack(Some(fast), Some(middle), Some(slow))
        .map_err(|_| ResearchEmaError::NonFiniteCalculation)?;

    Ok(ResearchEmaObservation {
        snapshot_id: snapshot.snapshot_id().to_owned(),
        symbol: snapshot.symbol().to_owned(),
        source_id: series.source_id().to_owned(),
        timeframe: config.timeframe,
        fast_window: config.fast_window,
        middle_window: config.middle_window,
        slow_window: config.slow_window,
        seed: config.seed,
        fast_value: fast,
        middle_value: middle,
        slow_value: slow,
        baseline_stack: stack,
        provenance: RuleProvenance::ValidationRequiredAssumption,
    })
}
