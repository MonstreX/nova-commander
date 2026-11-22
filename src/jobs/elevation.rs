//! What copy, move and delete share of ADR-0007: the operation's answers to "Access denied".

use crate::vfs::local::elevated_session::{self, Policy};

/// What the rights are needed for: the second line of FAR's question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb {
    Create,
    /// A change made on an opened file (times, compression, a link's target): FAR's "to open".
    Open,
    SetOwner,
    Encrypt,
    Decrypt,
    Replace,
    SetAttributes,
    Delete,
    Copy,
    Move,
    Recycle,
    HardLink,
    SymbolicLink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Through the helper.
    Elevate,
    /// As it failed.
    Refuse,
    /// Esc: the operation stops.
    Cancel,
}

/// "Do this for all current objects": the answer for the rest of the operation.
#[derive(Debug, Default)]
pub struct Approval {
    rest: Option<bool>,
    /// The helper could not be started (a refused prompt): this operation does not try again.
    given_up: bool,
}

impl Approval {
    /// The session's answer, the operation's, or `ask`'s, which also says whether it holds for the rest.
    pub fn decide(&mut self, ask: impl FnOnce() -> (Decision, bool)) -> Decision {
        if self.given_up {
            return Decision::Refuse;
        }
        match elevated_session::policy() {
            Policy::Always => return Decision::Elevate,
            Policy::Never => return Decision::Refuse,
            Policy::Ask => {}
        }
        if let Some(elevate) = self.rest {
            return if elevate {
                Decision::Elevate
            } else {
                Decision::Refuse
            };
        }
        let (decision, for_all) = ask();
        if for_all && decision != Decision::Cancel {
            self.rest = Some(decision == Decision::Elevate);
        }
        decision
    }

    /// ADR-0007 §3: the object fails with its own error, the rest of the operation is not asked about.
    pub fn give_up(&mut self) {
        self.given_up = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asked_once_when_it_is_for_all_and_each_time_when_not() {
        let mut approval = Approval::default();
        assert_eq!(
            approval.decide(|| (Decision::Elevate, false)),
            Decision::Elevate
        );
        assert_eq!(
            approval.decide(|| (Decision::Refuse, true)),
            Decision::Refuse
        );
        assert_eq!(
            approval.decide(|| panic!("the rest was answered")),
            Decision::Refuse
        );
    }

    #[test]
    fn the_sessions_answer_goes_before_the_question() {
        let mut approval = Approval::default();
        elevated_session::set_policy(Policy::Always);
        assert_eq!(approval.decide(|| panic!("not asked")), Decision::Elevate);
        elevated_session::set_policy(Policy::Never);
        assert_eq!(approval.decide(|| panic!("not asked")), Decision::Refuse);
        elevated_session::set_policy(Policy::Ask);
    }

    #[test]
    fn a_helper_that_could_not_start_ends_the_asking_even_with_the_sessions_yes() {
        let mut approval = Approval::default();
        approval.give_up();
        elevated_session::set_policy(Policy::Always);
        assert_eq!(approval.decide(|| panic!("not asked")), Decision::Refuse);
        elevated_session::set_policy(Policy::Ask);
    }

    #[test]
    fn the_sessions_answer_goes_before_the_answer_for_the_rest_of_the_operation() {
        let mut refused = Approval::default();
        assert_eq!(
            refused.decide(|| (Decision::Refuse, true)),
            Decision::Refuse
        );
        elevated_session::set_policy(Policy::Always);
        assert_eq!(refused.decide(|| panic!("not asked")), Decision::Elevate);

        let mut allowed = Approval::default();
        elevated_session::set_policy(Policy::Ask);
        assert_eq!(
            allowed.decide(|| (Decision::Elevate, true)),
            Decision::Elevate
        );
        elevated_session::set_policy(Policy::Never);
        assert_eq!(allowed.decide(|| panic!("not asked")), Decision::Refuse);
        elevated_session::set_policy(Policy::Ask);
    }

    #[test]
    fn a_yes_for_all_answers_the_rest_without_asking() {
        let mut approval = Approval::default();
        assert_eq!(
            approval.decide(|| (Decision::Elevate, true)),
            Decision::Elevate
        );
        assert_eq!(
            approval.decide(|| panic!("the rest was answered")),
            Decision::Elevate
        );
    }

    #[test]
    fn a_helper_that_could_not_start_goes_before_a_yes_for_all_and_before_the_question() {
        let mut answered = Approval::default();
        assert_eq!(
            answered.decide(|| (Decision::Elevate, true)),
            Decision::Elevate
        );
        answered.give_up();
        assert_eq!(answered.decide(|| panic!("not asked")), Decision::Refuse);

        let mut unanswered = Approval::default();
        unanswered.give_up();
        assert_eq!(unanswered.decide(|| panic!("not asked")), Decision::Refuse);
    }

    #[test]
    fn a_cancel_is_not_kept_for_the_rest() {
        let mut approval = Approval::default();
        assert_eq!(
            approval.decide(|| (Decision::Cancel, true)),
            Decision::Cancel
        );
        assert_eq!(
            approval.decide(|| (Decision::Elevate, false)),
            Decision::Elevate
        );
    }
}
