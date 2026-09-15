use ledalert::{
    config::Config,
    history::{EditorHistory, EditorSnapshot},
};

fn snapshot(width: f32) -> EditorSnapshot {
    let mut config = Config::default();
    config.room.width = width;
    EditorSnapshot {
        address: config.device.address.to_string(),
        config,
    }
}

#[test]
fn continuous_gesture_undoes_to_start_and_redoes_to_final_draft() {
    let start = snapshot(5.0);
    let mut history = EditorHistory::new(start.config.clone(), start.address.clone());
    for width in [5.5, 6.0, 7.0] {
        let draft = snapshot(width);
        assert!(history.record(&draft.config, &draft.address, Some(10)));
    }
    history.finish_group();

    assert!(history.can_undo());
    assert!(!history.can_redo());
    assert_eq!(history.undo(), Some(start));
    assert!(!history.can_undo());
    assert!(history.can_redo());
    assert_eq!(history.redo(), Some(snapshot(7.0)));
    assert_eq!(history.redo(), None);
}

#[test]
fn discrete_actions_group_changes_and_finished_groups_are_separate_steps() {
    let start = snapshot(5.0);
    let mut history = EditorHistory::new(start.config.clone(), start.address.clone());
    for (width, group) in [(6.0, None), (7.0, None), (8.0, Some(1)), (9.0, Some(2))] {
        let draft = snapshot(width);
        assert!(history.record(&draft.config, &draft.address, group));
    }
    history.finish_group();
    let last = snapshot(10.0);
    assert!(history.record(&last.config, &last.address, Some(2)));

    for width in [9.0, 8.0, 7.0, 6.0, 5.0] {
        assert_eq!(history.undo(), Some(snapshot(width)));
    }
    assert_eq!(history.undo(), None);
    for width in [6.0, 7.0, 8.0, 9.0, 10.0] {
        assert_eq!(history.redo(), Some(snapshot(width)));
    }
}

#[test]
fn changed_edit_after_undo_discards_redo_and_starts_a_new_gesture() {
    let start = snapshot(5.0);
    let middle = snapshot(6.0);
    let old_tip = snapshot(7.0);
    let branch = snapshot(8.0);
    let mut history = EditorHistory::new(start.config.clone(), start.address.clone());
    assert!(history.record(&middle.config, &middle.address, None));
    assert!(history.record(&old_tip.config, &old_tip.address, Some(1)));
    assert_eq!(history.undo(), Some(middle.clone()));

    assert!(history.record(&branch.config, &branch.address, Some(1)));
    assert!(!history.can_redo());
    assert_eq!(history.redo(), None);
    assert_eq!(history.undo(), Some(middle));
    assert_eq!(history.undo(), Some(start));
    assert_eq!(history.redo(), Some(snapshot(6.0)));
    assert_eq!(history.redo(), Some(branch));
}

#[test]
fn no_op_records_create_no_steps_and_preserve_redo() {
    let start = snapshot(5.0);
    let next = snapshot(6.0);
    let mut history = EditorHistory::new(start.config.clone(), start.address.clone());
    assert!(!history.record(&start.config, &start.address, None));
    assert!(!history.record(&start.config, &start.address, Some(1)));
    assert_eq!(history.undo(), None);
    assert_eq!(history.redo(), None);

    assert!(history.record(&next.config, &next.address, Some(1)));
    assert!(!history.record(&next.config, &next.address, Some(1)));
    assert_eq!(history.undo(), Some(start.clone()));
    assert!(!history.record(&start.config, &start.address, None));
    assert!(!history.record(&start.config, &start.address, Some(1)));
    assert!(history.can_redo());
    assert_eq!(history.redo(), Some(next));
    assert_eq!(history.undo(), Some(start));
    assert!(!history.can_undo());
}

#[test]
fn even_a_no_op_group_change_ends_the_previous_gesture() {
    let start = snapshot(5.0);
    let middle = snapshot(6.0);
    let last = snapshot(7.0);
    let mut history = EditorHistory::new(start.config.clone(), start.address.clone());
    assert!(history.record(&middle.config, &middle.address, Some(1)));
    assert!(!history.record(&middle.config, &middle.address, None));
    assert!(history.record(&last.config, &last.address, Some(1)));

    assert_eq!(history.undo(), Some(middle));
    assert_eq!(history.undo(), Some(start));
}

#[test]
fn address_only_drafts_restore_exact_text_without_changing_device_configuration() {
    let start = snapshot(5.0);
    let mut history = EditorHistory::new(start.config.clone(), start.address.clone());
    assert!(history.record(&start.config, "", None));
    assert!(history.record(&start.config, " 192.168.not-an-address ", None));

    assert_eq!(
        history.undo(),
        Some(EditorSnapshot {
            config: start.config.clone(),
            address: String::new(),
        })
    );
    assert_eq!(history.undo(), Some(start.clone()));
    history.redo().unwrap();
    assert_eq!(
        history.redo(),
        Some(EditorSnapshot {
            config: start.config,
            address: " 192.168.not-an-address ".into(),
        })
    );
}

#[test]
fn invalid_configuration_drafts_are_undoable_without_validation() {
    let start = snapshot(5.0);
    let invalid = snapshot(-1.0);
    let mut history = EditorHistory::new(start.config.clone(), start.address.clone());
    assert!(history.record(&invalid.config, &invalid.address, None));
    assert_eq!(history.undo(), Some(start));
    assert_eq!(history.redo(), Some(invalid));
}

#[test]
fn eviction_keeps_exactly_64_undo_steps_and_the_same_bounded_redo_timeline() {
    let start = snapshot(1.0);
    let mut history = EditorHistory::new(start.config, start.address);
    for width in 2..=81 {
        let draft = snapshot(width as f32);
        assert!(history.record(&draft.config, &draft.address, None));
    }

    for width in (17..=80).rev() {
        assert_eq!(history.undo(), Some(snapshot(width as f32)));
    }
    assert_eq!(history.undo(), None);
    assert!(!history.can_undo());
    for width in 18..=81 {
        assert_eq!(history.redo(), Some(snapshot(width as f32)));
    }
    assert_eq!(history.redo(), None);
    assert!(!history.can_redo());
}

#[test]
fn returning_a_gesture_to_its_original_state_removes_its_undo_step() {
    let start = snapshot(5.0);
    let moved = snapshot(6.0);
    let mut history = EditorHistory::new(start.config.clone(), start.address.clone());
    assert!(history.record(&moved.config, &moved.address, Some(1)));
    assert!(history.record(&start.config, &start.address, Some(1)));
    assert!(!history.can_undo());
    assert!(!history.can_redo());
    assert!(!history.record(&start.config, &start.address, Some(1)));

    // Continuing the same gesture after crossing its origin still has one step.
    assert!(history.record(&moved.config, &moved.address, Some(1)));
    let last = snapshot(7.0);
    assert!(history.record(&last.config, &last.address, Some(1)));
    assert_eq!(history.undo(), Some(start));
    assert_eq!(history.redo(), Some(last));
}

#[test]
fn cancelling_a_gesture_preserves_earlier_edits() {
    let start = snapshot(5.0);
    let middle = snapshot(6.0);
    let moved = snapshot(7.0);
    let mut history = EditorHistory::new(start.config.clone(), start.address.clone());
    assert!(history.record(&middle.config, &middle.address, None));
    assert!(history.record(&moved.config, &moved.address, Some(1)));
    assert!(history.record(&middle.config, &middle.address, Some(1)));
    history.finish_group();

    assert_eq!(history.undo(), Some(start));
    assert_eq!(history.redo(), Some(middle));
    assert!(!history.can_redo());
}
