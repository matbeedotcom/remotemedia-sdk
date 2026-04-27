//! session-replay — pretty-print and filter pipeline session trace
//! files produced by the SessionRecorder.
//!
//! The recorder writes one JSON record per captured frame. This CLI
//! reads a JSONL file (or stdin with `-`), optionally filters by
//! source / kind / substring, and renders a compact timeline with
//! colour coding. It is a pure observer — no side effects beyond
//! printing.
//!
//! Usage:
//!
//!   session-replay ~/.remotemedia/traces/s1776961697_0.jsonl
//!   session-replay trace.jsonl --source llm.out
//!   session-replay trace.jsonl --kind text --grep "hello"
//!   session-replay trace.jsonl --since-first 0 --until-first 5000
//!
//! Each line:
//!
//!   [+00:00.123] llm.out        text    "Yes, I can hear you perfectly."
//!
//! Offsets are milliseconds relative to the first record in the file.

use clap::Parser;
use console::{style, Style};
use serde::Deserialize;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "session-replay",
    about = "Pretty-print pipeline session traces.",
    version
)]
struct Cli {
    /// Path to the JSONL trace file. Use `-` for stdin.
    path: String,

    /// Only show events whose `source` matches this substring.
    #[arg(long)]
    source: Option<String>,

    /// Only show events whose `kind` equals this string
    /// (`text`/`audio`/`json`/`binary`/`lag`/...).
    #[arg(long)]
    kind: Option<String>,

    /// Only show events whose payload stringification contains this
    /// substring (case-insensitive). Useful for "find where `hello`
    /// first appears in the pipeline".
    #[arg(long)]
    grep: Option<String>,

    /// Skip events earlier than this offset (ms since first record).
    #[arg(long)]
    since_first: Option<i64>,

    /// Skip events later than this offset (ms since first record).
    #[arg(long)]
    until_first: Option<i64>,

    /// Show full payloads (otherwise text payloads are truncated at
    /// 120 chars).
    #[arg(long)]
    full: bool,

    /// Summarise at the end: count events per (source, kind).
    #[arg(long)]
    summary: bool,
}

#[derive(Debug, Deserialize)]
struct TraceEvent {
    ts_ms: u64,
    #[serde(default)]
    session_id: String,
    source: String,
    kind: String,
    payload: serde_json::Value,
}

fn open_reader(path: &str) -> anyhow::Result<Box<dyn Read>> {
    if path == "-" {
        Ok(Box::new(std::io::stdin().lock()))
    } else {
        Ok(Box::new(File::open(PathBuf::from(path))?))
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let reader = BufReader::new(open_reader(&cli.path)?);

    let grep_needle = cli.grep.as_deref().map(str::to_ascii_lowercase);

    let mut first_ts: Option<u64> = None;
    let mut totals: std::collections::BTreeMap<(String, String), u64> =
        std::collections::BTreeMap::new();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("read error: {e}");
                continue;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let ev: TraceEvent = match serde_json::from_str(&line) {
            Ok(ev) => ev,
            Err(e) => {
                eprintln!("skipping malformed line: {e}");
                continue;
            }
        };

        let first = *first_ts.get_or_insert(ev.ts_ms);
        let offset_ms = ev.ts_ms as i64 - first as i64;

        if let Some(lo) = cli.since_first {
            if offset_ms < lo {
                continue;
            }
        }
        if let Some(hi) = cli.until_first {
            if offset_ms > hi {
                continue;
            }
        }
        if let Some(src) = &cli.source {
            if !ev.source.contains(src) {
                continue;
            }
        }
        if let Some(k) = &cli.kind {
            if ev.kind != *k {
                continue;
            }
        }

        let payload_str = render_payload(&ev, cli.full);
        if let Some(needle) = &grep_needle {
            if !payload_str.to_ascii_lowercase().contains(needle)
                && !ev.source.to_ascii_lowercase().contains(needle)
            {
                continue;
            }
        }

        println!(
            "[{}] {:<18} {:<7} {}",
            style(format_offset(offset_ms)).dim(),
            source_style(&ev.source).apply_to(&ev.source),
            kind_style(&ev.kind).apply_to(&ev.kind),
            payload_str,
        );

        *totals
            .entry((ev.source.clone(), ev.kind.clone()))
            .or_insert(0) += 1;
    }

    if cli.summary {
        println!();
        println!("{}", style("── summary ────────────────────").dim());
        for ((source, kind), count) in &totals {
            println!("  {:<24} {:<8} {}", source, kind, count);
        }
    }

    Ok(())
}

fn format_offset(offset_ms: i64) -> String {
    let sign = if offset_ms < 0 { "-" } else { "+" };
    let ms = offset_ms.unsigned_abs();
    let secs = ms / 1000;
    let mins = secs / 60;
    let s = secs % 60;
    let m = mins % 60;
    format!("{}{:02}:{:02}.{:03}", sign, m, s, ms % 1000)
}

fn render_payload(ev: &TraceEvent, full: bool) -> String {
    let raw = match ev.kind.as_str() {
        "text" => match &ev.payload {
            serde_json::Value::String(s) => format!("{:?}", s),
            other => other.to_string(),
        },
        "audio" => {
            // {"samples":…,"sample_rate":…,"channels":…,"duration_ms":…}
            let samples = ev.payload.get("samples").and_then(|v| v.as_u64());
            let sr = ev.payload.get("sample_rate").and_then(|v| v.as_u64());
            let ch = ev.payload.get("channels").and_then(|v| v.as_u64());
            let dur = ev.payload.get("duration_ms").and_then(|v| v.as_u64());
            format!(
                "samples={} sr={} ch={} dur={}ms",
                samples.map(|v| v.to_string()).unwrap_or("-".into()),
                sr.map(|v| v.to_string()).unwrap_or("-".into()),
                ch.map(|v| v.to_string()).unwrap_or("-".into()),
                dur.map(|v| v.to_string()).unwrap_or("-".into()),
            )
        }
        _ => ev.payload.to_string(),
    };
    if full {
        raw
    } else if raw.chars().count() > 120 {
        let truncated: String = raw.chars().take(117).collect();
        format!("{}…", truncated)
    } else {
        raw
    }
}

fn source_style(source: &str) -> Style {
    if source.starts_with("vad") {
        Style::new().magenta()
    } else if source.starts_with("stt") {
        Style::new().cyan()
    } else if source.starts_with("llm") {
        Style::new().green()
    } else if source.starts_with("coordinator") {
        Style::new().yellow()
    } else if source.contains("tts") || source == "audio.out" {
        Style::new().blue()
    } else {
        Style::new().white()
    }
}

fn kind_style(kind: &str) -> Style {
    match kind {
        "text" => Style::new().green(),
        "json" => Style::new().yellow(),
        "audio" => Style::new().blue(),
        "lag" => Style::new().red().bold(),
        _ => Style::new().dim(),
    }
}
