use std::io::Write;

use crate::*;

fn kra_fixture() -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("mimetype", stored).unwrap();
    zip.write_all(b"application/x-krita").unwrap();
    zip.start_file("maindoc.xml", stored).unwrap();
    zip.write_all(
            br#"<DOC xmlns="http://www.calligra.org/DTD/krita" syntaxVersion="2">
                <IMAGE name="Test" mime="application/x-krita" width="2" height="2" colorspacename="RGBA">
                  <layers>
                    <layer name="Paint" filename="layer1" nodetype="paintlayer"
                           opacity="128" visible="1" x="0" y="0"/>
                  </layers>
                </IMAGE>
            </DOC>"#,
        )
        .unwrap();
    zip.start_file("Test/layers/layer1", stored).unwrap();
    let header = b"VERSION 2\nTILEWIDTH 2\nTILEHEIGHT 2\nPIXELSIZE 4\nDATA 1\n";
    zip.write_all(header).unwrap();
    // One stored tile: flag 0, then 4 pixels in B, G, R, A planes.
    let mut plane = vec![0_u8];
    plane.extend_from_slice(&[0, 0, 0, 10]); // blue
    plane.extend_from_slice(&[0, 255, 0, 20]); // green
    plane.extend_from_slice(&[255, 0, 0, 30]); // red
    plane.extend_from_slice(&[255, 255, 128, 255]); // alpha
    zip.write_all(b"0,0,NONE,17\n").unwrap();
    zip.write_all(&plane).unwrap();
    zip.finish().unwrap().into_inner()
}

fn sample_export() -> ExportDocument {
    let mut red = vec![0_u8; 2 * 2 * 4];
    for pixel in red.as_chunks_mut::<4>().0 {
        pixel.copy_from_slice(&[255, 0, 0, 255]);
    }
    let mut ink = vec![0_u8; 2 * 2 * 4];
    ink[0..4].copy_from_slice(&[0, 0, 0, 255]);
    ExportDocument {
        width: 2,
        height: 2,
        layers: vec![
            ExportLayer {
                name: "background".into(),
                visible: true,
                opacity: 1.0,
                rgba: red,
            },
            ExportLayer {
                name: "ink".into(),
                visible: true,
                opacity: 0.5,
                rgba: ink,
            },
        ],
    }
}

#[test]
fn a_krita_file_round_trips() {
    let bytes = write_kra(&sample_export()).unwrap();
    let document = read_kra_bytes(&bytes).unwrap();
    assert_eq!((document.width, document.height), (2, 2));
    assert_eq!(document.layers.len(), 2);
    assert_eq!(document.layers[0].name, "background");
    assert_eq!(document.layers[1].name, "ink");
    assert_eq!(document.layers[1].opacity, 128.0 / 255.0);
    assert_eq!(&document.layers[1].rgba[0..4], &[0, 0, 0, 255]);
}

#[test]
fn a_photoshop_file_round_trips() {
    let bytes = write_psd(&sample_export()).unwrap();
    let document = read_psd_bytes(&bytes).unwrap();
    assert_eq!((document.width, document.height), (2, 2));
    assert_eq!(document.layers.len(), 2);
    assert_eq!(document.layers[0].name, "background");
    assert_eq!(document.layers[1].name, "ink");
    assert_eq!(&document.layers[0].rgba[0..4], &[255, 0, 0, 255]);
}

#[test]
fn a_photoshop_file_keeps_layer_visibility() {
    let document = read_psd_bytes(include_bytes!("../tests/fixtures/visibility.psd")).unwrap();
    assert_eq!((document.width, document.height), (1, 1));
    assert_eq!(document.layers.len(), 2);
    let visible = document
        .layers
        .iter()
        .find(|layer| layer.name == "visible")
        .unwrap();
    let invisible = document
        .layers
        .iter()
        .find(|layer| layer.name == "invisible")
        .unwrap();
    assert!(visible.visible);
    assert!(!invisible.visible);
    assert_eq!(&visible.rgba[0..4], &[0, 0, 0, 255]);
}

#[test]
fn a_photoshop_layer_is_placed_at_its_offset() {
    let document =
        read_psd_bytes(include_bytes!("../tests/fixtures/3x3-opaque-center.psd")).unwrap();
    assert_eq!((document.width, document.height), (3, 3));
    assert_eq!(document.layers.len(), 1);
    let layer = &document.layers[0];
    assert_eq!(layer.name, "OpaqueCenter");
    let painted = layer
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[3] > 0)
        .count();
    assert!(painted > 0, "the layer was flattened away");
}

#[test]
fn a_krita_layer_is_assembled_from_planes() {
    let document = read_kra_bytes(&kra_fixture()).unwrap();
    assert_eq!((document.width, document.height), (2, 2));
    assert_eq!(document.layers.len(), 1);
    let layer = &document.layers[0];
    assert_eq!(layer.name, "Paint");
    assert_eq!(layer.opacity, 128.0 / 255.0);
    assert!(layer.visible);
    assert_eq!(&layer.rgba[0..4], &[255, 0, 0, 255]);
    assert_eq!(&layer.rgba[4..8], &[0, 255, 0, 255]);
    assert_eq!(&layer.rgba[8..12], &[0, 0, 0, 128]);
}

#[test]
fn a_stroke_tape_round_trips() {
    let tape = TapeDocument {
        width: 320,
        height: 240,
        layers: 2,
        color_space: 1,
        dynamic_range: 1,
        layer_info: Vec::new(),
        active: Some(1),
        frames: Vec::new(),
        active_frame: 0,
        fps: 12,
        frame_durations: Vec::new(),
        loop_enabled: true,
        loop_start: 0,
        loop_end: 0,
        brushes: Vec::new(),
        textures: Vec::new(),
        events: vec![
            TapeEvent {
                time_ms: 0,
                group: Some(7),
                command: TapeCommand::Stroke {
                    brush: 3,
                    layer: 0,
                    from: [10.0, 20.0],
                    to: [30.0, 40.0],
                    radius: 6.5,
                    pressure: 0.7,
                    color: [1.0, 0.0, 0.0, 1.0],
                    seed: 0xDEAD_BEEF,
                    tilt: [0.1, -0.2],
                    azimuth: 1.3,
                    roll: 0.4,
                    scale: 1,
                },
            },
            TapeEvent {
                time_ms: 16,
                group: None,
                command: TapeCommand::Erase {
                    brush: 2,
                    layer: 1,
                    from: [30.0, 40.0],
                    to: [40.0, 50.0],
                    radius: 4.0,
                    pressure: 1.0,
                    seed: 1,
                    tilt: [0.0; 2],
                    azimuth: 0.0,
                    roll: 0.0,
                    scale: 0,
                },
            },
            TapeEvent {
                time_ms: 20,
                group: None,
                command: TapeCommand::Fill {
                    layer: 1,
                    x: 3,
                    y: 4,
                    tolerance: 0.12,
                    color: [0.0, 0.0, 1.0, 1.0],
                    close: 2,
                    expand: 3,
                    feather: 4,
                },
            },
            TapeEvent {
                time_ms: 24,
                group: None,
                command: TapeCommand::Clear {
                    layer: 0,
                    color: [1.0, 1.0, 1.0, 1.0],
                },
            },
        ],
    };
    let bytes = write_dkp(&tape).unwrap();
    let read = read_dkp_bytes(&bytes).unwrap();
    assert_eq!((read.width, read.height), (320, 240));
    assert_eq!(read.layers, 2);
    assert_eq!(read.active, Some(1));
    assert_eq!(read.events.len(), 4);
    match &read.events[0].command {
        TapeCommand::Stroke {
            seed,
            tilt,
            color,
            brush,
            scale,
            ..
        } => {
            assert_eq!(*brush, 3);
            assert_eq!(*scale, 1, "the thickness policy did not survive the tape");
            assert_eq!(*seed, 0xDEAD_BEEF);
            assert_eq!(*tilt, [0.1, -0.2]);
            assert_eq!(*color, [1.0, 0.0, 0.0, 1.0]);
        }
        other => panic!("wrong command: {other:?}"),
    }
    assert_eq!(read.events[0].group, Some(7));

    let mut truncated = bytes.clone();
    truncated.truncate(truncated.len() / 2);
    assert!(
        read_dkp_bytes(&truncated).is_err(),
        "truncation was accepted"
    );
}

#[test]
fn a_cached_layer_remembers_where_it_sits() {
    let tape = TapeDocument {
        width: 64,
        height: 64,
        layers: 1,
        color_space: 0,
        dynamic_range: 0,
        active: Some(0),
        layer_info: vec![TapeLayer {
            name: "import".to_string(),
            kind: 0,
            parent: None,
            visible: true,
            opacity: 1.0,
            blend: 0,
            locked: false,
            alpha_locked: false,
            clipping: false,
            expanded: true,
            reference: true,
            color_tag: 3,
            fill: [0.0; 4],
            raster: Some(vec![0x89, b'P', b'N', b'G']),
            raster_rect: Some([16, 32, 8, 8]),
        }],
        events: Vec::new(),
        frames: Vec::new(),
        active_frame: 0,
        fps: 12,
        frame_durations: Vec::new(),
        loop_enabled: true,
        loop_start: 0,
        loop_end: 0,
        brushes: Vec::new(),
        textures: Vec::new(),
    };
    let read = read_dkp_bytes(&write_dkp(&tape).unwrap()).unwrap();
    assert_eq!(read.layer_info[0].raster_rect, Some([16, 32, 8, 8]));
    assert!(read.layer_info[0].reference);
    assert_eq!(read.layer_info[0].color_tag, 3);
    assert_eq!(
        read.layer_info[0].raster.as_deref(),
        Some(&[0x89, b'P', b'N', b'G'][..])
    );
}

#[test]
fn a_smudge_event_round_trips() {
    let tape = TapeDocument {
        width: 32,
        height: 32,
        layers: 1,
        color_space: 0,
        dynamic_range: 0,
        layer_info: Vec::new(),
        active: None,
        events: vec![TapeEvent {
            time_ms: 50,
            group: Some(3),
            command: TapeCommand::Smudge {
                layer: 0,
                from: [4.0, 5.0],
                to: [20.0, 6.0],
                radius: 7.0,
                pressure: 0.8,
                strength: 0.6,
                brush: 2,
            },
        }],
        frames: Vec::new(),
        active_frame: 0,
        fps: 12,
        frame_durations: Vec::new(),
        loop_enabled: true,
        loop_start: 0,
        loop_end: 0,
        brushes: Vec::new(),
        textures: Vec::new(),
    };
    let read = read_dkp_bytes(&write_dkp(&tape).unwrap()).unwrap();
    match &read.events[0].command {
        TapeCommand::Smudge {
            radius,
            strength,
            brush,
            ..
        } => {
            assert_eq!(*radius, 7.0);
            assert_eq!(*strength, 0.6);
            assert_eq!(*brush, 2);
        }
        other => panic!("wrong command: {other:?}"),
    }
}
