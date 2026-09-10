//! Turning a [`UiCommand`] into world effects.
//!
//! `Game.tsx` calls `resetUI()` right after every `dispatch(...)`, so a
//! [`UiCommand::Dispatch`] means "send the action **and** go back to idle".
//! [`apply_ui_command`] is that rule, kept pure so it can be asserted without a
//! world: it mutates the two selection resources and hands back the action the
//! caller has to write to [`DispatchAction`](crate::bridge::DispatchAction).

use tcgop_engine::types::GameAction;

use crate::selection::{SelectedHandCard, UiCommand, UiMode};

/// Apply `command`, returning the action to dispatch (if any).
pub fn apply_ui_command(
    command: UiCommand,
    mode: &mut UiMode,
    selected: &mut SelectedHandCard,
) -> Option<GameAction> {
    match command {
        UiCommand::Ignore => None,
        UiCommand::Dispatch(action) => {
            *mode = UiMode::Idle;
            selected.0 = None;
            Some(action)
        }
        UiCommand::SetMode(next) => {
            *mode = next;
            None
        }
        UiCommand::SelectHandCard { instance_id, mode: next } => {
            selected.0 = Some(instance_id);
            *mode = next;
            None
        }
        UiCommand::Reset => {
            *mode = UiMode::Idle;
            selected.0 = None;
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tcgop_engine::types::Slot;

    fn state() -> (UiMode, SelectedHandCard) {
        (
            UiMode::SelectingSlot {
                card_id: "c1".into(),
            },
            SelectedHandCard(Some("c1".into())),
        )
    }

    #[test]
    fn dispatching_resets_the_whole_ui() {
        let (mut mode, mut selected) = state();
        let action = GameAction::DeployCharacter {
            instance_id: "c1".into(),
            slot: Slot::V1,
        };
        let out = apply_ui_command(UiCommand::Dispatch(action.clone()), &mut mode, &mut selected);
        assert_eq!(out, Some(action));
        assert_eq!(mode, UiMode::Idle);
        assert_eq!(selected.0, None);
    }

    #[test]
    fn ignoring_changes_nothing() {
        let (mut mode, mut selected) = state();
        let before = (mode.clone(), selected.clone());
        assert_eq!(apply_ui_command(UiCommand::Ignore, &mut mode, &mut selected), None);
        assert_eq!((mode, selected), before);
    }

    #[test]
    fn set_mode_keeps_the_selected_hand_card() {
        let (mut mode, mut selected) = state();
        let out = apply_ui_command(
            UiCommand::SetMode(UiMode::ActionMenu {
                instance_id: "u1".into(),
            }),
            &mut mode,
            &mut selected,
        );
        assert_eq!(out, None);
        assert_eq!(
            mode,
            UiMode::ActionMenu {
                instance_id: "u1".into()
            }
        );
        assert_eq!(selected.0, Some("c1".into()), "TS only clears on resetUI");
    }

    #[test]
    fn selecting_a_hand_card_records_it() {
        let mut mode = UiMode::Idle;
        let mut selected = SelectedHandCard(None);
        apply_ui_command(
            UiCommand::SelectHandCard {
                instance_id: "h9".into(),
                mode: UiMode::SelectingSlot {
                    card_id: "h9".into(),
                },
            },
            &mut mode,
            &mut selected,
        );
        assert_eq!(selected.0, Some("h9".into()));
        assert!(mode.is_selecting());
    }

    #[test]
    fn reset_goes_back_to_idle() {
        let (mut mode, mut selected) = state();
        assert_eq!(apply_ui_command(UiCommand::Reset, &mut mode, &mut selected), None);
        assert!(mode.is_idle());
        assert_eq!(selected.0, None);
    }
}
