mod common;

use auction_core::{
    BrokerAdapter, BrokerCapabilities, BrokerReconciliation, BrokerSubmission, Condition,
    Direction, EtTime, ExecutionCoordinator, ExecutionGateContext, LocationSetup, MarketState,
    OrderProposal, RejectionCode, StructureKey, SwingImpulse,
};

fn long_impulse(start_index: usize, end_index: usize) -> SwingImpulse {
    SwingImpulse {
        direction: Direction::Long,
        start_index,
        end_index,
        swing_low: 80.0,
        swing_high: 100.0,
    }
}

#[test]
fn sections_37_117_new_structure_can_start_during_entry_window_but_never_reuse_known_structure() {
    let known = StructureKey::from(long_impulse(2, 7));
    let candidate = long_impulse(8, 13);

    let setup = LocationSetup::from_new_structure(
        EtTime::from_hms(10, 30, 0).unwrap(),
        MarketState::ValueUp,
        candidate,
        90.0,
        0.25,
        &[known],
    )
    .unwrap();
    assert_eq!(setup.structure(), StructureKey::from(candidate));

    assert!(
        LocationSetup::from_new_structure(
            EtTime::from_hms(10, 30, 0).unwrap(),
            MarketState::ValueUp,
            candidate,
            90.0,
            0.25,
            &[known, StructureKey::from(candidate)],
        )
        .is_none()
    );
}

#[test]
fn sections_3_37_117_new_structure_creation_obeys_exact_entry_cutoff_and_direction() {
    let candidate = long_impulse(8, 13);
    assert!(
        LocationSetup::from_new_structure(
            EtTime::from_hms(10, 59, 59).unwrap(),
            MarketState::ValueUp,
            candidate,
            90.0,
            0.25,
            &[],
        )
        .is_some()
    );
    assert!(
        LocationSetup::from_new_structure(
            EtTime::from_hms(11, 0, 0).unwrap(),
            MarketState::ValueUp,
            candidate,
            90.0,
            0.25,
            &[],
        )
        .is_none()
    );
    assert!(
        LocationSetup::from_new_structure(
            EtTime::from_hms(9, 29, 59).unwrap(),
            MarketState::ValueUp,
            candidate,
            90.0,
            0.25,
            &[],
        )
        .is_none()
    );
    assert!(
        LocationSetup::from_new_structure(
            EtTime::from_hms(10, 30, 0).unwrap(),
            MarketState::ValueDown,
            candidate,
            90.0,
            0.25,
            &[],
        )
        .is_none()
    );
}

#[derive(Debug, Default)]
struct GateBroker {
    reconciliation: Option<BrokerReconciliation>,
}

impl BrokerAdapter for GateBroker {
    type Error = ();

    fn capabilities(&self) -> BrokerCapabilities {
        BrokerCapabilities {
            protected_stop_limit_bracket: Condition::True,
        }
    }

    fn reconcile(&mut self) -> Result<BrokerReconciliation, Self::Error> {
        Ok(self
            .reconciliation
            .clone()
            .unwrap_or_else(|| BrokerReconciliation {
                broker_connected: Condition::True,
                broker_state_known: Condition::True,
                position_state_known: Condition::True,
                open_order_state_known: Condition::True,
                no_conflicting_order: Condition::True,
                open_order_setup_ids: vec![],
                position_setup_ids: vec![],
            }))
    }

    fn submit_protected(
        &mut self,
        _proposal: &OrderProposal,
    ) -> Result<BrokerSubmission, Self::Error> {
        unreachable!("gate-only hardening test must never submit")
    }

    fn entry_filled(&mut self, _setup_id: &str) -> Result<Condition, Self::Error> {
        unreachable!("gate-only hardening test must never reconcile fill")
    }

    fn hard_stop_confirmed(&mut self, _setup_id: &str) -> Result<Condition, Self::Error> {
        unreachable!("gate-only hardening test must never inspect protection")
    }

    fn flatten_instrument(&mut self, _instrument: &str) -> Result<(), Self::Error> {
        unreachable!("gate-only hardening test must never flatten")
    }
}

fn ready_coordinator(setup_id: &str) -> ExecutionCoordinator<GateBroker> {
    let mut coordinator = ExecutionCoordinator::new(GateBroker::default());
    coordinator
        .operational_safety_mut()
        .set_operational_risk_clear(Condition::True);
    coordinator
        .operational_safety_mut()
        .observe_emergency_drawdown(Condition::False);
    coordinator.register_setup(setup_id, common::OWNER).unwrap();
    coordinator
}

fn context(llm_setup_pass: Condition) -> ExecutionGateContext<'static> {
    ExecutionGateContext {
        submitting_agent_id: common::OWNER,
        llm_setup_pass,
    }
}

#[test]
fn sections_109_122_llm_false_or_unknown_cannot_cross_sealed_execution_gate() {
    for llm_setup_pass in [Condition::False, Condition::Unknown] {
        let setup_id = "SETUP-HARDEN-LLM";
        let proposal = common::valid_long_proposal(setup_id, common::OWNER, common::INSTRUMENT);
        let mut coordinator = ready_coordinator(setup_id);
        assert_eq!(
            coordinator.gate(&proposal, context(llm_setup_pass)),
            Err(RejectionCode::ProcessError)
        );
    }
}

#[test]
fn sections_9_103_109_broker_reconciled_same_setup_cannot_be_bypassed() {
    let setup_id = "SETUP-HARDEN-DUP";
    let proposal = common::valid_long_proposal(setup_id, common::OWNER, common::INSTRUMENT);
    let mut coordinator = ready_coordinator(setup_id);
    coordinator.adapter_mut().reconciliation = Some(BrokerReconciliation {
        broker_connected: Condition::True,
        broker_state_known: Condition::True,
        position_state_known: Condition::True,
        open_order_state_known: Condition::True,
        no_conflicting_order: Condition::True,
        open_order_setup_ids: vec![setup_id.to_owned()],
        position_setup_ids: vec![],
    });
    assert_eq!(
        coordinator.gate(&proposal, context(Condition::True)),
        Err(RejectionCode::DuplicateSetup)
    );
}
