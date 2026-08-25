use std::{collections::VecDeque, sync::Arc, time::Instant};

use tokio::{sync::Mutex, time::sleep};

const WINDOW_MS: u64 = 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetDecision {
    Granted,
    RetryAfterMs(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BudgetError {
    InvalidLimit,
    TimeReversed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestBudget {
    effective_limit: usize,
    calls: VecDeque<u64>,
    last_now_ms: Option<u64>,
}

impl RestBudget {
    pub fn new(provider_limit: u32, reserved_headroom: u32) -> Result<Self, BudgetError> {
        if provider_limit == 0 || reserved_headroom >= provider_limit {
            return Err(BudgetError::InvalidLimit);
        }
        Ok(Self {
            effective_limit: (provider_limit - reserved_headroom) as usize,
            calls: VecDeque::new(),
            last_now_ms: None,
        })
    }

    #[must_use]
    pub const fn effective_limit(&self) -> usize {
        self.effective_limit
    }

    pub fn reserve(&mut self, now_ms: u64) -> Result<BudgetDecision, BudgetError> {
        if self.last_now_ms.is_some_and(|previous| now_ms < previous) {
            return Err(BudgetError::TimeReversed);
        }
        self.last_now_ms = Some(now_ms);
        while self
            .calls
            .front()
            .is_some_and(|oldest| now_ms.saturating_sub(*oldest) >= WINDOW_MS)
        {
            self.calls.pop_front();
        }

        if self.calls.len() < self.effective_limit {
            self.calls.push_back(now_ms);
            return Ok(BudgetDecision::Granted);
        }

        let oldest = *self.calls.front().expect("non-empty when budget exhausted");
        Ok(BudgetDecision::RetryAfterMs(
            WINDOW_MS.saturating_sub(now_ms.saturating_sub(oldest)),
        ))
    }
}

#[derive(Debug, Clone)]
pub struct RestRateLimiter {
    started: Instant,
    effective_limit: usize,
    budget: Arc<Mutex<RestBudget>>,
}

impl RestRateLimiter {
    pub fn new(provider_limit: u32, reserved_headroom: u32) -> Result<Self, BudgetError> {
        let budget = RestBudget::new(provider_limit, reserved_headroom)?;
        let effective_limit = budget.effective_limit();
        Ok(Self {
            started: Instant::now(),
            effective_limit,
            budget: Arc::new(Mutex::new(budget)),
        })
    }

    #[must_use]
    pub const fn effective_limit(&self) -> usize {
        self.effective_limit
    }

    pub async fn acquire(&self) -> Result<(), BudgetError> {
        loop {
            let now_ms = self.started.elapsed().as_millis() as u64;
            let decision = self.budget.lock().await.reserve(now_ms)?;
            match decision {
                BudgetDecision::Granted => return Ok(()),
                BudgetDecision::RetryAfterMs(delay) => {
                    sleep(std::time::Duration::from_millis(delay)).await;
                }
            }
        }
    }
}
