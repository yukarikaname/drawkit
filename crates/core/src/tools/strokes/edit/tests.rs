use std::collections::HashSet;
use std::time::Instant;

use super::*;
use crate::record::TapeEdit;
use crate::surface::TILE;

const RED: Rgba = [1.0, 0.0, 0.0, 1.0];
const BLUE: Rgba = [0.0, 0.0, 1.0, 1.0];
const GREEN: Rgba = [0.0, 1.0, 0.0, 1.0];

fn paint(editor: &mut Editor, from: (f32, f32), to: (f32, f32), radius: f32, color: Rgba) -> u64 {
    editor.begin_stroke(0);
    editor.stroke_at(from, to, radius, 1.0, color, 0);
    editor.end_stroke();
    editor
        .recording
        .events()
        .last()
        .and_then(|event| event.group)
        .expect("the stroke was recorded")
}

fn canvas(editor: &Editor) -> Vec<Rgba> {
    let (width, height) = (editor.document.width(), editor.document.height());
    editor.document.layers[0]
        .surface
        .snapshot(Rect::new(0, 0, width, height))
}

/// A whole-tape replay, the reference the tile edit has to match.
fn replay(editor: &Editor) -> Vec<Rgba> {
    let alpha_locked = editor.document.layers[0].alpha_locked;
    let scratch = editor
        .tape_region(0, None, alpha_locked, None)
        .expect("the tape replays");
    let (width, height) = (editor.document.width(), editor.document.height());
    scratch.snapshot(Rect::new(0, 0, width, height))
}

fn two_overlapping(editor: &mut Editor) -> (u64, u64) {
    let first = paint(editor, (20.0, 45.0), (100.0, 45.0), 7.0, RED);
    let second = paint(editor, (60.0, 15.0), (60.0, 105.0), 7.0, BLUE);
    (first, second)
}

/// Erase inside a pointer session, the way a shell that wraps the eraser does. The writer entry
/// has to exist before `end_stroke`: the per-segment extension is what a tile edit relies on.
fn erase(editor: &mut Editor, from: (f32, f32), to: (f32, f32), radius: f32, strength: f32) -> u64 {
    editor.eraser_strength = strength;
    editor.begin_stroke(0);
    editor.erase_at(from, to, radius, 1.0, 0);
    let group = editor
        .recording
        .events()
        .last()
        .and_then(|event| event.group)
        .expect("the erase was recorded");
    assert!(
        editor.document.layers[0]
            .surface
            .tiles_for_group(group)
            .is_some(),
        "the eraser recorded no writer entry for its tiles"
    );
    editor.end_stroke();
    group
}

#[test]
fn recolouring_a_stroke_the_eraser_crossed_matches_the_whole_tape() {
    let mut editor = Editor::new(128, 128, 1 << 20);
    let stroke = paint(&mut editor, (20.0, 45.0), (100.0, 45.0), 7.0, RED);
    let eraser = erase(&mut editor, (60.0, 20.0), (60.0, 70.0), 6.0, 1.0);
    assert!(
        editor.document.layers[0]
            .surface
            .tiles_for_group(eraser)
            .is_some(),
        "the erase group lost its entry"
    );

    assert!(
        editor.recolor_stroke(stroke, GREEN),
        "the tile edit refused because of the eraser"
    );
    assert_eq!(
        canvas(&editor),
        replay(&editor),
        "the tile replay diverged from the whole tape"
    );
    assert!(
        editor.document.layers[0]
            .surface
            .tiles_for_group(stroke)
            .is_some(),
        "a recolour dropped the writer entry"
    );
    let repainted = editor.document.layers[0].surface.get(30, 45);
    assert!(
        repainted[1] > 0.9 && repainted[0] < 0.1,
        "the edited stroke is not green: {repainted:?}"
    );
    let erased = editor.document.layers[0].surface.get(60, 45);
    assert!(
        erased[3] < 0.05,
        "the erase was not replayed over the recolour: {erased:?}"
    );
}

#[test]
fn a_recolour_keeps_the_erased_pixels_at_the_recorded_strength() {
    let mut editor = Editor::new(128, 128, 1 << 20);
    let stroke = paint(&mut editor, (20.0, 45.0), (100.0, 45.0), 7.0, RED);
    erase(&mut editor, (60.0, 20.0), (60.0, 70.0), 6.0, 0.5);

    assert!(
        editor.recolor_stroke(stroke, GREEN),
        "a half-strength eraser made the tile edit refuse"
    );
    assert_eq!(
        canvas(&editor),
        replay(&editor),
        "the erase replayed at a strength the tape did not record"
    );
    let half = editor.document.layers[0].surface.get(60, 45);
    assert!(
        half[3] > 0.2 && half[3] < 0.8,
        "the half erase was not kept: {half:?}"
    );
}

#[test]
fn a_deleted_group_left_the_writer_map_and_the_replay_closure() {
    let mut editor = Editor::new(128, 128, 1 << 20);
    let (first, second) = two_overlapping(&mut editor);
    let (_, shared) = editor.edit_region(0, second).expect("the second entry");
    assert!(shared.contains(&first), "the strokes did not share a tile");

    assert!(editor.delete_stroke(first));
    assert_eq!(
        editor.document.layers[0].surface.tiles_for_group(first),
        None,
        "the removed group kept a writer entry"
    );
    let (_, groups) = editor.edit_region(0, second).expect("the second entry");
    assert!(
        !groups.contains(&first),
        "the deleted group joined the replay: {groups:?}"
    );
    assert_eq!(groups.len(), 1);

    assert!(editor.recolor_stroke(second, GREEN));
    assert_eq!(
        canvas(&editor),
        replay(&editor),
        "the tile replay diverged from the whole tape"
    );
}

#[test]
fn recolouring_one_of_two_overlapping_strokes_matches_the_whole_tape() {
    let mut editor = Editor::new(128, 128, 1 << 20);
    let (first, second) = two_overlapping(&mut editor);
    assert!(editor.recolor_stroke(first, GREEN));

    assert_eq!(
        canvas(&editor),
        replay(&editor),
        "the tile replay diverged from the whole tape"
    );
    let recoloured = editor.document.layers[0].surface.get(30, 45);
    assert!(
        recoloured[1] > 0.9 && recoloured[0] < 0.1,
        "the edited stroke is not green: {recoloured:?}"
    );
    let untouched = editor.document.layers[0].surface.get(60, 95);
    assert!(
        untouched[2] > 0.9 && untouched[0] < 0.1,
        "the other stroke changed colour: {untouched:?}"
    );
    assert_eq!(
        editor.recording.group_color(second),
        Some(BLUE),
        "the edit recoloured the wrong group"
    );
}

#[test]
fn deleting_the_earlier_overlapping_stroke_keeps_the_later_one_intact() {
    let mut editor = Editor::new(128, 128, 1 << 20);
    let (first, second) = two_overlapping(&mut editor);
    assert!(editor.delete_stroke(first));

    assert_eq!(
        canvas(&editor),
        replay(&editor),
        "the tile replay diverged from the whole tape"
    );
    assert_eq!(
        editor.document.layers[0].surface.get(30, 45),
        [0.0; 4],
        "the deleted stroke left paint behind"
    );
    let above = editor.document.layers[0].surface.get(60, 95);
    assert!(
        above[2] > 0.9 && above[0] < 0.1,
        "the later stroke was not re-rendered: {above:?}"
    );
    assert_eq!(
        editor.recording.group_color(second),
        Some(BLUE),
        "the deletion removed the wrong group"
    );
}

#[test]
fn a_layer_with_pixels_the_tape_did_not_paint_is_refused() {
    let mut editor = Editor::new(64, 64, 1 << 20);
    let group = paint(&mut editor, (10.0, 10.0), (40.0, 10.0), 4.0, RED);
    editor.document.layers[0]
        .surface
        .set(25, 10, [0.0, 1.0, 0.0, 1.0]);
    assert!(!editor.recolor_stroke(group, BLUE));
    assert_eq!(
        editor.recording.group_color(group),
        Some(RED),
        "a refused edit changed the tape"
    );
}

#[test]
fn a_stroke_on_another_tile_does_not_join_the_replay() {
    let mut editor = Editor::new(1024, 1024, 1 << 20);
    let first = paint(&mut editor, (100.0, 100.0), (140.0, 100.0), 4.0, RED);
    let second = paint(&mut editor, (700.0, 700.0), (740.0, 700.0), 4.0, BLUE);

    let (tiles, groups) = editor.edit_region(0, first).expect("the writer entry");
    let own = editor.document.layers[0]
        .surface
        .tiles_for_group(first)
        .expect("the first stroke's tiles");
    assert_eq!(
        groups.len(),
        1,
        "the far stroke was replayed too: {groups:?}"
    );
    assert!(groups.contains(&first));
    assert!(!groups.contains(&second));
    assert_eq!(
        tiles.len(),
        own.len(),
        "the closure grew past the stroke's own tiles"
    );
}

#[test]
fn undo_after_a_recolour_restores_the_pre_edit_pixels() {
    let mut editor = Editor::new(128, 128, 1 << 20);
    let (first, _second) = two_overlapping(&mut editor);
    let before = canvas(&editor);
    assert!(editor.recolor_stroke(first, GREEN));
    assert_ne!(canvas(&editor), before, "the edit did not change a pixel");

    assert!(editor.undo().is_some());
    assert_eq!(canvas(&editor), before, "undo did not restore the layer");
    assert_eq!(editor.recording.group_color(first), Some(RED));
}

fn four_k_editor() -> (Editor, u64) {
    let mut editor = Editor::new(4096, 4096, 1 << 20);
    let mut last = 0;
    // One small stroke at the centre of each of 200 distinct tiles, so the target shares no tile
    // with any other stroke and the old path still has 200 groups to replay.
    for index in 0..200u32 {
        let tx = index % 16;
        let ty = index / 16;
        let x = (tx as f32 + 0.5) * TILE as f32;
        let y = (ty as f32 + 0.5) * TILE as f32;
        last = paint(&mut editor, (x - 8.0, y), (x + 8.0, y), 4.0, RED);
    }
    (editor, last)
}

#[test]
fn a_4k_layer_with_two_hundred_strokes_replays_one_tile() {
    let (mut fast, group) = four_k_editor();
    let bounds = fast.stroke_bounds(group).expect("bounds");
    let (tiles, groups) = fast.edit_region(0, group).expect("the writer entry");
    assert_eq!(groups.len(), 1, "a neighbour joined the replay");
    let start = Instant::now();
    assert!(fast.recolor_stroke(group, GREEN));
    let fast_time = start.elapsed();
    let after = fast.document.layers[0].surface.snapshot(bounds);
    drop(fast);

    let (mut slow, slow_group) = four_k_editor();
    assert_eq!(slow_group, group);
    let before = slow.recording.group_color(group).expect("the old colour");
    let edit = TapeEdit::Recolor {
        group,
        before,
        after: GREEN,
    };
    let alpha_locked = slow.document.layers[0].alpha_locked;
    let start = Instant::now();
    let original = slow
        .tape_region(0, None, alpha_locked, None)
        .expect("the whole tape replays");
    assert!(similar(
        &original.snapshot(bounds),
        &slow.document.layers[0].surface.snapshot(bounds)
    ));
    drop(original);
    let edited = slow
        .tape_region(0, Some(&edit), alpha_locked, None)
        .expect("the edited tape replays");
    let pixels = edited.snapshot(bounds);
    slow.recording.apply_edit(&edit, true);
    slow.document.layers[0]
        .surface
        .write_pixels(bounds, &pixels);
    let slow_time = start.elapsed();

    let slow_tiles: HashSet<Tile> = slow.document.layers[0]
        .surface
        .writer_groups()
        .flat_map(|(_, written)| written.iter().copied())
        .collect();
    assert_eq!(
        after,
        slow.document.layers[0].surface.snapshot(bounds),
        "the tile replay and the whole-layer replay disagree"
    );
    println!(
        "4K, 200 strokes, recolour one: tile path {} tile(s) in {fast_time:?}; \
         whole layer {} tiles in {slow_time:?}",
        tiles.len(),
        slow_tiles.len()
    );
}
