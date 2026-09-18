//! `--bench-internal <trace.json>`: replays a josephg/editing-traces file
//! directly against a `SharQueue` in-process -- no socket, no serialization,
//! no separate process -- to isolate pure CRDT/queue cost from transport
//! cost. Compare against `bench/trace-runner.js`'s numbers for the same
//! trace to see what the socket layer actually costs.
//!
//! Traces ship gzipped; decompress first (e.g. `gunzip -k trace.json.gz`)
//! since this only reads plain JSON.

use crate::shar::core::queue::SharQueue;
use crate::shar::core::tree::ring_search_diagnostics;
use crate::shar::prelude::{IdSize, PeerIdSize};
use crate::types::{IdeAdd, IdeAddConfirmed, Remove};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Deserialize)]
struct Trace {
    txns: Vec<Txn>,
    #[serde(rename = "endContent")]
    end_content: String,
}

#[derive(Deserialize)]
struct Txn {
    patches: Vec<(usize, usize, String)>,
}

/// One character's identity, mirroring the client-side `docState` model in
/// `extension.js`/`bench/doc-mirror.js` -- except confirmation is always
/// synchronous here (no network round-trip), so a cell is resolved to its
/// real `(id, peer)` the instant it's inserted, and `parent_tag` never
/// actually gets exercised: there's never an unconfirmed parent to
/// reference by tag when nothing is ever in flight.
struct Cell {
    id: IdSize,
    peer: PeerIdSize,
    value: char,
}

struct Line {
    anchor_id: IdSize,
    anchor_peer: PeerIdSize,
    cells: Vec<Cell>,
}

/// Same offset -> (row, col0) math as `doc-mirror.js`'s `locate`.
fn locate(lines: &[Line], offset: usize) -> (usize, usize) {
    let mut acc = 0;
    for (row, line) in lines.iter().enumerate() {
        if offset <= acc + line.cells.len() {
            return (row, offset - acc);
        }
        acc += line.cells.len() + 1;
    }
    panic!("offset {offset} out of range (doc length {acc})");
}

fn current_text(lines: &[Line]) -> String {
    lines
        .iter()
        .map(|line| line.cells.iter().map(|c| c.value).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

fn percentile(sorted: &[f64], p: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let idx = ((p * sorted.len() as f64).floor() as usize).min(sorted.len() - 1);
    Some(sorted[idx])
}

fn read_rss_kb() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            return rest.trim().split_whitespace().next()?.parse().ok();
        }
    }
    None
}

async fn insert_char(
    queue: &mut SharQueue,
    lines: &mut Vec<Line>,
    file_path: &Path,
    offset: usize,
    value: char,
    next_tag: &mut u32,
    confirmed: &Arc<Mutex<Option<(IdSize, PeerIdSize)>>>,
) -> crate::shar::prelude::Result<()> {
    let (row, col0) = locate(lines, offset);
    let (parent_id, parent_peer) = if col0 == 0 {
        (lines[row].anchor_id, lines[row].anchor_peer)
    } else {
        let c = &lines[row].cells[col0 - 1];
        (c.id, c.peer)
    };

    let tag = *next_tag;
    *next_tag += 1;

    let op = IdeAdd::new(
        file_path.to_path_buf(),
        Some(parent_id),
        Some(parent_peer),
        None,
        value,
        tag,
        row,
    );
    queue.add_ide_operation(op).await?;

    let (id, peer) = confirmed
        .lock()
        .unwrap()
        .take()
        .expect("add_ide_operation always confirms before returning");

    if value == '\n' {
        let moved: Vec<Cell> = lines[row].cells.split_off(col0);
        lines.insert(
            row + 1,
            Line {
                anchor_id: id,
                anchor_peer: peer,
                cells: moved,
            },
        );
    } else {
        lines[row].cells.insert(col0, Cell { id, peer, value });
    }

    Ok(())
}

async fn remove_char(queue: &mut SharQueue, lines: &mut Vec<Line>, file_path: &Path, offset: usize) {
    let (row, col0) = locate(lines, offset);

    if col0 < lines[row].cells.len() {
        let cell = lines[row].cells.remove(col0);
        queue
            .remove_ide_operation(Remove::new(file_path.to_path_buf(), cell.id, cell.peer, row))
            .await;
    } else {
        let next = lines.remove(row + 1);
        lines[row].cells.extend(next.cells);
        queue
            .remove_ide_operation(Remove::new(
                file_path.to_path_buf(),
                next.anchor_id,
                next.anchor_peer,
                row + 1,
            ))
            .await;
    }
}

pub async fn run(trace_path: &Path, label: &str) -> Result<(), Box<dyn std::error::Error>> {
    let raw = std::fs::read_to_string(trace_path)?;
    let trace: Trace = serde_json::from_str(&raw)?;

    let total_inserts: usize = trace
        .txns
        .iter()
        .flat_map(|t| &t.patches)
        .map(|(_, _, inserted)| inserted.chars().count())
        .sum();
    let total_deletes: usize = trace.txns.iter().flat_map(|t| &t.patches).map(|(_, del, _)| *del).sum();
    let total_ops = total_inserts + total_deletes;

    println!(
        "[{label}] loaded: {total_inserts} char inserts, {total_deletes} char deletes ({total_ops} total char ops), endContent length {}",
        trace.end_content.chars().count()
    );

    let scratch_dir: PathBuf = std::env::temp_dir().join(format!("shario-inproc-{}", std::process::id()));
    std::fs::create_dir_all(&scratch_dir)?;
    let file_path = scratch_dir.join("doc.txt");
    std::fs::write(&file_path, "")?;

    let confirmed: Arc<Mutex<Option<(IdSize, PeerIdSize)>>> = Arc::new(Mutex::new(None));
    let confirmed_for_callback = confirmed.clone();

    let mut queue = SharQueue::new(
        scratch_dir.clone(),
        0,
        Box::new(|_row, _col| Box::pin(async {})),
        Box::new(|_row, _col| Box::pin(async {})),
        Box::new(move |op_return: IdeAddConfirmed| {
            let confirmed = confirmed_for_callback.clone();
            Box::pin(async move {
                *confirmed.lock().unwrap() = Some((op_return.id, op_return.peer));
            })
        }),
        Box::new(|_op| Box::pin(async {})),
        Box::new(|_op| Box::pin(async {})),
    )?;

    let mut lines = vec![Line {
        anchor_id: 0,
        anchor_peer: 0,
        cells: Vec::new(),
    }];
    let mut next_tag: u32 = 0;

    let idle_rss_kb = read_rss_kb();
    println!("[{label}] playing trace...");
    let start = std::time::Instant::now();
    let mut latencies_ms: Vec<f64> = Vec::with_capacity(total_ops);

    for txn in &trace.txns {
        for (pos, del_count, inserted) in &txn.patches {
            for _ in 0..*del_count {
                let op_start = std::time::Instant::now();
                remove_char(&mut queue, &mut lines, &file_path, *pos).await;
                latencies_ms.push(op_start.elapsed().as_secs_f64() * 1000.0);
            }
            for (i, ch) in inserted.chars().enumerate() {
                let op_start = std::time::Instant::now();
                insert_char(&mut queue, &mut lines, &file_path, pos + i, ch, &mut next_tag, &confirmed).await?;
                latencies_ms.push(op_start.elapsed().as_secs_f64() * 1000.0);
            }
        }
    }

    let total_ms = start.elapsed().as_secs_f64() * 1000.0;
    let peak_rss_kb = read_rss_kb();

    let actual_text = current_text(&lines);
    let matches = actual_text == trace.end_content;

    latencies_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean_latency_ms = latencies_ms.iter().sum::<f64>() / (latencies_ms.len().max(1) as f64);

    let result = serde_json::json!({
        "label": label,
        "totalCharOps": total_ops,
        "totalCharInserts": total_inserts,
        "totalCharDeletes": total_deletes,
        "totalMs": total_ms,
        "opsPerSec": (total_ops as f64) / (total_ms / 1000.0),
        "idleRssKb": idle_rss_kb,
        "peakRssKb": peak_rss_kb,
        "peakRssGrowthKb": idle_rss_kb.zip(peak_rss_kb).map(|(idle, peak)| peak.saturating_sub(idle)),
        "ringSearch": {
            "calls": ring_search_diagnostics().0,
            "totalDistance": ring_search_diagnostics().1,
            "maxDistance": ring_search_diagnostics().2,
        },
        "latencyMs": {
            "mean": mean_latency_ms,
            "min": latencies_ms.first(),
            "p50": percentile(&latencies_ms, 0.5),
            "p95": percentile(&latencies_ms, 0.95),
            "p99": percentile(&latencies_ms, 0.99),
            "max": latencies_ms.last(),
        },
        "correctnessCheck": if matches { "PASS" } else { "FAIL" },
    });

    println!("\n[{label}] RESULT");
    println!("{}", serde_json::to_string_pretty(&result)?);

    if !matches {
        eprintln!(
            "[{label}] MISMATCH: expected {} chars, got {} chars",
            trace.end_content.chars().count(),
            actual_text.chars().count()
        );
    }

    std::fs::remove_dir_all(&scratch_dir).ok();

    Ok(())
}
