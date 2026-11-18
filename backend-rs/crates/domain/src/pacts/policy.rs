//! Who may do what with a pact, and when. The services enforce [`check`]; the API's `actions`
//! block is [`actions`], computed by the same function, so the two cannot disagree.

use chrono::{DateTime, Utc};
use serde::Serialize;

use super::MAX_ATTACHMENTS;
use super::outcome::Outcome;
use super::state::{ClaimState, Pact, ParticipantState, Role};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Accept or decline an invite.
    Respond,
    /// Call off a pact that has not started (its creator).
    Cancel,
    /// Ask to join.
    Join,
    /// Take back one's join request.
    Withdraw,
    /// Approve or reject participant `id`'s join request.
    Decide(i64),
    /// Claim how it ended: a wager (participant id) of a bet, or the whole pact (`None`).
    Claim(Option<i64>),
    Confirm(i64),
    Dispute(i64),
    /// Put disputed claim `id` to a Sejmik vote.
    Escalate(i64),
    /// Put a resolution to a vote of every other member.
    CallJudgment,
    Attach,
    /// Take down a picture uploaded by this user.
    RemoveAttachment {
        uploaded_by: i64,
    },
}

/// Why an action is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Denial {
    /// The claim (or picture) is not part of this pact (404).
    NotFound,
    /// Not now: the pact's (or a claim's) state does not allow it (409).
    Conflict {
        code: &'static str,
        message: &'static str,
    },
    /// Not this person (403).
    Forbidden {
        code: &'static str,
        message: &'static str,
    },
    /// The request is wrong (400).
    Invalid {
        code: &'static str,
        message: &'static str,
    },
}

const fn conflict(code: &'static str, message: &'static str) -> Denial {
    Denial::Conflict { code, message }
}

const fn forbidden(code: &'static str, message: &'static str) -> Denial {
    Denial::Forbidden { code, message }
}

const NOT_CURRENT: Denial = conflict("pact_closed", "Ta umowa nie jest już aktualna.");
const NOT_IN_IT: Denial = forbidden("not_participant", "Nie bierzesz udziału w tej umowie.");
const IN_FLIGHT: Denial = conflict(
    "claim_in_flight",
    "Trwa rozstrzyganie wyniku, poczekaj na jego zakończenie.",
);

/// What the caller can do right now.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schema", schema(as = PactActions))]
#[allow(clippy::struct_excessive_bools)] // one flag per action
pub struct Actions {
    /// Answer your invite.
    pub respond: bool,
    pub cancel: bool,
    /// Ask to join.
    pub join: bool,
    /// Take back your join request.
    pub withdraw: bool,
    /// Call the Sejmik vote that judges a resolution.
    pub judgment: bool,
    /// Claim a result now (a bet: about one of `wagers`).
    pub claim: bool,
    /// Propose calling it off (possible before the deadline too).
    pub void: bool,
    pub attach: bool,
    /// Bets: the wagers (participant ids) you may make a claim about.
    pub wagers: Vec<i64>,
    /// Join requests (participant ids) waiting for your decision.
    pub decide: Vec<i64>,
    /// Claims waiting for you to confirm or dispute.
    pub confirm: Vec<i64>,
    /// Disputed claims you may put to a Sejmik vote.
    pub escalate: Vec<i64>,
}

pub fn actions(pact: &Pact, user_id: i64, now: DateTime<Utc>) -> Actions {
    let can = |action| check(action, pact, user_id, now).is_ok();
    let wagers: Vec<i64> = if pact.rules().wager_based {
        pact.opponents()
            .map(|p| p.id)
            .filter(|id| can(Action::Claim(Some(*id))))
            .collect()
    } else {
        Vec::new()
    };
    let void = if pact.rules().wager_based {
        !wagers.is_empty()
    } else {
        can(Action::Claim(None))
    };
    Actions {
        respond: can(Action::Respond),
        cancel: can(Action::Cancel),
        join: can(Action::Join),
        withdraw: can(Action::Withdraw),
        judgment: can(Action::CallJudgment),
        claim: void && (pact.result_allowed(now) || pact.rules().early_claims),
        void,
        attach: can(Action::Attach),
        decide: pact
            .participants
            .iter()
            .filter(|p| p.state == ParticipantState::Requested && can(Action::Decide(p.id)))
            .map(|p| p.id)
            .collect(),
        confirm: pact
            .claims
            .iter()
            .filter(|c| can(Action::Confirm(c.id)))
            .map(|c| c.id)
            .collect(),
        escalate: pact
            .claims
            .iter()
            .filter(|c| can(Action::Escalate(c.id)))
            .map(|c| c.id)
            .collect(),
        wagers,
    }
}

/// Whether `user_id` may do `action` to `pact` at `now`.
pub fn check(action: Action, pact: &Pact, user_id: i64, now: DateTime<Utc>) -> Result<(), Denial> {
    let mine = pact.row_of(user_id);
    match action {
        Action::Respond => {
            let mine = mine.ok_or(NOT_IN_IT)?;
            if !pact.status.running() {
                return Err(NOT_CURRENT);
            }
            if mine.state != ParticipantState::Invited {
                return Err(conflict(
                    "already_answered",
                    "Odpowiedziałeś(-aś) już na to zaproszenie.",
                ));
            }
            if pact.claim_in_flight() {
                return Err(IN_FLIGHT);
            }
        }
        Action::Cancel => {
            if user_id != pact.creator_id {
                return Err(forbidden("not_creator", "Tylko autor może anulować umowę."));
            }
            if pact.status.finished() {
                return Err(NOT_CURRENT);
            }
            if pact.status != super::Status::Proposed {
                return Err(conflict("pact_started", "Umowa już się rozpoczęła."));
            }
        }
        Action::Join => {
            if !pact.rules().joinable {
                return Err(conflict(
                    "not_joinable",
                    "Do tego rodzaju umowy nie można dołączać.",
                ));
            }
            if !pact.status.running() || pact.due_passed(now) {
                return Err(conflict(
                    "joining_closed",
                    "Do tej umowy nie można już dołączyć.",
                ));
            }
            if pact.claim_in_flight() {
                return Err(IN_FLIGHT);
            }
            if mine.is_some_and(|p| !p.state.may_retry()) {
                return Err(conflict(
                    "already_in",
                    "Już bierzesz udział w tej umowie albo czekasz na decyzję.",
                ));
            }
        }
        Action::Withdraw => {
            let mine = mine.ok_or(NOT_IN_IT)?;
            if mine.state != ParticipantState::Requested {
                return Err(conflict(
                    "no_request",
                    "Nie masz oczekującej prośby w tej umowie.",
                ));
            }
            if !pact.status.running() {
                return Err(NOT_CURRENT);
            }
        }
        Action::Decide(id) => {
            let requester = pact
                .participant(id)
                .filter(|p| p.state == ParticipantState::Requested)
                .ok_or(conflict(
                    "request_decided",
                    "Ta prośba nie czeka już na decyzję.",
                ))?;
            let consent = requester
                .consents
                .iter()
                .find(|c| c.approver_id == user_id)
                .ok_or(forbidden("not_approver", "Nie decydujesz o tej prośbie."))?;
            if consent.approved.is_some() {
                return Err(conflict(
                    "already_decided",
                    "Podjąłeś(-ęłaś) już decyzję w tej sprawie.",
                ));
            }
            if !pact.status.running() {
                return Err(NOT_CURRENT);
            }
            if pact.claim_in_flight() {
                return Err(IN_FLIGHT);
            }
        }
        Action::Claim(wager) => claim(pact, user_id, wager)?,
        Action::Confirm(id) | Action::Dispute(id) => {
            let claim = pact.claim(id).ok_or(Denial::NotFound)?;
            if claim.state != ClaimState::Pending {
                return Err(conflict(
                    "claim_decided",
                    "Ten wynik nie czeka już na potwierdzenie.",
                ));
            }
            if user_id == claim.proposed_by {
                return Err(forbidden(
                    "own_claim",
                    "Wynik musi potwierdzić druga strona.",
                ));
            }
            if !pact.confirmers(claim).contains(&user_id) {
                return Err(forbidden(
                    "not_party",
                    "Tylko strony zakładu mogą potwierdzić wynik.",
                ));
            }
            if matches!(action, Action::Confirm(_)) && claim.confirmed_by.contains(&user_id) {
                return Err(conflict(
                    "already_confirmed",
                    "Potwierdziłeś(-aś) już ten wynik.",
                ));
            }
        }
        Action::Escalate(id) => {
            let claim = pact.claim(id).ok_or(Denial::NotFound)?;
            if !pact.parties(claim).contains(&user_id) {
                return Err(forbidden(
                    "not_party",
                    "Tylko strony sporu mogą oddać go pod głosowanie.",
                ));
            }
            if claim.state != ClaimState::Disputed || claim.ruling.is_some() {
                return Err(conflict(
                    "not_disputed",
                    "Ten wynik nie jest sporny albo już trafił pod głosowanie.",
                ));
            }
            if !pact.status.deciding() {
                return Err(NOT_CURRENT);
            }
        }
        Action::CallJudgment => {
            if !pact.rules().judged_by_everyone {
                return Err(conflict(
                    "not_judged",
                    "Ta umowa nie jest oceniana w głosowaniu.",
                ));
            }
            if !pact.status.deciding() {
                return Err(NOT_CURRENT);
            }
            if !pact.result_allowed(now) && user_id != pact.creator_id {
                return Err(forbidden(
                    "too_early",
                    "Postanowienie można ocenić dopiero po upływie terminu.",
                ));
            }
            if pact.judgment_poll_id.is_some() {
                return Err(conflict(
                    "judgment_running",
                    "Posłowie już oceniają to postanowienie.",
                ));
            }
        }
        Action::Attach => {
            if !mine.is_some_and(|p| p.state.may_attach()) {
                return Err(forbidden(
                    "not_participant",
                    "Tylko uczestnicy mogą dodawać zdjęcia do umowy.",
                ));
            }
            if pact.attachments >= MAX_ATTACHMENTS {
                return Err(Denial::Invalid {
                    code: "too_many_attachments",
                    message: "Do umowy można dodać maksymalnie 10 zdjęć.",
                });
            }
        }
        Action::RemoveAttachment { uploaded_by } => {
            if user_id != uploaded_by && user_id != pact.creator_id {
                return Err(forbidden(
                    "not_uploader",
                    "Zdjęcie może usunąć jego autor albo autor umowy.",
                ));
            }
        }
    }
    Ok(())
}

/// Who may claim, and about what (the claimed result is checked by [`claim_timing`]).
fn claim(pact: &Pact, user_id: i64, wager: Option<i64>) -> Result<(), Denial> {
    if !pact.status.deciding() {
        return Err(NOT_CURRENT);
    }
    if pact.rules().judged_by_everyone {
        return Err(conflict(
            "judged_by_vote",
            "To postanowienie oceniają wszyscy posłowie w głosowaniu w Sejmiku.",
        ));
    }
    let scope = if pact.rules().wager_based {
        let wager = wager
            .and_then(|id| pact.participant(id))
            .filter(|p| p.role == Role::Opponent)
            .ok_or(Denial::Invalid {
                code: "wager_required",
                message: "Wskaż zakład, którego dotyczy wynik (wager_id).",
            })?;
        if user_id != pact.creator_id && user_id != wager.user_id {
            return Err(forbidden(
                "not_party",
                "Tylko strony zakładu mogą podać wynik.",
            ));
        }
        if wager.state != ParticipantState::Active {
            return Err(conflict("wager_inactive", "Ten zakład nie jest aktywny."));
        }
        Some(wager.id)
    } else {
        if !pact.is_active(user_id) {
            return Err(forbidden("not_party", "Tylko uczestnicy mogą podać wynik."));
        }
        None
    };
    if pact.ruling_open(scope) {
        return Err(conflict(
            "ruling_open",
            "Trwa głosowanie w sprawie spornego wyniku.",
        ));
    }
    Ok(())
}

/// With a deadline ahead, the only claims are calling it off and those the kind allows early.
pub fn claim_timing(pact: &Pact, outcome: &Outcome, now: DateTime<Utc>) -> Result<(), Denial> {
    if pact.result_allowed(now) || outcome.early_ok(pact.kind) {
        return Ok(());
    }
    Err(conflict(
        "too_early",
        "Wynik można podać dopiero po upływie terminu. Wcześniej można tylko unieważnić.",
    ))
}

#[cfg(test)]
mod tests {
    use chrono::TimeDelta;

    use super::*;
    use crate::pacts::kind::PactKind;
    use crate::pacts::state::tests::{claim as claim_row, pact, row};
    use crate::pacts::state::{Consent, Participant, Ruling, Status};

    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000, 0).unwrap()
    }

    fn started_bet() -> Pact {
        let mut bet = pact(PactKind::Bet);
        bet.status = Status::Active;
        bet.participants[1].state = ParticipantState::Active;
        bet.participants[2].state = ParticipantState::Active;
        bet
    }

    #[test]
    fn an_invitee_answers_once_and_strangers_not_at_all() {
        let bet = pact(PactKind::Bet);
        assert!(actions(&bet, 2, now()).respond);
        assert_eq!(check(Action::Respond, &bet, 9, now()), Err(NOT_IN_IT));
        let mut answered = bet.clone();
        answered.participants[1].state = ParticipantState::Active;
        assert!(matches!(
            check(Action::Respond, &answered, 2, now()),
            Err(Denial::Conflict {
                code: "already_answered",
                ..
            })
        ));
    }

    #[test]
    fn only_the_creator_cancels_and_only_before_it_starts() {
        let bet = pact(PactKind::Bet);
        assert!(actions(&bet, 1, now()).cancel);
        assert!(matches!(
            check(Action::Cancel, &bet, 2, now()),
            Err(Denial::Forbidden { .. })
        ));
        assert!(matches!(
            check(Action::Cancel, &started_bet(), 1, now()),
            Err(Denial::Conflict {
                code: "pact_started",
                ..
            })
        ));
    }

    #[test]
    fn joining_needs_a_running_pact_before_its_deadline() {
        let mut bet = started_bet();
        assert!(actions(&bet, 9, now()).join);
        assert!(!actions(&bet, 2, now()).join, "already in");
        bet.due_at = Some(now());
        assert!(!actions(&bet, 9, now()).join);
        let resolution = pact(PactKind::Resolution);
        assert!(!actions(&resolution, 9, now()).join);
        let mut retry = started_bet();
        retry.participants[1].state = ParticipantState::Declined;
        assert!(actions(&retry, 2, now()).join);
    }

    #[test]
    fn join_requests_are_decided_by_their_approvers_once() {
        let mut bet = started_bet();
        bet.participants.push(Participant {
            consents: vec![Consent {
                approver_id: 1,
                approved: None,
            }],
            ..row(40, 4, Role::Opponent, ParticipantState::Requested)
        });
        assert_eq!(actions(&bet, 1, now()).decide, [40]);
        assert!(actions(&bet, 4, now()).withdraw);
        assert!(matches!(
            check(Action::Decide(40), &bet, 2, now()),
            Err(Denial::Forbidden { .. })
        ));
        bet.participants[3].consents[0].approved = Some(true);
        assert_eq!(actions(&bet, 1, now()).decide, Vec::<i64>::new());
        bet.status = Status::Resolved;
        bet.participants[3].consents[0].approved = None;
        assert_eq!(check(Action::Decide(40), &bet, 1, now()), Err(NOT_CURRENT));
        assert_eq!(check(Action::Withdraw, &bet, 4, now()), Err(NOT_CURRENT));
    }

    #[test]
    fn a_wager_is_claimed_by_its_two_sides_after_the_deadline_or_called_off_before() {
        let mut bet = started_bet();
        bet.due_at = Some(now() + TimeDelta::days(1));
        let host = actions(&bet, 1, now());
        assert_eq!(host.wagers, [20, 30]);
        assert!(host.void && !host.claim);
        assert_eq!(actions(&bet, 2, now()).wagers, [20]);
        assert_eq!(actions(&bet, 9, now()).wagers, Vec::<i64>::new());
        let host_win = Outcome::Winner {
            winner: "host".into(),
        };
        assert!(claim_timing(&bet, &host_win, now()).is_err());
        assert!(claim_timing(&bet, &Outcome::VOID, now()).is_ok());
        bet.due_at = Some(now());
        assert!(actions(&bet, 1, now()).claim);
        assert!(matches!(
            check(Action::Claim(Some(10)), &bet, 1, now()),
            Err(Denial::Invalid { .. })
        ));
        assert!(matches!(
            check(Action::Claim(Some(20)), &bet, 3, now()),
            Err(Denial::Forbidden { .. })
        ));
    }

    #[test]
    fn a_shared_goal_can_be_claimed_done_early() {
        let mut goal = pact(PactKind::SharedGoal);
        goal.status = Status::Active;
        goal.participants[1].state = ParticipantState::Active;
        goal.due_at = Some(now() + TimeDelta::days(365));
        let can = actions(&goal, 2, now());
        assert!(can.claim && can.void);
        assert_eq!(can.wagers, Vec::<i64>::new());
        assert!(!actions(&goal, 3, now()).void, "still only invited");
        assert!(claim_timing(&goal, &Outcome::Done { done: true }, now()).is_ok());
        assert!(claim_timing(&goal, &Outcome::Done { done: false }, now()).is_err());
    }

    #[test]
    fn claims_are_confirmed_by_the_other_side_and_escalated_by_either() {
        let mut bet = started_bet();
        bet.claims
            .push(claim_row(5, Some(20), 1, ClaimState::Pending));
        assert_eq!(actions(&bet, 2, now()).confirm, [5]);
        assert_eq!(actions(&bet, 1, now()).confirm, Vec::<i64>::new());
        assert_eq!(actions(&bet, 3, now()).confirm, Vec::<i64>::new());
        assert_eq!(
            check(Action::Confirm(6), &bet, 2, now()),
            Err(Denial::NotFound)
        );
        bet.claims[0].state = ClaimState::Disputed;
        assert_eq!(actions(&bet, 1, now()).escalate, [5]);
        assert_eq!(actions(&bet, 2, now()).escalate, [5]);
        assert_eq!(actions(&bet, 3, now()).escalate, Vec::<i64>::new());
        bet.claims[0].ruling = Some(Ruling {
            poll_id: 7,
            open: true,
        });
        assert_eq!(actions(&bet, 1, now()).escalate, Vec::<i64>::new());
        assert!(
            !actions(&bet, 1, now()).wagers.contains(&20),
            "no new claim while the vote runs"
        );
    }

    #[test]
    fn a_resolution_is_judged_by_vote_called_early_only_by_its_author() {
        let mut resolution = pact(PactKind::Resolution);
        resolution.status = Status::Active;
        resolution.participants.truncate(1);
        resolution.due_at = Some(now() + TimeDelta::days(1));
        assert!(actions(&resolution, 1, now()).judgment);
        assert!(!actions(&resolution, 2, now()).judgment);
        assert!(!actions(&resolution, 1, now()).void, "judged by vote only");
        resolution.due_at = Some(now());
        assert!(actions(&resolution, 2, now()).judgment);
        resolution.judgment_poll_id = Some(3);
        assert!(!actions(&resolution, 2, now()).judgment);
    }

    #[test]
    fn pictures_come_from_participants_and_go_with_their_uploader_or_the_creator() {
        let mut bet = started_bet();
        assert!(actions(&bet, 2, now()).attach);
        assert!(!actions(&bet, 9, now()).attach);
        bet.attachments = MAX_ATTACHMENTS;
        assert!(!actions(&bet, 2, now()).attach);
        let remove = |user| {
            check(
                Action::RemoveAttachment { uploaded_by: 2 },
                &bet,
                user,
                now(),
            )
        };
        assert!(remove(2).is_ok() && remove(1).is_ok() && remove(3).is_err());
    }
}
