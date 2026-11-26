//! Tape undo and redo without an editor in the way.

use super::*;

#[test]
fn time_never_goes_backwards() {
    let mut recording = Recording::default();
    recording.push(
        100,
        Command::Clear {
            layer: 0,
            color: [0.0; 4],
        },
    );
    recording.push(
        50,
        Command::Clear {
            layer: 0,
            color: [0.0; 4],
        },
    );
    assert_eq!(recording.duration_ms(), 100);
}

#[test]
fn undo_drops_every_segment_of_a_grouped_stroke() {
    let mut recording = Recording::default();
    recording.push_grouped(
        1,
        Some(7),
        Command::Clear {
            layer: 0,
            color: [0.0; 4],
        },
    );
    recording.push_grouped(
        2,
        Some(7),
        Command::Clear {
            layer: 0,
            color: [0.0; 4],
        },
    );
    recording.push_grouped(
        3,
        Some(7),
        Command::Clear {
            layer: 0,
            color: [0.0; 4],
        },
    );
    recording.push(
        4,
        Command::Clear {
            layer: 0,
            color: [0.0; 4],
        },
    );
    assert!(recording.undo());
    assert_eq!(
        recording.events().len(),
        3,
        "the ungrouped command survived"
    );
    assert!(recording.undo());
    assert!(
        recording.is_empty(),
        "a segment of the stroke stayed on the tape"
    );
}

#[test]
fn undo_drops_the_last_command() {
    let mut recording = Recording::default();
    recording.push(
        1,
        Command::Clear {
            layer: 0,
            color: [0.0; 4],
        },
    );
    recording.push(
        2,
        Command::Clear {
            layer: 0,
            color: [0.0; 4],
        },
    );
    assert!(recording.undo());
    assert_eq!(recording.events().len(), 1);
    assert_eq!(recording.duration_ms(), 1);
}

fn clear() -> Command {
    Command::Clear {
        layer: 0,
        color: [0.0; 4],
    }
}

#[test]
fn redo_restores_the_group_it_undid_in_order() {
    let mut recording = Recording::default();
    recording.push_grouped(1, Some(7), clear());
    recording.push_grouped(2, Some(7), clear());
    recording.push_grouped(3, Some(7), clear());
    let original = recording.events().to_vec();
    assert!(recording.undo());
    assert!(recording.is_empty(), "the undo left a segment behind");
    assert!(recording.redo());
    assert_eq!(recording.events(), original.as_slice());
}

#[test]
fn two_steps_undo_and_redo_in_order() {
    let mut recording = Recording::default();
    recording.push_grouped(10, Some(1), clear());
    recording.push_grouped(20, Some(1), clear());
    recording.push(30, clear());
    let original = recording.events().to_vec();
    assert!(recording.undo());
    assert!(recording.undo());
    assert!(recording.is_empty());
    assert!(recording.redo());
    assert!(recording.redo());
    assert_eq!(recording.events(), original.as_slice());
}

#[test]
fn a_new_command_after_undo_drops_the_undone_events() {
    let mut recording = Recording::default();
    recording.push(5, clear());
    assert!(recording.undo());
    recording.push(6, clear());
    assert!(recording.undo());
    assert!(
        recording.is_empty(),
        "undo resurrected the abandoned command"
    );
    assert!(recording.redo());
    assert_eq!(recording.events().len(), 1);
    assert_eq!(recording.events()[0].time_ms, 6);
    assert!(!recording.redo(), "the abandoned command came back");
}

#[test]
fn clear_drops_the_undone_events() {
    let mut recording = Recording::default();
    recording.push(5, clear());
    assert!(recording.undo());
    recording.clear();
    assert!(!recording.redo());
}

#[test]
fn a_tape_edit_drops_the_undone_events() {
    let mut recording = Recording::default();
    recording.push_grouped(1, Some(3), clear());
    assert!(recording.undo());
    recording.apply_edit(
        &TapeEdit::Recolor {
            group: 3,
            before: [0.0; 4],
            after: [1.0; 4],
        },
        true,
    );
    assert!(!recording.redo());
}

#[test]
fn redo_never_goes_back_in_time() {
    let mut recording = Recording::default();
    recording.push(100, clear());
    assert!(recording.undo());
    // A command already at a later time, as a tape edit that re-inserts an event would do. The
    // restored one must not replay before it.
    recording.events.push(Event {
        time_ms: 150,
        group: None,
        command: clear(),
    });
    assert!(recording.redo());
    assert_eq!(recording.events().last().unwrap().time_ms, 150);
}

#[test]
fn a_recorded_erase_keeps_its_strength_on_the_tape() {
    let mut recording = Recording::default();
    recording.push(
        0,
        Command::Erase {
            layer: 0,
            from: (1.0, 2.0),
            to: (3.0, 4.0),
            radius: 5.0,
            pressure: 1.0,
            brush: BrushKind::SoftRound,
            seed: 9,
            tilt: [0.0; 2],
            azimuth: 0.0,
            roll: 0.0,
            scale: BrushScale::Canvas,
            strength: 0.5,
        },
    );
    let tape = recording.tape(32, 32, 1);
    let bytes = drawkit_formats::write_dkp(&tape).unwrap();
    let read = drawkit_formats::read_dkp_bytes(&bytes).unwrap();
    match &read.events[0].command {
        drawkit_formats::TapeCommand::Erase { strength, .. } => {
            assert_eq!(*strength, 0.5, "the tape lost the erase strength");
        }
        other => panic!("the tape read back the wrong command: {other:?}"),
    }
    let restored = Recording::from_tape(&read.events);
    match restored.events()[0].command {
        Command::Erase { strength, .. } => assert_eq!(strength, 0.5),
        _ => panic!("the recording lost the erase command"),
    }
}
