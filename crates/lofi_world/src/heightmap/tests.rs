use std::path::PathBuf;

use super::*;
use crate::grid::GridSize;
use crate::materials::MaterialId;
use crate::terrain::Terrain;

fn dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lofi_heightmap_{name}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn hilly() -> Terrain {
    let size = GridSize::new(1, 1).expect("valid size");
    let mut t = Terrain::flat(size, 2.0, 0.0, MaterialId(0));
    for (x, z) in size.full_rect().iter() {
        t.set_height(x, z, (x as f32 * 0.7).sin() * 12.0 + z as f32 * 0.25 - 3.0);
    }
    t
}

#[test]
fn every_format_round_trips() {
    let t = hilly();
    let (low, high) = t.height_range();
    let step = (high - low) / 65535.0;
    let d = dir("round_trip");
    for format in HeightmapFormat::ALL {
        let path = d.join(format!("h.{}", format.extension()));
        let info = export_heightmap(&t, format, &path).expect("exports");
        let (map, side) = read_heightmap(&path).expect("reads");
        assert_eq!(side, Some(info));
        assert_eq!((map.width, map.height), (33, 33));
        let metres = map.into_metres(info.min, info.max);
        let worst = metres
            .samples
            .iter()
            .zip(t.heights_row_major())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max);
        if format == HeightmapFormat::R32 {
            assert_eq!(worst, 0.0, "R32 is exact");
        } else {
            assert!(worst <= step, "{format:?} off by {worst}");
        }
    }
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn raw_without_a_sidecar_must_be_square() {
    let d = dir("square");
    let square = d.join("s.r16");
    std::fs::write(&square, vec![0u8; 9 * 9 * 2]).expect("write");
    let (map, info) = read_heightmap(&square).expect("square reads");
    assert_eq!((map.width, map.height, info), (9, 9, None));

    let odd = d.join("o.r16");
    std::fs::write(&odd, vec![0u8; 9 * 10 * 2]).expect("write");
    assert!(read_heightmap(&odd).is_err());
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn eight_bit_png_reads_and_colour_is_refused() {
    let d = dir("png8");
    let write = |name: &str, color, data: &[u8]| {
        let path = d.join(name);
        let mut out = Vec::new();
        let mut e = png::Encoder::new(&mut out, 2, 2);
        e.set_color(color);
        e.set_depth(png::BitDepth::Eight);
        let mut w = e.write_header().expect("header");
        w.write_image_data(data).expect("data");
        w.finish().expect("finish");
        std::fs::write(&path, out).expect("write");
        path
    };
    let grey = write("g.png", png::ColorType::Grayscale, &[0, 255, 51, 102]);
    let (map, _) = read_heightmap(&grey).expect("reads");
    assert_eq!(map.samples, vec![0.0, 1.0, 0.2, 0.4]);
    let rgb = write("c.png", png::ColorType::Rgb, &[0; 12]);
    assert!(matches!(
        read_heightmap(&rgb),
        Err(HeightmapError::Invalid(_))
    ));
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn damaged_files_error_without_panicking() {
    let t = hilly();
    let d = dir("damaged");
    for format in HeightmapFormat::ALL {
        let path = d.join(format!("h.{}", format.extension()));
        export_heightmap(&t, format, &path).expect("exports");
        let bytes = std::fs::read(&path).expect("read back");
        for cut in [0, 1, 7, bytes.len() / 2] {
            std::fs::write(&path, &bytes[..cut]).expect("write");
            assert!(read_heightmap(&path).is_err(), "{format:?} cut to {cut}");
        }
        // A PNG missing only its end marker still holds the whole image;
        // reading it either way is fine, panicking isn't.
        std::fs::write(&path, &bytes[..bytes.len() - 1]).expect("write");
        let _ = read_heightmap(&path);
    }
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn oversized_sidecar_is_refused_before_allocating() {
    let d = dir("huge");
    let path = d.join("h.r16");
    std::fs::write(&path, [0u8; 8]).expect("write");
    let info = HeightmapInfo {
        format: HeightmapFormat::R16,
        width: 100_000,
        height: 100_000,
        min: 0.0,
        max: 1.0,
        cell_size: 2.0,
    };
    std::fs::write(
        sidecar_path(&path),
        serde_json::to_vec(&info).expect("json"),
    )
    .expect("write");
    assert!(read_heightmap(&path).is_err());
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_png_without_its_sidecar_still_knows_its_range() {
    let t = hilly();
    let d = dir("png_range");
    let path = d.join("h.png");
    let info = export_heightmap(&t, HeightmapFormat::Png16, &path).expect("exports");
    std::fs::remove_file(sidecar_path(&path)).expect("remove sidecar");
    let (_, found) = read_heightmap(&path).expect("reads");
    assert_eq!(found, Some(info));
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn export_replaces_a_stale_sidecar() {
    let t = hilly();
    let d = dir("stale");
    let path = d.join("h.r16");
    std::fs::write(sidecar_path(&path), b"{\"stale\": true}").expect("write");
    let info = export_heightmap(&t, HeightmapFormat::R16, &path).expect("exports");
    assert_eq!(read_heightmap(&path).expect("reads").1, Some(info));
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn a_sidecar_that_disagrees_with_the_file_is_refused() {
    let t = hilly();
    let d = dir("mismatch");
    let path = d.join("h.r16");
    let mut info = export_heightmap(&t, HeightmapFormat::R16, &path).expect("exports");
    info.width += 1;
    std::fs::write(
        sidecar_path(&path),
        serde_json::to_vec(&info).expect("json"),
    )
    .expect("write");
    assert!(read_heightmap(&path).is_err());
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn shrinking_averages_instead_of_aliasing() {
    // A one-sample checkerboard: bilinear at 4× down would pick lone
    // samples; averaging gives the mid value.
    let side = 33;
    let checker = Heightmap {
        width: side,
        height: side,
        samples: (0..side * side)
            .map(|i| ((i % side + i / side) % 2) as f32)
            .collect(),
        units: Units::Metres,
    };
    let small = checker.resample(9, 9);
    for &s in &small.samples[10..70] {
        assert!((s - 0.5).abs() < 0.15, "{s}");
    }
}

#[test]
fn resampling_never_reads_past_the_edge() {
    // Sizes where the last sample's position rounded past the end in f32.
    for (from, to) in [(450, 225), (4096, 865), (4096, 1505), (3, 2049), (2049, 2)] {
        let line = Heightmap {
            width: from,
            height: 2,
            samples: (0..from * 2).map(|i| (i % from) as f32).collect(),
            units: Units::Metres,
        };
        let out = line.resample(to, 3);
        assert_eq!(out.samples.len(), (to * 3) as usize);
        assert_eq!(out.at(0, 1), 0.0, "{from} -> {to}");
        assert!(
            (out.at(to - 1, 1) - (from - 1) as f32).abs() < 1e-3,
            "{from} -> {to}"
        );
    }
}

#[test]
fn each_axis_is_filtered_for_its_own_scale() {
    // Shrinks along x, grows along z: z must stay smooth, not terraced.
    let ramp = Heightmap {
        width: 129,
        height: 9,
        samples: (0..129 * 9).map(|i| (i / 129) as f32).collect(),
        units: Units::Metres,
    };
    let out = ramp.resample(65, 33);
    let column: Vec<f32> = (0..33).map(|z| out.at(10, z)).collect();
    for (z, v) in column.iter().enumerate() {
        assert!((v - z as f32 * 0.25).abs() < 1e-4, "{column:?}");
    }
}

#[test]
fn flipping_reverses_rows() {
    let m = Heightmap {
        width: 2,
        height: 3,
        samples: vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0],
        units: Units::Metres,
    };
    assert_eq!(m.flipped().samples, vec![4.0, 5.0, 2.0, 3.0, 0.0, 1.0]);
}

#[test]
fn resampling_keeps_a_ramp_straight() {
    let ramp = Heightmap {
        width: 5,
        height: 2,
        samples: vec![0.0, 1.0, 2.0, 3.0, 4.0, 0.0, 1.0, 2.0, 3.0, 4.0],
        units: Units::Metres,
    };
    let wide = ramp.resample(9, 3);
    for x in 0..9 {
        assert!((wide.at(x, 1) - x as f32 * 0.5).abs() < 1e-5);
    }
    let cut = ramp.crop_or_pad(7, 1);
    assert_eq!(cut.samples, vec![0.0, 1.0, 2.0, 3.0, 4.0, 4.0, 4.0]);
}
