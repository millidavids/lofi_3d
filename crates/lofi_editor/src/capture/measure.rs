//! `--measure`: time frames with the whole map in view, print, quit.
//!
//! Wall-clock frame time is paced by the compositor on some platforms
//! (Wayland throttles frame callbacks), so GPU pass timings are printed too.

use std::time::Duration;

use bevy::diagnostic::{Diagnostic, DiagnosticsStore};
use bevy::prelude::*;

use crate::document::MapDocument;
use crate::launch::LaunchOptions;

/// Frames to skip while meshes upload and pipelines compile.
const WARM_UP: u32 = 60;
const SAMPLES: usize = 300;
/// Slowest GPU passes to list.
const PASSES_SHOWN: usize = 4;

pub(super) fn wants_measure(options: Res<LaunchOptions>) -> bool {
    options.measure
}

pub(super) fn measure_frames(
    time: Res<Time<Real>>,
    document: Res<MapDocument>,
    diagnostics: Option<Res<DiagnosticsStore>>,
    mut frames: Local<u32>,
    mut samples: Local<Vec<Duration>>,
    mut exit: MessageWriter<AppExit>,
) {
    *frames += 1;
    if *frames <= WARM_UP {
        return;
    }
    samples.push(time.delta());
    if samples.len() < SAMPLES {
        return;
    }

    samples.sort();
    let ms = |d: Duration| d.as_secs_f64() * 1e3;
    let mean = samples.iter().map(|d| ms(*d)).sum::<f64>() / samples.len() as f64;
    let p95 = ms(samples[samples.len() * 95 / 100]);
    let worst = ms(samples[samples.len() - 1]);
    let size = document.terrain().size();
    let triangles = u64::from(size.cells_x()) * u64::from(size.cells_z()) * 2;
    println!(
        "measure: {}x{} chunks ({} draws), {triangles} triangles, \
         mean {mean:.2} ms ({:.0} fps), p95 {p95:.2} ms, worst {worst:.2} ms",
        size.chunks_x(),
        size.chunks_z(),
        size.chunk_count(),
        1000.0 / mean,
    );
    if let Some(store) = diagnostics {
        print_gpu_passes(&store);
    }
    exit.write(AppExit::Success);
}

/// Total GPU time over top-level passes, then the slowest passes.
fn print_gpu_passes(store: &DiagnosticsStore) {
    let mut passes: Vec<(String, usize, f64)> = store
        .iter()
        .filter(|d| d.path().as_str().ends_with("/elapsed_gpu"))
        .filter_map(|d: &Diagnostic| {
            let depth = d.path().components().count();
            Some((d.path().as_str().to_owned(), depth, d.average()?))
        })
        .collect();
    if passes.is_empty() {
        println!("measure: no GPU timings (adapter lacks timestamp queries)");
        return;
    }
    // render/<pass>/elapsed_gpu; deeper paths are nested inside one of these.
    let total: f64 = passes.iter().filter(|p| p.1 == 3).map(|p| p.2).sum();
    passes.sort_by(|a, b| b.2.total_cmp(&a.2));
    let slowest: Vec<String> = passes
        .iter()
        .take(PASSES_SHOWN)
        .map(|(path, _, ms)| {
            let name = path
                .trim_start_matches("render/")
                .trim_end_matches("/elapsed_gpu");
            format!("{name} {ms:.2}")
        })
        .collect();
    println!(
        "measure: gpu {total:.2} ms; slowest: {}",
        slowest.join(", ")
    );
}
