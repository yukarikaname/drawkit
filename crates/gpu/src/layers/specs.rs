//! Structural edits to the `LayerSpec` list.
//!
//! A spec refers to other layers by position: `parent` names a group and `clip` names the layer
//! whose alpha clips it. Inserting, removing or moving a layer therefore has to shift those
//! references with it, and every spec's `layer` field has to follow its new position.

use super::LayerSpec;

/// Where a layer at `position` ends up after `from` is moved to `to`.
pub(crate) fn moved(position: usize, from: usize, to: usize) -> usize {
    if position == from {
        to
    } else if from < to && position > from && position <= to {
        position - 1
    } else if to < from && position >= to && position < from {
        position + 1
    } else {
        position
    }
}

fn renumber(specs: &mut [LayerSpec]) {
    for (index, spec) in specs.iter_mut().enumerate() {
        spec.layer = index;
    }
}

/// Whether another spec uses `index` as its group or its clip base.
pub(crate) fn referenced(specs: &[LayerSpec], index: usize) -> bool {
    specs
        .iter()
        .any(|spec| spec.parent == Some(index) || spec.clip == Some(index))
}

/// Insert the spec for a new layer at `index`.
pub(crate) fn insert(specs: &mut Vec<LayerSpec>, index: usize) {
    for spec in specs.iter_mut() {
        spec.parent = spec
            .parent
            .map(|parent| parent + usize::from(parent >= index));
        spec.clip = spec.clip.map(|clip| clip + usize::from(clip >= index));
    }
    specs.insert(index, LayerSpec::new(index));
    renumber(specs);
}

/// Remove the spec at `index`. The caller refuses when `referenced` says another spec needs it.
pub(crate) fn remove(specs: &mut Vec<LayerSpec>, index: usize) {
    specs.remove(index);
    let down = |reference: usize| {
        if reference > index {
            reference - 1
        } else {
            reference
        }
    };
    for spec in specs.iter_mut() {
        spec.parent = spec.parent.map(down);
        spec.clip = spec.clip.map(down);
    }
    renumber(specs);
}

/// Move the spec at `from` to `to`, carrying its group and clip references along.
pub(crate) fn move_layer(specs: &mut Vec<LayerSpec>, from: usize, to: usize) {
    if from >= specs.len() || to >= specs.len() || from == to {
        return;
    }
    let spec = specs.remove(from);
    specs.insert(to, spec);
    for spec in specs.iter_mut() {
        spec.parent = spec.parent.map(|parent| moved(parent, from, to));
        spec.clip = spec.clip.map(|clip| moved(clip, from, to));
    }
    renumber(specs);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(layer: usize) -> LayerSpec {
        LayerSpec::new(layer)
    }

    #[test]
    fn insert_shifts_group_and_clip_references() {
        let mut specs = vec![spec(0), spec(1).parent(0), spec(2).clip(1)];
        insert(&mut specs, 1);
        assert_eq!(specs.len(), 4);
        assert_eq!(specs[1].layer, 1);
        assert_eq!(specs[2].layer, 2);
        assert_eq!(specs[2].parent, Some(0));
        assert_eq!(specs[3].layer, 3);
        assert_eq!(specs[3].clip, Some(2));
    }

    #[test]
    fn a_referenced_layer_is_reported() {
        let specs = vec![spec(0), spec(1).clip(0), spec(2).parent(0)];
        assert!(referenced(&specs, 0));
        assert!(!referenced(&specs, 1));
    }

    #[test]
    fn remove_shifts_the_layers_above_down() {
        let mut specs = vec![
            spec(0).clip(2),
            spec(1),
            spec(2),
            spec(3).parent(4),
            spec(4),
        ];
        remove(&mut specs, 1);
        assert_eq!(specs.len(), 4);
        assert_eq!(specs[0].clip, Some(1));
        assert_eq!(specs[2].layer, 2);
        assert_eq!(specs[2].parent, Some(3));
    }

    #[test]
    fn moving_a_group_carries_its_children() {
        let mut specs = vec![spec(0), spec(1).parent(0), spec(2), spec(3)];
        move_layer(&mut specs, 0, 2);
        assert_eq!(specs.len(), 4);
        let positions: Vec<usize> = specs.iter().map(|spec| spec.layer).collect();
        assert_eq!(positions, vec![0, 1, 2, 3]);
        let child = specs
            .iter()
            .find(|spec| spec.parent == Some(2))
            .expect("the child still points at the group");
        assert_eq!(child.layer, 0);
        assert_eq!(specs[2].layer, 2);
    }

    #[test]
    fn the_move_mapping_is_a_permutation() {
        for from in 0..5 {
            for to in 0..5 {
                let mut seen: Vec<usize> = (0..5).map(|p| moved(p, from, to)).collect();
                seen.sort_unstable();
                assert_eq!(seen, vec![0, 1, 2, 3, 4], "from {from} to {to}");
            }
        }
    }
}
