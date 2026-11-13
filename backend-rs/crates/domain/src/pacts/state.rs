//! A pact as stored, with the rules that follow from its state alone.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde::Serialize;

use super::kind::{KindRules, PactKind};
use super::outcome::Outcome;
use super::settle::Party;
use super::terms::{PactConfig, Terms};

text_enum!(
    /// Where a pact is in its life.
    Status {
        /// Waiting for the first invitee (or joiner) to come in.
        Proposed = "proposed",
        Active = "active",
        /// The deadline passed and nobody has settled it yet.
        AwaitingResult = "awaiting_result",
        Resolved = "resolved",
        /// Called off by its creator before it started, or it never started by its deadline.
        Cancelled = "cancelled",
        /// Every claim that ended it called it off.
        Void = "void",
    }
);

impl Status {
    /// Still taking people in.
    pub fn running(self) -> bool {
        matches!(self, Self::Proposed | Self::Active)
    }

    /// Can be settled (claims, judgments).
    pub fn deciding(self) -> bool {
        matches!(self, Self::Active | Self::AwaitingResult)
    }

    pub fn finished(self) -> bool {
        matches!(self, Self::Resolved | Self::Cancelled | Self::Void)
    }
}

text_enum!(
    Role {
        Host = "host",
        Opponent = "opponent",
    }
);

text_enum!(
    /// One person's place in a pact.
    ParticipantState {
        Invited = "invited",
        /// Asked to join, waiting for the approvers.
        Requested = "requested",
        Active = "active",
        /// Their wager (or the pact) was settled.
        Settled = "settled",
        /// Their wager (or the pact) was called off.
        Void = "void",
        Declined = "declined",
        /// An approver turned the join request down.
        Rejected = "rejected",
        Withdrawn = "withdrawn",
        /// An invite or join request nobody answered in time.
        Expired = "expired",
    }
);

impl ParticipantState {
    /// Out of it, but may ask to join again.
    pub fn may_retry(self) -> bool {
        matches!(
            self,
            Self::Declined | Self::Rejected | Self::Withdrawn | Self::Expired
        )
    }

    /// A wager that is not over yet (keeps a bet from resolving).
    pub fn blocking(self) -> bool {
        matches!(self, Self::Invited | Self::Active)
    }

    /// Waiting for an answer (an invite or a join request).
    pub fn waiting(self) -> bool {
        matches!(self, Self::Invited | Self::Requested)
    }

    /// Takes part (or took part) closely enough to add pictures.
    pub fn may_attach(self) -> bool {
        matches!(
            self,
            Self::Invited | Self::Requested | Self::Active | Self::Settled | Self::Void
        )
    }
}

text_enum!(
    /// Where an outcome claim is.
    ClaimState {
        /// Waiting for the other side to confirm.
        Pending = "pending",
        Confirmed = "confirmed",
        Disputed = "disputed",
        /// Replaced by a newer claim.
        Superseded = "superseded",
        /// A Sejmik ruling rejected the disputed claim.
        Overruled = "overruled",
    }
);

impl ClaimState {
    /// Pending or disputed: at most one per pact (bet: per wager).
    pub fn live(self) -> bool {
        matches!(self, Self::Pending | Self::Disputed)
    }
}

/// One approver's say on a join request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Consent {
    pub approver_id: i64,
    /// `None` until they decide.
    pub approved: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Participant {
    pub id: i64,
    pub user_id: i64,
    pub role: Role,
    pub state: ParticipantState,
    pub terms: Terms,
    /// Join requests only.
    pub consents: Vec<Consent>,
    /// When they were invited or (last) asked to join.
    pub created_at: DateTime<Utc>,
    /// When an unanswered invite or request lapses.
    pub expires_at: Option<DateTime<Utc>>,
}

impl Participant {
    pub fn party(&self) -> Party {
        Party {
            user_id: self.user_id,
            is_host: self.role == Role::Host,
            side: self.terms.side.clone(),
            stake_amount: self.terms.stake_amount,
            stake_note: self.terms.stake_note.clone(),
        }
    }
}

/// The Sejmik vote a disputed claim went to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ruling {
    pub poll_id: i64,
    pub open: bool,
}

/// Someone's claim of how the pact (bet: one wager) ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    pub id: i64,
    /// The wager it settles (`bet`); `None`: the whole pact.
    pub wager_id: Option<i64>,
    pub proposed_by: i64,
    pub result: Outcome,
    pub state: ClaimState,
    /// Who agreed so far.
    pub confirmed_by: Vec<i64>,
    pub ruling: Option<Ruling>,
    pub created_at: DateTime<Utc>,
    /// When it was confirmed, disputed or overruled.
    pub decided_at: Option<DateTime<Utc>>,
}

/// The whole pact, as the rules need it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pact {
    pub id: i64,
    pub kind: PactKind,
    pub status: Status,
    pub creator_id: i64,
    pub title: String,
    pub due_at: Option<DateTime<Utc>>,
    pub config: PactConfig,
    /// The `resolution_judgment` vote running now.
    pub judgment_poll_id: Option<i64>,
    /// In the order they joined (the host first).
    pub participants: Vec<Participant>,
    /// Oldest first.
    pub claims: Vec<Claim>,
    pub attachments: usize,
}

/// How a finished pact ended, derived from its confirmed claims.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
#[serde(untagged)]
pub enum PactOutcome {
    /// `bet`: each settled wager, in the order they were settled.
    Wagers { wagers: Vec<WagerOutcome> },
    /// Every other kind: the claim that ended it.
    Whole { result: Outcome },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
pub struct WagerOutcome {
    pub participant_id: i64,
    pub result: Outcome,
}

impl Pact {
    pub fn rules(&self) -> KindRules {
        self.kind.rules()
    }

    /// The caller's row.
    pub fn row_of(&self, user_id: i64) -> Option<&Participant> {
        self.participants.iter().find(|p| p.user_id == user_id)
    }

    pub fn participant(&self, id: i64) -> Option<&Participant> {
        self.participants.iter().find(|p| p.id == id)
    }

    pub fn opponents(&self) -> impl Iterator<Item = &Participant> {
        self.participants
            .iter()
            .filter(|p| p.role == Role::Opponent)
    }

    pub fn active_ids(&self) -> Vec<i64> {
        self.participants
            .iter()
            .filter(|p| p.state == ParticipantState::Active)
            .map(|p| p.user_id)
            .collect()
    }

    pub fn is_active(&self, user_id: i64) -> bool {
        self.row_of(user_id)
            .is_some_and(|p| p.state == ParticipantState::Active)
    }

    /// With a deadline, nobody can call a result before it has passed (only call it off).
    pub fn result_allowed(&self, now: DateTime<Utc>) -> bool {
        self.due_at.is_none_or(|due| due <= now)
    }

    pub fn due_passed(&self, now: DateTime<Utc>) -> bool {
        self.due_at.is_some_and(|due| due <= now)
    }

    pub fn claim(&self, id: i64) -> Option<&Claim> {
        self.claims.iter().find(|c| c.id == id)
    }

    /// A disputed claim about `wager` (or the whole pact) is being voted on in the Sejmik.
    pub fn ruling_open(&self, wager: Option<i64>) -> bool {
        self.claims.iter().any(|c| {
            c.wager_id == wager
                && c.state == ClaimState::Disputed
                && c.ruling.is_some_and(|r| r.open)
        })
    }

    /// Whole-pact kinds cannot change who is in them while a claim is being decided (its
    /// confirmers and payouts would shift under it).
    pub fn claim_in_flight(&self) -> bool {
        !self.rules().wager_based
            && self.claims.iter().any(|c| {
                c.state == ClaimState::Pending
                    || (c.state == ClaimState::Disputed && c.ruling.is_some_and(|r| r.open))
            })
    }

    /// Whose agreement `claim` needs: the other side of a wager, or everyone else active.
    pub fn confirmers(&self, claim: &Claim) -> BTreeSet<i64> {
        let mut parties = self.parties(claim);
        parties.remove(&claim.proposed_by);
        parties
    }

    /// Everyone on either side of `claim`.
    pub fn parties(&self, claim: &Claim) -> BTreeSet<i64> {
        let mut parties: BTreeSet<i64> = match claim.wager_id {
            Some(wager) => self
                .participant(wager)
                .map(|w| BTreeSet::from([self.creator_id, w.user_id]))
                .unwrap_or_default(),
            None => self.active_ids().into_iter().collect(),
        };
        parties.insert(claim.proposed_by);
        parties
    }

    /// A proposed pact starts once someone is in, unless the kind waits for every invitee.
    pub fn ready_to_start(&self) -> bool {
        self.status == Status::Proposed
            && self
                .opponents()
                .any(|p| p.state == ParticipantState::Active)
            && !(self.rules().all_must_answer
                && self
                    .opponents()
                    .any(|p| p.state == ParticipantState::Invited))
    }

    /// No wager of a bet is still running or waiting for an answer (always true for the
    /// other kinds, which settle as a whole).
    pub fn wagers_done(&self) -> bool {
        !self.rules().wager_based || !self.opponents().any(|p| p.state.blocking())
    }

    pub fn confirmed_claims(&self) -> impl Iterator<Item = &Claim> {
        self.claims
            .iter()
            .filter(|c| c.state == ClaimState::Confirmed)
    }

    /// What a settled pact ends as: void if what ended it called it off (a bet: every wager).
    pub fn final_status(&self) -> Status {
        let void = if self.rules().wager_based {
            self.confirmed_claims().all(|c| c.result.is_void())
        } else {
            self.confirmed_claims()
                .last()
                .is_some_and(|c| c.result.is_void())
        };
        if void { Status::Void } else { Status::Resolved }
    }

    /// The parties a confirmed `claim` settles (bet: the host and that wager's opponent;
    /// otherwise everyone active) and the participant rows it ends.
    pub fn settled_by(&self, claim: &Claim) -> (Vec<Party>, Vec<i64>) {
        if let Some(wager) = claim.wager_id {
            let parties = self
                .participants
                .iter()
                .filter(|p| p.id == wager || p.role == Role::Host)
                .map(Participant::party)
                .collect();
            return (parties, vec![wager]);
        }
        let active: Vec<&Participant> = self
            .participants
            .iter()
            .filter(|p| p.state == ParticipantState::Active)
            .collect();
        (
            active.iter().map(|p| p.party()).collect(),
            active.iter().map(|p| p.id).collect(),
        )
    }

    pub fn outcome(&self) -> Option<PactOutcome> {
        if !matches!(self.status, Status::Resolved | Status::Void) {
            return None;
        }
        if self.rules().wager_based {
            return Some(PactOutcome::Wagers {
                wagers: self
                    .confirmed_claims()
                    .filter_map(|c| {
                        c.wager_id.map(|participant_id| WagerOutcome {
                            participant_id,
                            result: c.result.clone(),
                        })
                    })
                    .collect(),
            });
        }
        self.confirmed_claims().last().map(|c| PactOutcome::Whole {
            result: c.result.clone(),
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use chrono::TimeDelta;

    use super::*;

    pub fn row(id: i64, user_id: i64, role: Role, state: ParticipantState) -> Participant {
        Participant {
            id,
            user_id,
            role,
            state,
            terms: Terms::default(),
            consents: Vec::new(),
            created_at: DateTime::UNIX_EPOCH,
            expires_at: None,
        }
    }

    /// Alice (1) hosts; bob (2) and carol (3) are invited.
    pub fn pact(kind: PactKind) -> Pact {
        Pact {
            id: 1,
            kind,
            status: Status::Proposed,
            creator_id: 1,
            title: "t".into(),
            due_at: None,
            config: PactConfig::default(),
            judgment_poll_id: None,
            participants: vec![
                row(10, 1, Role::Host, ParticipantState::Active),
                row(20, 2, Role::Opponent, ParticipantState::Invited),
                row(30, 3, Role::Opponent, ParticipantState::Invited),
            ],
            claims: Vec::new(),
            attachments: 0,
        }
    }

    pub fn claim(id: i64, wager_id: Option<i64>, by: i64, state: ClaimState) -> Claim {
        Claim {
            id,
            wager_id,
            proposed_by: by,
            result: Outcome::Winner {
                winner: "host".into(),
            },
            state,
            confirmed_by: Vec::new(),
            ruling: None,
            created_at: DateTime::UNIX_EPOCH,
            decided_at: None,
        }
    }

    #[test]
    fn a_group_bet_starts_only_when_nobody_is_left_to_answer() {
        let mut bet = pact(PactKind::Bet);
        bet.participants[1].state = ParticipantState::Active;
        assert!(bet.ready_to_start());
        let mut group = pact(PactKind::GroupBet);
        group.participants[1].state = ParticipantState::Active;
        assert!(!group.ready_to_start());
        group.participants[2].state = ParticipantState::Declined;
        assert!(group.ready_to_start());
    }

    #[test]
    fn a_wager_claim_needs_the_other_side_and_a_whole_claim_everyone_else() {
        let mut bet = pact(PactKind::Bet);
        bet.participants[1].state = ParticipantState::Active;
        let wager = claim(1, Some(20), 2, ClaimState::Pending);
        assert_eq!(bet.confirmers(&wager), BTreeSet::from([1]));
        let mut group = pact(PactKind::GroupBet);
        group.participants[1].state = ParticipantState::Active;
        group.participants[2].state = ParticipantState::Active;
        let whole = claim(1, None, 2, ClaimState::Pending);
        assert_eq!(group.confirmers(&whole), BTreeSet::from([1, 3]));
        assert_eq!(group.parties(&whole), BTreeSet::from([1, 2, 3]));
    }

    #[test]
    fn only_whole_pact_kinds_freeze_while_a_claim_is_decided() {
        let mut group = pact(PactKind::GroupBet);
        assert!(!group.claim_in_flight());
        group.claims.push(claim(1, None, 1, ClaimState::Disputed));
        assert!(!group.claim_in_flight(), "parked, nobody is voting on it");
        group.claims[0].ruling = Some(Ruling {
            poll_id: 9,
            open: true,
        });
        assert!(group.claim_in_flight() && group.ruling_open(None));
        let mut bet = pact(PactKind::Bet);
        bet.claims.push(claim(1, Some(20), 1, ClaimState::Pending));
        assert!(!bet.claim_in_flight());
    }

    #[test]
    fn a_bet_is_done_once_no_wager_runs_or_waits() {
        let mut bet = pact(PactKind::Bet);
        assert!(!bet.wagers_done());
        bet.participants[1].state = ParticipantState::Settled;
        bet.participants[2].state = ParticipantState::Requested;
        assert!(!bet.participants[2].state.blocking());
        assert!(bet.wagers_done());
    }

    #[test]
    fn a_pact_is_void_when_what_ended_it_called_it_off() {
        let mut bet = pact(PactKind::Bet);
        bet.status = Status::Resolved;
        let mut off = claim(1, Some(20), 1, ClaimState::Confirmed);
        off.result = Outcome::VOID;
        bet.claims.push(off.clone());
        assert_eq!(bet.final_status(), Status::Void);
        bet.claims
            .push(claim(2, Some(30), 1, ClaimState::Confirmed));
        assert_eq!(bet.final_status(), Status::Resolved);
        assert_eq!(
            bet.outcome(),
            Some(PactOutcome::Wagers {
                wagers: vec![
                    WagerOutcome {
                        participant_id: 20,
                        result: Outcome::VOID
                    },
                    WagerOutcome {
                        participant_id: 30,
                        result: Outcome::Winner {
                            winner: "host".into()
                        }
                    },
                ]
            })
        );
    }

    #[test]
    fn results_wait_for_the_deadline() {
        let now = DateTime::from_timestamp(1_800_000_000, 0).unwrap();
        let mut goal = pact(PactKind::SharedGoal);
        assert!(goal.result_allowed(now));
        goal.due_at = Some(now + TimeDelta::days(1));
        assert!(!goal.result_allowed(now) && !goal.due_passed(now));
        assert!(goal.result_allowed(now + TimeDelta::days(1)));
    }
}
