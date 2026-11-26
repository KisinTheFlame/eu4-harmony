//! Keep province pixel shape updates (`0xa402a0`) O(1) across world resets
//! (docs/game/program/load-preparation.md, "Map pixels and retained province shapes").
//!
//! `0x87d140` resets runtime provinces and re-runs `0x888e60` over every pixel
//! of `provinces.bmp` without first clearing the static map entries' shapes
//! (`map+0x6c0`, stride `0x230`, shape at `+0x130`). `0xa402a0` caches the last
//! span at `shape+0x18`, but its fast path only recognizes extending that span
//! to the right (`x == span.x + span.len`), and its existing-point branch at
//! `0xa40368` leaves `last_span` null. On the second and every later reset,
//! every pixel already belongs to an existing span, so all 11.5 million pixel
//! updates fall back to a full linear scan of the province's span vector.
//!
//! This replacement keeps `last_span` on existing-point hits, skips contiguous
//! prefixes already covered by `last_span` in O(1), and extends newly created
//! or already-cached right-extending spans across contiguous horizontal runs in
//! one step, producing the exact same span vector bytes as the game's function.
use crate::base::called_by_game;
use crate::native::points::{Function, Point};
use crate::native::speed_up::SpeedUp;

/// `(shape, points, count)`: add `count` `(x, y)` i16 pixels to a province shape.
const ADD_PIXELS: usize = 0xa402a0;
/// `(shape, at, &span)`: grow and append a 6-byte span to the shape's vector.
const PUSH_SPAN: usize = 0xa445c0;

type AddPixels = unsafe extern "C" fn(usize, *const Point2, i32);
type PushSpan = unsafe extern "C" fn(usize, usize, *const Span);

static ORIGINAL_ADD_PIXELS: Function<AddPixels> = Function::new();
static PUSH_SPAN_FN: Function<PushSpan> = Function::new();

pub const SPEED_UP: SpeedUp = SpeedUp::new("province-shapes", "loading", points, false);

fn points() -> Vec<Point> {
    vec![
        Point::replace(
            ADD_PIXELS,
            "45 85 c0 0f 8e 6c 01 00 00 53 56 41 55 48 83 ec 30",
            "add pixels to a province shape",
            add_pixels as AddPixels,
            &ORIGINAL_ADD_PIXELS,
        ),
        Point::keep(PUSH_SPAN, "40 53 55 57 41 55 41 56 48 83 ec 20 4c 8b 09 48 8b da", "province shape span vector grow").recorded(PUSH_SPAN_FN.slot()),
        Point::keep(0x889088usize, "e8 13 72 1b 00", "map pixel scan updates a run's province shape"),
        Point::keep(0x88960ausize, "e8 91 6c 1b 00", "map pixel scan updates a row-end run's province shape"),
    ]
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Point2 {
    x: i16,
    y: i16,
}

#[repr(C, packed)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Span {
    x: i16,
    y: i16,
    len: i16,
}

const SPAN_SIZE: usize = std::mem::size_of::<Span>();

#[inline]
unsafe fn read_usize(addr: usize) -> usize {
    (addr as *const usize).read()
}

#[inline]
unsafe fn write_usize(addr: usize, val: usize) {
    (addr as *mut usize).write(val);
}

#[inline]
fn contiguous_run_len(pts: &[Point2], start: usize, max_run: i32) -> i32 {
    let first = pts[start];
    let mut run = 1i32;
    while run < max_run && start + (run as usize) < pts.len() {
        let next = pts[start + run as usize];
        if next.y != first.y || next.x != first.x.wrapping_add(run as i16) {
            break;
        }
        run += 1;
    }
    run
}

#[inline]
fn covered_prefix_len(pts: &[Point2], start: usize, span_x: i32, span_y: i16, span_end: i32) -> usize {
    let mut i = start + 1;
    while i < pts.len() {
        let p = pts[i];
        if p.y != span_y || (p.x as i32) < span_x || (p.x as i32) >= span_end {
            break;
        }
        i += 1;
    }
    i - start
}

/// Core implementation shared by the hook and unit tests.
unsafe fn add_pixels_with_grow(shape: usize, pts: &[Point2], grow: unsafe fn(usize, usize, &Span)) {
    let mut i = 0usize;
    while i < pts.len() {
        let pt = pts[i];
        let px = pt.x as i32;
        let py = pt.y;

        let last = read_usize(shape + 0x18);
        if last != 0 {
            let mut s = (last as *const Span).read_unaligned();
            if s.y == py {
                let sx = s.x as i32;
                let slen = s.len as i32;
                if sx + slen == px {
                    let max_add = (i16::MAX as i32 - slen).max(1);
                    let run = contiguous_run_len(pts, i, max_add);
                    s.len = (slen + run) as i16;
                    (last as *mut Span).write_unaligned(s);
                    i += run as usize;
                    continue;
                }
                if px >= sx && px < sx + slen {
                    i += covered_prefix_len(pts, i, sx, py, sx + slen);
                    continue;
                }
            }
            write_usize(shape + 0x18, 0);
        }

        let begin = read_usize(shape);
        let end = read_usize(shape + 0x08);
        let mut cursor = begin;
        let mut matched = false;
        while cursor < end {
            let mut s = (cursor as *const Span).read_unaligned();
            if s.y == py {
                let sx = s.x as i32;
                let slen = s.len as i32;
                if px >= sx && px < sx + slen {
                    // Restore `last_span` on existing-point hits so subsequent
                    // pixels on the same span resolve in O(1).
                    write_usize(shape + 0x18, cursor);
                    i += covered_prefix_len(pts, i, sx, py, sx + slen);
                    matched = true;
                    break;
                }
                if sx + slen == px {
                    s.len = s.len.wrapping_add(1);
                    (cursor as *mut Span).write_unaligned(s);
                    i += 1;
                    matched = true;
                    break;
                }
                if sx - 1 == px {
                    s.x = s.x.wrapping_sub(1);
                    s.len = s.len.wrapping_add(1);
                    (cursor as *mut Span).write_unaligned(s);
                    i += 1;
                    matched = true;
                    break;
                }
            }
            cursor += SPAN_SIZE;
        }
        if matched {
            continue;
        }

        // Appending a new span sets `last_span` to it, so in the game's loop
        // every following contiguous pixel on the same row extends this span
        // through the fast path without scanning earlier spans.
        let run = contiguous_run_len(pts, i, i16::MAX as i32);
        let new_span = Span { x: pt.x, y: py, len: run as i16 };
        let cap = read_usize(shape + 0x10);
        if end == cap {
            grow(shape, end, &new_span);
        } else {
            (end as *mut Span).write_unaligned(new_span);
            write_usize(shape + 0x08, end + SPAN_SIZE);
        }
        let new_end = read_usize(shape + 0x08);
        write_usize(shape + 0x18, new_end - SPAN_SIZE);
        i += run as usize;
    }
}

unsafe fn native_grow(shape: usize, end: usize, span: &Span) {
    PUSH_SPAN_FN.get()(shape, end, span as *const Span);
}

#[called_by_game]
unsafe extern "C" fn add_pixels(shape: usize, points: *const Point2, count: i32) {
    if count <= 0 || shape == 0 || points.is_null() {
        return;
    }
    let pts = std::slice::from_raw_parts(points, count as usize);
    add_pixels_with_grow(shape, pts, native_grow);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exact port of vanilla `0xa402a0` operating on a Rust `Vec<Span>`.
    fn vanilla_add_pixels(spans: &mut Vec<Span>, last_idx: &mut Option<usize>, pts: &[Point2]) {
        for &pt in pts {
            if let Some(idx) = *last_idx {
                let s = &mut spans[idx];
                if s.y == pt.y && s.x as i32 + s.len as i32 == pt.x as i32 {
                    s.len = s.len.wrapping_add(1);
                    continue;
                }
                *last_idx = None;
            }
            let mut found = false;
            for s in spans.iter_mut() {
                if s.y == pt.y {
                    let sx = s.x as i32;
                    let slen = s.len as i32;
                    let px = pt.x as i32;
                    if px >= sx && px < sx + slen {
                        found = true;
                        break;
                    }
                    if sx + slen == px {
                        s.len = s.len.wrapping_add(1);
                        found = true;
                        break;
                    }
                    if sx - 1 == px {
                        s.x = s.x.wrapping_sub(1);
                        s.len = s.len.wrapping_add(1);
                        found = true;
                        break;
                    }
                }
            }
            if !found {
                spans.push(Span { x: pt.x, y: pt.y, len: 1 });
                *last_idx = Some(spans.len() - 1);
            }
        }
    }

    struct FakeShape {
        header: [usize; 4],
        buf: Vec<Span>,
    }

    impl FakeShape {
        fn new(cap: usize) -> Self {
            let mut buf = Vec::with_capacity(cap);
            let ptr = buf.as_mut_ptr() as usize;
            Self { header: [ptr, ptr, ptr + cap * SPAN_SIZE, 0], buf }
        }

        fn spans(&self) -> &[Span] {
            let len = (self.header[1] - self.header[0]) / SPAN_SIZE;
            unsafe { std::slice::from_raw_parts(self.header[0] as *const Span, len) }
        }
    }

    unsafe fn unreachable_grow(_shape: usize, _end: usize, _span: &Span) {
        panic!("test shape capacity exceeded");
    }

    #[test]
    fn repeated_map_scans_and_mixed_runs_match_vanilla_byte_for_byte() {
        let mut vanilla_spans = Vec::new();
        let mut vanilla_last = None;
        let mut fast = FakeShape::new(256);

        let batches: &[&[Point2]] = &[
            // Initial scan: two disjoint runs on row 10, one on row 11.
            &[Point2 { x: 5, y: 10 }, Point2 { x: 6, y: 10 }, Point2 { x: 7, y: 10 }],
            &[Point2 { x: 20, y: 10 }, Point2 { x: 21, y: 10 }],
            &[Point2 { x: 4, y: 11 }, Point2 { x: 5, y: 11 }, Point2 { x: 6, y: 11 }, Point2 { x: 7, y: 11 }],
            // Second scan (simulating reset without clearing shapes): exact repeats.
            &[Point2 { x: 5, y: 10 }, Point2 { x: 6, y: 10 }, Point2 { x: 7, y: 10 }],
            &[Point2 { x: 20, y: 10 }, Point2 { x: 21, y: 10 }],
            &[Point2 { x: 4, y: 11 }, Point2 { x: 5, y: 11 }, Point2 { x: 6, y: 11 }, Point2 { x: 7, y: 11 }],
            // Edge cases: left-extend, right-extend, out-of-order, partial overlap, bridge.
            &[Point2 { x: 4, y: 10 }, Point2 { x: 8, y: 10 }, Point2 { x: 6, y: 10 }, Point2 { x: 19, y: 10 }, Point2 { x: 22, y: 10 }],
            // New span whose horizontal run runs right into an existing span on row 10.
            &[Point2 { x: 1, y: 10 }, Point2 { x: 2, y: 10 }, Point2 { x: 3, y: 10 }, Point2 { x: 4, y: 10 }, Point2 { x: 5, y: 10 }],
        ];

        for batch in batches {
            vanilla_add_pixels(&mut vanilla_spans, &mut vanilla_last, batch);
            unsafe {
                add_pixels_with_grow(fast.header.as_mut_ptr() as usize, batch, unreachable_grow);
            }
            assert_eq!(fast.spans(), vanilla_spans.as_slice());
        }
        // Keep `fast.buf` alive for the slice pointer.
        let _ = fast.buf.capacity();
    }
}
