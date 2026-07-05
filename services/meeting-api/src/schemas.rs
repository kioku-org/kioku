//! Faithful port of schemas.py's status/completion-reason/failure-stage enums and the state
//! machine (get_valid_status_transitions / is_valid_status_transition / get_status_source).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeetingStatus {
    Requested,
    Joining,
    AwaitingAdmission,
    Active,
    NeedsHumanHelp,
    Stopping,
    Completed,
    Failed,
}

impl MeetingStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            MeetingStatus::Requested => "requested",
            MeetingStatus::Joining => "joining",
            MeetingStatus::AwaitingAdmission => "awaiting_admission",
            MeetingStatus::Active => "active",
            MeetingStatus::NeedsHumanHelp => "needs_human_help",
            MeetingStatus::Stopping => "stopping",
            MeetingStatus::Completed => "completed",
            MeetingStatus::Failed => "failed",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "requested" => MeetingStatus::Requested,
            "joining" => MeetingStatus::Joining,
            "awaiting_admission" => MeetingStatus::AwaitingAdmission,
            "active" => MeetingStatus::Active,
            "needs_human_help" => MeetingStatus::NeedsHumanHelp,
            "stopping" => MeetingStatus::Stopping,
            "completed" => MeetingStatus::Completed,
            "failed" => MeetingStatus::Failed,
            _ => return None,
        })
    }

    /// Mirrors get_valid_status_transitions().
    pub fn valid_next(&self) -> &'static [MeetingStatus] {
        use MeetingStatus::*;
        match self {
            Requested => &[Joining, Failed, Completed, Stopping],
            Joining => &[AwaitingAdmission, Active, NeedsHumanHelp, Failed, Completed, Stopping],
            AwaitingAdmission => &[Active, NeedsHumanHelp, Failed, Completed, Stopping],
            NeedsHumanHelp => &[Active, Failed, Stopping, Completed],
            Active => &[Stopping, Completed, Failed],
            Stopping => &[Completed, Failed],
            Completed => &[],
            Failed => &[],
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, MeetingStatus::Completed | MeetingStatus::Failed)
    }
}

pub fn is_valid_status_transition(from: MeetingStatus, to: MeetingStatus) -> bool {
    from.valid_next().contains(&to)
}

/// Mirrors get_status_source().
pub fn get_status_source(from: MeetingStatus, to: MeetingStatus) -> &'static str {
    use MeetingStatus::*;
    if matches!(to, Stopping | Completed) {
        return "user";
    }
    const BOT_CALLBACK_TRANSITIONS: &[(MeetingStatus, MeetingStatus)] = &[
        (Requested, Joining),
        (Joining, AwaitingAdmission),
        (AwaitingAdmission, Active),
        (Active, Completed),
        (Stopping, Completed),
        (Requested, Failed),
        (Joining, Failed),
        (AwaitingAdmission, Failed),
        (Active, Failed),
        (Stopping, Failed),
        (Joining, NeedsHumanHelp),
        (AwaitingAdmission, NeedsHumanHelp),
        (NeedsHumanHelp, Active),
        (NeedsHumanHelp, Failed),
    ];
    if BOT_CALLBACK_TRANSITIONS.contains(&(from, to)) {
        return "bot_callback";
    }
    if to == Failed && from == Requested {
        return "validation_error";
    }
    "unknown"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeetingCompletionReason {
    Stopped,
    ValidationError,
    AwaitingAdmissionTimeout,
    AwaitingAdmissionRejected,
    LeftAlone,
    Evicted,
    MaxBotTimeExceeded,
    StoppedBeforeAdmission,
    StoppedWithNoAudio,
    JoinFailure,
}

impl MeetingCompletionReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::ValidationError => "validation_error",
            Self::AwaitingAdmissionTimeout => "awaiting_admission_timeout",
            Self::AwaitingAdmissionRejected => "awaiting_admission_rejected",
            Self::LeftAlone => "left_alone",
            Self::Evicted => "evicted",
            Self::MaxBotTimeExceeded => "max_bot_time_exceeded",
            Self::StoppedBeforeAdmission => "stopped_before_admission",
            Self::StoppedWithNoAudio => "stopped_with_no_audio",
            Self::JoinFailure => "join_failure",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "stopped" => Self::Stopped,
            "validation_error" => Self::ValidationError,
            "awaiting_admission_timeout" => Self::AwaitingAdmissionTimeout,
            "awaiting_admission_rejected" => Self::AwaitingAdmissionRejected,
            "left_alone" => Self::LeftAlone,
            "evicted" => Self::Evicted,
            "max_bot_time_exceeded" => Self::MaxBotTimeExceeded,
            "stopped_before_admission" => Self::StoppedBeforeAdmission,
            "stopped_with_no_audio" => Self::StoppedWithNoAudio,
            "join_failure" => Self::JoinFailure,
            _ => return None,
        })
    }

    /// Pack J.4 — explicit non-success reasons that always route to FAILED
    /// (for the system-initiated case; user-initiated is intercepted earlier).
    pub fn is_explicit_failure(&self) -> bool {
        matches!(
            self,
            Self::AwaitingAdmissionTimeout
                | Self::AwaitingAdmissionRejected
                | Self::Evicted
                | Self::MaxBotTimeExceeded
                | Self::ValidationError
                | Self::StoppedBeforeAdmission
                | Self::StoppedWithNoAudio
                | Self::JoinFailure
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeetingFailureStage {
    Requested,
    Joining,
    AwaitingAdmission,
    Active,
}

impl MeetingFailureStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::Joining => "joining",
            Self::AwaitingAdmission => "awaiting_admission",
            Self::Active => "active",
        }
    }
}

/// Mirrors _failure_stage_from_status() in callbacks.py.
pub fn failure_stage_from_status(status: MeetingStatus) -> MeetingFailureStage {
    match status {
        MeetingStatus::Requested => MeetingFailureStage::Requested,
        MeetingStatus::Joining => MeetingFailureStage::Joining,
        MeetingStatus::AwaitingAdmission => MeetingFailureStage::AwaitingAdmission,
        _ => MeetingFailureStage::Active,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_states_have_no_valid_transitions() {
        assert!(MeetingStatus::Completed.valid_next().is_empty());
        assert!(MeetingStatus::Failed.valid_next().is_empty());
    }

    #[test]
    fn requested_to_joining_is_valid_and_bot_sourced() {
        assert!(is_valid_status_transition(MeetingStatus::Requested, MeetingStatus::Joining));
        assert_eq!(get_status_source(MeetingStatus::Requested, MeetingStatus::Joining), "bot_callback");
    }

    #[test]
    fn requested_to_active_is_not_a_legal_direct_transition() {
        // Pack X finding: only via JOINING, never REQUESTED -> ACTIVE directly.
        assert!(!is_valid_status_transition(MeetingStatus::Requested, MeetingStatus::Active));
    }

    #[test]
    fn stopping_is_always_user_sourced() {
        assert_eq!(get_status_source(MeetingStatus::Active, MeetingStatus::Stopping), "user");
        assert_eq!(get_status_source(MeetingStatus::Joining, MeetingStatus::Stopping), "user");
    }

    #[test]
    fn requested_to_failed_is_bot_callback_not_validation_error() {
        // Faithful-port note: schemas.py's `to_status == FAILED and from_status == REQUESTED`
        // validation_error check is unreachable in the original — (REQUESTED, FAILED) is
        // already listed in bot_callback_transitions, which is checked first. Preserved here
        // exactly, quirk and all, rather than "fixed" into behavior Python never actually had.
        assert_eq!(get_status_source(MeetingStatus::Requested, MeetingStatus::Failed), "bot_callback");
    }
}
