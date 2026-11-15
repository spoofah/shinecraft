//! SHINECRAFT as an EffectCraft external effect helper (docs/external-helpers.md, protocol 1):
//! the same effect, id and parameters as the WebAssembly plug-in, rendered natively on every core.
//! EffectCraft starts it as `effectcraft-helper-shinecraft.exe --effectcraft-helper` and talks JSON
//! lines over stdin/stdout; pixels stay in the frame exchange file, which is mapped and rendered
//! in place.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, Write};

use serde_json::{Value, json};
use shinecraft::{MANIFEST, NPARAMS, Params, Px, padding, shine};

fn send(out: &mut impl Write, v: Value) {
    // A broken stdout means EffectCraft is gone; the next read ends the loop.
    let _ = writeln!(out, "@ec-helper {v}").and_then(|()| out.flush());
}

fn main() {
    if std::env::args().nth(1).as_deref() != Some("--effectcraft-helper") {
        eprintln!("This is an EffectCraft effect helper: put it in EffectCraft's helpers folder and use Effect > Scan for Effect Helpers.");
        std::process::exit(2);
    }
    let manifest: Value = serde_json::from_str(MANIFEST).expect("the built-in manifest is valid JSON");
    let id = manifest["id"].as_str().unwrap_or_default().to_string();
    let mut frames: Option<File> = None;
    // The exchange file mapped once (and again when EffectCraft grows it): mapping it per render
    // cost a soft page fault per 4 KB of frame, ~3 ms at 1080p. EffectCraft only writes at offsets
    // and never shrinks the file, and Windows keeps a mapped view coherent with its file I/O.
    let mut view: Option<memmap2::MmapMut> = None;
    let mut out = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let Ok(req) = serde_json::from_str::<Value>(&line) else {
            eprintln!("unreadable request: {line:.200}");
            continue;
        };
        match req["op"].as_str().unwrap_or("") {
            "hello" => {
                view = None;
                frames = req["frames"].as_str().and_then(|p| OpenOptions::new().read(true).write(true).open(p).map_err(|e| eprintln!("frame exchange {p}: {e}")).ok());
                send(&mut out, json!({"op": "hello", "version": 1, "name": "SHINECRAFT", "helper_version": env!("CARGO_PKG_VERSION"), "loads": []}));
            }
            // time_varying: shimmer and fractal evolution change with time (as `ec_flags` says for the WASM).
            "describe" => send(&mut out, json!({"op": "effects", "effects": [{"manifest": manifest, "padding": true, "time_varying": true}], "errors": []})),
            "render" => match render(&req, &id, frames.as_ref(), &mut view) {
                Ok(()) => send(&mut out, json!({"op": "done"})),
                Err(e) => send(&mut out, json!({"op": "error", "message": e})),
            },
            "padding" => {
                let (flat, layer) = (flat(&req), layer_size(&req));
                let pixels = if flat.len() < NPARAMS { 0 } else { padding(&Params::from_flat(&flat), req["scale"].as_f64().unwrap_or(1.0), layer) };
                send(&mut out, json!({"op": "padding", "pixels": pixels}));
            }
            "load" => send(&mut out, json!({"op": "error", "message": "SHINECRAFT loads no files"})),
            "ping" => send(&mut out, json!({"op": "pong"})),
            "destroy" => {}
            "shutdown" => break,
            op => send(&mut out, json!({"op": "error", "message": format!("unknown request `{op}`")})),
        }
    }
}

/// Renders the frame the request points at, in place in the exchange file.
fn render(req: &Value, id: &str, frames: Option<&File>, view: &mut Option<memmap2::MmapMut>) -> Result<(), String> {
    if req["effect"].as_str() != Some(id) {
        return Err(format!("unknown effect {}", req["effect"]));
    }
    let file = frames.ok_or("the frame exchange file isn't open")?;
    let f = &req["frame"];
    let num = |v: &Value| v.as_f64().ok_or_else(|| format!("bad frame ref {f}"));
    let (offset, w, h) = (num(&f["offset"])? as u64, num(&f["width"])? as usize, num(&f["height"])? as usize);
    let scale = f["scale"].as_f64().unwrap_or(1.0);
    let flat = flat(req);
    if flat.len() < NPARAMS {
        return Err(format!("expected {NPARAMS} parameters, got {}", flat.len()));
    }
    let n = w.checked_mul(h).filter(|&n| n > 0).ok_or("empty frame")?;
    let bytes = n.checked_mul(size_of::<Px>()).ok_or("frame too large")?;
    let end = usize::try_from(offset).ok().and_then(|o| o.checked_add(bytes)).ok_or("frame offset too large")?;
    if view.as_ref().is_none_or(|m| m.len() < end) {
        let len = file.metadata().map_err(|e| format!("frame exchange: {e}"))?.len();
        if (len as usize) < end {
            return Err(format!("frame ends at {end} but the exchange file has {len} bytes"));
        }
        *view = None;
        // SAFETY: EffectCraft doesn't touch the frame while it waits for this render's reply,
        // and never shrinks the file.
        *view = Some(unsafe { memmap2::MmapOptions::new().len(len as usize).map_mut(file) }.map_err(|e| format!("map frames: {e}"))?);
    }
    let map = view.as_mut().ok_or("no frame mapping")?;
    let frame = &mut map[end - bytes..end];
    if frame.as_ptr().align_offset(align_of::<Px>()) != 0 {
        return Err("frame isn't 4-byte aligned in the exchange file".into());
    }
    // SAFETY: n pixels of 16 bytes are exactly `frame`, which is aligned for f32 (checked above).
    let px = unsafe { std::slice::from_raw_parts_mut(frame.as_mut_ptr().cast::<Px>(), n) };
    let mut p = Params::from_flat(&flat);
    let layer = layer_size(req);
    // Padded (Extend Past Layer): keep the working size of the layer itself.
    if padding(&p, scale, layer) > 0 {
        p.layer_long = (layer[0].max(layer[1]) * scale) as f32;
    }
    shine(px, w, h, &p, req["time"].as_f64().unwrap_or(0.0) as f32, scale as f32);
    Ok(())
}

fn flat(req: &Value) -> Vec<f64> {
    req["flat"].as_array().map(|a| a.iter().map(|v| v.as_f64().unwrap_or(0.0)).collect()).unwrap_or_default()
}

fn layer_size(req: &Value) -> [f64; 2] {
    let s = &req["layer_size"];
    [s[0].as_f64().unwrap_or(0.0), s[1].as_f64().unwrap_or(0.0)]
}
