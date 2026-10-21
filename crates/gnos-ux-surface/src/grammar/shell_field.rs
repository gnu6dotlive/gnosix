//! SHELL-FIELD-01B: the whole shell as one signed distance field.
//!
//! The frame, the Bar and everything extruded out of them (panels, popouts,
//! the Deck) are one shape: the frame is the output with a rounded hollow,
//! and each member is a rounded box joined to the rest by a smooth union
//! whose radius is the fillet. Continuity follows from the construction: a
//! member that comes out of the Bar is the same shape as the Bar, so there is
//! no second layer and no seam to hide.
//!
//! The field is negative inside the shell. From it:
//! - `contour` traces its edge by marching squares, linearly interpolated:
//!   the hollow's edge, with every member and fillet in it. Filled as one
//!   path, the output's rectangle with these loops as holes, the shell has no
//!   internal edge, so no antialiasing seam between pieces;
//! - `topology` counts its connected pieces and its holes on the same grid,
//!   so a member torn from the Bar is a failed receipt, not a look.
//!
//! The vocabulary follows blob.in's (CSG, `smooth_union`, components and
//! holes); the code is Gnosix's own.

use super::curves::Point2;

/// A rounded box: `[left, top, right, bottom]` and its corner radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FieldBox {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub radius: f32,
}

impl FieldBox {
    pub fn new(left: f32, top: f32, right: f32, bottom: f32, radius: f32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
            radius,
        }
    }

    /// Signed distance to the box's edge (negative inside).
    pub fn distance(&self, x: f32, y: f32) -> f32 {
        let hw = (self.right - self.left) * 0.5;
        let hh = (self.bottom - self.top) * 0.5;
        let r = self.radius.clamp(0.0, hw.min(hh).max(0.0));
        let qx = (x - (self.left + hw)).abs() - (hw - r);
        let qy = (y - (self.top + hh)).abs() - (hh - r);
        let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
        outside + qx.max(qy).min(0.0) - r
    }

    fn grown(&self, by: f32) -> [f32; 4] {
        [
            self.left - by,
            self.top - by,
            self.right + by,
            self.bottom + by,
        ]
    }
}

/// Polynomial smooth minimum: `min(a, b)` rounded over `k` (the fillet).
pub fn smooth_union(a: f32, b: f32, k: f32) -> f32 {
    if k <= 0.0 {
        return a.min(b);
    }
    let h = (k - (a - b).abs()).max(0.0) / k;
    a.min(b) - h * h * k * 0.25
}

/// The shell: an output-sized frame with a rounded hollow, and its members.
#[derive(Clone, Debug, PartialEq)]
pub struct ShellField {
    /// The output, `(width, height)`.
    pub output: (f32, f32),
    /// The hollow inside the frame (the Bar is the frame's thick side).
    pub hollow: FieldBox,
    /// What is extruded out of the frame, each joined by `blend`.
    pub members: Vec<FieldBox>,
    /// The fillet radius of every joint.
    pub blend: f32,
}

/// What `topology` found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldTopology {
    /// Connected pieces of shell (one when everything is joined).
    pub components: usize,
    /// Pieces of non-shell enclosed by shell (the hollow is one).
    pub holes: usize,
}

impl ShellField {
    /// Signed distance to the shell's edge (negative inside the shell).
    pub fn distance(&self, x: f32, y: f32) -> f32 {
        // The frame: everything outside the hollow (past the output too, so
        // the only edge is the hollow's, with what comes out into it).
        let frame = -self.hollow.distance(x, y);
        let output = FieldBox::new(0.0, 0.0, self.output.0, self.output.1, 0.0);
        self.members.iter().fold(frame, |d, m| {
            // A member farther than `d + blend` changes nothing: a cheap
            // lower bound (the distance to its box, corners ignored) skips it.
            let gap = (m.left - x)
                .max(x - m.right)
                .max(m.top - y)
                .max(y - m.bottom);
            if gap > d + self.blend {
                return d;
            }
            // A member never spills past the output.
            let member = m.distance(x, y).max(output.distance(x, y));
            smooth_union(d, member, self.blend)
        })
    }

    /// Where the edge can differ from the bare frame: each member grown by
    /// the fillet, clipped to the output.
    fn active(&self, margin: f32) -> Vec<[f32; 4]> {
        self.members
            .iter()
            .map(|m| {
                let [l, t, r, b] = m.grown(self.blend + margin);
                [
                    l.max(0.0),
                    t.max(0.0),
                    r.min(self.output.0),
                    b.min(self.output.1),
                ]
            })
            .collect()
    }

    /// The shell's edge as closed loops, traced on a `cell` px grid over the
    /// whole output and linearly interpolated (exact for straight edges;
    /// within `cell² / 8r` of an arc of radius `r`).
    pub fn contour(&self, cell: f32) -> Vec<Vec<Point2>> {
        let cell = cell.max(0.5);
        Grid::sample(self, cell, 0.0).loops(0.0)
    }

    /// Components and holes, counted on the same grid as `contour`.
    pub fn topology(&self, cell: f32) -> FieldTopology {
        Grid::sample(self, cell.max(0.5), 0.0).topology()
    }

    /// The loops at each level (`0` is the edge; a positive level runs
    /// that far inside the hollow, where the shell's inner shadow falls),
    /// sampled once.
    pub fn contours(&self, cell: f32, levels: &[f32]) -> Vec<Vec<Vec<Point2>>> {
        let band = levels.iter().copied().fold(0.0f32, f32::max);
        let grid = Grid::sample(self, cell.max(0.5), band);
        levels.iter().map(|level| grid.loops(*level)).collect()
    }

    /// The regions a member's joint touches, for callers that only repaint
    /// there.
    pub fn joint_regions(&self) -> Vec<[f32; 4]> {
        self.active(0.0)
    }
}

#[cfg(test)]
static EVALS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The field sampled at the corners of a regular grid.
struct Grid {
    cols: usize,
    rows: usize,
    cell: f32,
    values: Vec<f32>,
    /// The cells that may hold the edge (all others were proven far).
    near: Vec<(usize, usize)>,
}

impl Grid {
    /// Sample exactly where a level in `[0, band]` can cross.
    fn sample(field: &ShellField, cell: f32, band: f32) -> Self {
        // The grid runs two cells past the output, all of it frame, so the
        // only loops are the hollow's.
        let cols = (field.output.0 / cell).ceil() as usize + 5;
        let rows = (field.output.1 / cell).ceil() as usize + 5;
        let at = |i: usize, j: usize| {
            #[cfg(test)]
            EVALS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            field.distance((i as f32 - 2.0) * cell, (j as f32 - 2.0) * cell)
        };
        // Coarse to fine: the field is a distance (it changes by at most a
        // pixel per pixel), so a block whose corners all lie farther from the
        // edge than half its diagonal holds no edge and only
        // needs its sign. Blocks of 16 cells are tried, then of 4, and only
        // blocks near the edge are sampled at every cell.
        let mut sampler = Sampler {
            values: vec![f32::NAN; cols * rows],
            exact: vec![false; cols * rows],
            near: Vec::new(),
            cols,
            cell,
            band,
            at,
        };
        sampler.fill((0, 0, cols - 1, rows - 1), 16);
        let Sampler { values, near, .. } = sampler;
        Self {
            cols,
            rows,
            cell,
            values,
            near,
        }
    }

    fn at(&self, i: usize, j: usize) -> f32 {
        self.values[j * self.cols + i]
    }

    fn point(&self, i: f32, j: f32) -> Point2 {
        Point2::new((i - 2.0) * self.cell, (j - 2.0) * self.cell)
    }

    /// Where the edge crosses the grid edge from corner `a` to corner `b`.
    fn crossing(&self, a: (usize, usize), b: (usize, usize), level: f32) -> Point2 {
        let (va, vb) = (self.at(a.0, a.1) - level, self.at(b.0, b.1) - level);
        let t = if (va - vb).abs() < f32::EPSILON {
            0.5
        } else {
            (va / (va - vb)).clamp(0.0, 1.0)
        };
        self.point(
            a.0 as f32 + (b.0 as f32 - a.0 as f32) * t,
            a.1 as f32 + (b.1 as f32 - a.1 as f32) * t,
        )
    }

    /// Marching squares: segments oriented with the shell on their left,
    /// chained into closed loops.
    fn loops(&self, level: f32) -> Vec<Vec<Point2>> {
        // A grid edge's id: its lower corner and direction (0 along x, 1
        // along y). `next[id]` is the edge the contour goes to from it.
        let id = |i: usize, j: usize, along_x: bool| {
            ((j * self.cols + i) * 2 + usize::from(!along_x)) as u32
        };
        // (from edge, to edge, the crossing on `from`).
        let mut links: Vec<(u32, u32, Point2)> = Vec::with_capacity(self.near.len());
        for &(i, j) in &self.near {
            let v = [
                self.at(i, j),
                self.at(i + 1, j),
                self.at(i + 1, j + 1),
                self.at(i, j + 1),
            ];
            let mask = (u8::from(v[0] < level))
                | (u8::from(v[1] < level) << 1)
                | (u8::from(v[2] < level) << 2)
                | (u8::from(v[3] < level) << 3);
            if mask == 0 || mask == 15 {
                continue;
            }
            let corners = [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)];
            // The cell's four edges: top, right, bottom, left.
            let edges = [
                (id(i, j, true), corners[0], corners[1]),
                (id(i + 1, j, false), corners[1], corners[2]),
                (id(i, j + 1, true), corners[3], corners[2]),
                (id(i, j, false), corners[0], corners[3]),
            ];
            let centre_inside = v.iter().sum::<f32>() < 4.0 * level;
            for (from, to) in segments(mask, centre_inside) {
                let (fid, fa, fb) = edges[from];
                links.push((fid, edges[to].0, self.crossing(fa, fb, level)));
            }
        }
        links.sort_unstable_by_key(|l| l.0);
        let mut used = vec![false; links.len()];
        let find = |k: u32| links.binary_search_by_key(&k, |l| l.0).ok();
        let mut loops = Vec::new();
        for first in 0..links.len() {
            if used[first] {
                continue;
            }
            let mut lp = Vec::new();
            let mut at = first;
            while !used[at] {
                used[at] = true;
                lp.push(links[at].2);
                match find(links[at].1) {
                    Some(n) => at = n,
                    None => break,
                }
            }
            if lp.len() >= 3 {
                loops.push(simplify(lp));
            }
        }
        loops
    }

    fn topology(&self) -> FieldTopology {
        let inside: Vec<bool> = self.values.iter().map(|v| *v < 0.0).collect();
        let count = |want: bool, skip_border: bool| -> usize {
            let mut seen = vec![false; inside.len()];
            let mut n = 0;
            for start in 0..inside.len() {
                if seen[start] || inside[start] != want {
                    continue;
                }
                let mut touches_border = false;
                let mut stack = vec![start];
                seen[start] = true;
                while let Some(k) = stack.pop() {
                    let (i, j) = (k % self.cols, k / self.cols);
                    if i == 0 || j == 0 || i == self.cols - 1 || j == self.rows - 1 {
                        touches_border = true;
                    }
                    let mut push = |ni: usize, nj: usize| {
                        let nk = nj * self.cols + ni;
                        if !seen[nk] && inside[nk] == want {
                            seen[nk] = true;
                            stack.push(nk);
                        }
                    };
                    if i > 0 {
                        push(i - 1, j);
                    }
                    if j > 0 {
                        push(i, j - 1);
                    }
                    if i + 1 < self.cols {
                        push(i + 1, j);
                    }
                    if j + 1 < self.rows {
                        push(i, j + 1);
                    }
                }
                if !(skip_border && touches_border) {
                    n += 1;
                }
            }
            n
        };
        FieldTopology {
            components: count(true, false),
            holes: count(false, true),
        }
    }
}

/// The grid being sampled, coarse to fine.
struct Sampler<F: Fn(usize, usize) -> f32> {
    values: Vec<f32>,
    /// Whether a value is the field's (else a bound proven far).
    exact: Vec<bool>,
    near: Vec<(usize, usize)>,
    cols: usize,
    cell: f32,
    /// The farthest level to trace.
    band: f32,
    at: F,
}

impl<F: Fn(usize, usize) -> f32> Sampler<F> {
    fn sample(&mut self, i: usize, j: usize) -> f32 {
        let k = j * self.cols + i;
        if self.values[k].is_nan() {
            self.values[k] = (self.at)(i, j);
            self.exact[k] = true;
        }
        self.values[k]
    }

    /// Sample the block `(i0, j0, i1, j1)`: a block far from every level is
    /// filled with a bound, a near one split into blocks of `block / 4`
    /// cells; a single cell a level crosses is sampled exactly.
    fn fill(&mut self, (i0, j0, i1, j1): (usize, usize, usize, usize), block: usize) {
        for bj in (j0..j1.max(j0 + 1)).step_by(block) {
            for bi in (i0..i1.max(i0 + 1)).step_by(block) {
                let (ei, ej) = ((bi + block).min(i1), (bj + block).min(j1));
                let corners = [
                    self.sample(bi, bj),
                    self.sample(ei, bj),
                    self.sample(bi, ej),
                    self.sample(ei, ej),
                ];
                let diagonal = ((ei - bi) as f32).hypot((ej - bj) as f32) * self.cell;
                // Inside the shell, or past the farthest level: no level
                // crosses.
                let inside = corners.iter().all(|d| *d < 0.0);
                let outside = corners.iter().all(|d| *d > self.band);
                // Every point of the block is within half its diagonal of a
                // corner, and the field changes by at most a pixel per pixel
                // (the smooth union keeps that slope); a little margin on top.
                let reach = diagonal * 0.55;
                let clear = if inside {
                    corners.iter().fold(f32::MAX, |m, d| m.min(-d))
                } else {
                    corners.iter().fold(f32::MAX, |m, d| m.min(d - self.band))
                };
                if (inside || outside) && clear > reach {
                    // Every point inside is at least this clear of the levels.
                    let margin = (clear - reach).max(f32::MIN_POSITIVE);
                    let bound = if inside { -margin } else { self.band + margin };
                    for j in bj..=ej {
                        for i in bi..=ei {
                            let k = j * self.cols + i;
                            if self.values[k].is_nan() {
                                self.values[k] = bound;
                            }
                        }
                    }
                } else if block > 1 {
                    self.fill((bi, bj, ei, ej), (block / 4).max(1));
                } else if !(inside || outside) {
                    // One cell a level crosses: its crossings need exact values.
                    for (i, j) in [(bi, bj), (ei, bj), (bi, ej), (ei, ej)] {
                        let k = j * self.cols + i;
                        if !self.exact[k] {
                            self.values[k] = (self.at)(i, j);
                            self.exact[k] = true;
                        }
                    }
                    self.near.push((bi, bj));
                }
            }
        }
    }
}

/// The segments of one marching-squares cell, as (entry edge, exit edge)
/// with the shell on the left. Edges: 0 top, 1 right, 2 bottom, 3 left.
/// Corners (bits): 0 top-left, 1 top-right, 2 bottom-right, 3 bottom-left.
fn segments(mask: u8, centre_inside: bool) -> Vec<(usize, usize)> {
    match mask {
        1 => vec![(3, 0)],
        2 => vec![(0, 1)],
        3 => vec![(3, 1)],
        4 => vec![(1, 2)],
        5 => {
            if centre_inside {
                vec![(3, 2), (1, 0)]
            } else {
                vec![(3, 0), (1, 2)]
            }
        }
        6 => vec![(0, 2)],
        7 => vec![(3, 2)],
        8 => vec![(2, 3)],
        9 => vec![(2, 0)],
        10 => {
            if centre_inside {
                vec![(0, 3), (2, 1)]
            } else {
                vec![(0, 1), (2, 3)]
            }
        }
        11 => vec![(2, 1)],
        12 => vec![(1, 3)],
        13 => vec![(1, 0)],
        14 => vec![(0, 3)],
        _ => Vec::new(),
    }
}

/// Drop points on a straight run (collinear within a hundredth of a pixel),
/// so straight walls cost two points.
fn simplify(points: Vec<Point2>) -> Vec<Point2> {
    let n = points.len();
    if n < 4 {
        return points;
    }
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        let a = points[(k + n - 1) % n];
        let b = points[k];
        let c = points[(k + 1) % n];
        let cross = (b.x() - a.x()) * (c.y() - a.y()) - (b.y() - a.y()) * (c.x() - a.x());
        let len = ((c.x() - a.x()).powi(2) + (c.y() - a.y()).powi(2)).sqrt();
        if len == 0.0 || (cross / len).abs() > 0.01 {
            out.push(b);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUT: (f32, f32) = (1000.0, 800.0);

    /// A shell with a Top Bar 60 px thick and 10 px sides.
    fn shell(members: Vec<FieldBox>) -> ShellField {
        ShellField {
            output: OUT,
            hollow: FieldBox::new(10.0, 60.0, 990.0, 790.0, 25.0),
            members,
            blend: 20.0,
        }
    }

    #[test]
    fn a_rounded_box_measures_its_edge() {
        let b = FieldBox::new(0.0, 0.0, 100.0, 50.0, 10.0);
        assert!((b.distance(50.0, 25.0) + 25.0).abs() < 1e-4, "centre");
        assert!(b.distance(100.0, 25.0).abs() < 1e-4, "on the right edge");
        assert!((b.distance(110.0, 25.0) - 10.0).abs() < 1e-4);
        // The corner is an arc of radius 10 centred at (90, 10).
        let corner = 10.0 / 2f32.sqrt();
        assert!(b.distance(90.0 + corner, 10.0 - corner).abs() < 1e-3);
    }

    #[test]
    fn the_bare_frame_is_one_piece_around_one_hole() {
        let f = shell(Vec::new());
        assert!(f.distance(500.0, 30.0) < 0.0, "the Bar is shell");
        assert!(f.distance(500.0, 400.0) > 0.0, "the hollow is not");
        assert_eq!(
            f.topology(4.0),
            FieldTopology {
                components: 1,
                holes: 1
            }
        );
        let loops = f.contour(4.0);
        assert_eq!(loops.len(), 1, "the hollow's edge only");
    }

    #[test]
    fn a_panel_out_of_the_bar_is_the_same_piece_with_fillets() {
        // A panel hanging from the Bar: its top overlaps the Bar.
        let f = shell(vec![FieldBox::new(400.0, 50.0, 700.0, 300.0, 25.0)]);
        assert_eq!(
            f.topology(4.0),
            FieldTopology {
                components: 1,
                holes: 1
            },
            "one material, the hollow still one hole"
        );
        // The joint is filleted: just beside the panel under the Bar is
        // shell (bare frame would say hollow there).
        assert!(f.distance(397.0, 63.0) < 0.0, "concave fillet at the joint");
        assert!(shell(Vec::new()).distance(397.0, 63.0) > 0.0);
        // The panel's far corners stay round.
        assert!(f.distance(401.0, 299.0) > 0.0);
    }

    #[test]
    fn a_level_inside_the_hollow_runs_parallel_to_the_edge() {
        let f = shell(vec![FieldBox::new(400.0, 50.0, 700.0, 300.0, 25.0)]);
        let levels = f.contours(4.0, &[0.0, 6.0]);
        assert_eq!(levels.len(), 2);
        for p in levels[1]
            .iter()
            .flatten()
            .filter(|p| p.x() > 0.0 && p.y() > 0.0 && p.x() < OUT.0 && p.y() < OUT.1)
        {
            assert!((f.distance(p.x(), p.y()) - 6.0).abs() < 0.6, "{p:?}");
        }
        // The shadow goes around the panel too: a run along y = 306 under
        // its bottom edge spans the panel's middle.
        let under = levels[1].iter().any(|lp| {
            lp.iter().zip(lp.iter().cycle().skip(1)).any(|(a, b)| {
                (a.y() - 306.0).abs() < 1.0
                    && (b.y() - 306.0).abs() < 1.0
                    && a.x().min(b.x()) < 550.0
                    && a.x().max(b.x()) > 550.0
            })
        });
        assert!(under, "the level runs under the panel");
    }

    #[test]
    fn a_member_torn_from_the_bar_is_a_second_piece() {
        let f = shell(vec![FieldBox::new(400.0, 200.0, 700.0, 300.0, 25.0)]);
        assert_eq!(f.topology(4.0).components, 2, "torn: not one material");
    }

    #[test]
    fn the_contour_lies_on_the_edge_and_straight_walls_are_two_points() {
        let f = shell(vec![FieldBox::new(400.0, 50.0, 700.0, 300.0, 25.0)]);
        let loops = f.contour(4.0);
        // On screen, every point is on the shell's edge.
        for lp in &loops {
            for p in lp
                .iter()
                .filter(|p| p.x() > 0.0 && p.y() > 0.0 && p.x() < OUT.0 && p.y() < OUT.1)
            {
                assert!(f.distance(p.x(), p.y()).abs() < 0.6, "{p:?}");
            }
        }
        assert_eq!(loops.len(), 1, "the panel is part of the hollow's edge");
        assert!(loops.iter().map(Vec::len).sum::<usize>() < 200);
    }
}

#[cfg(test)]
mod timing {
    use super::*;

    /// `cargo test --release -p gnos-ux-surface shell_field_timing -- --ignored --nocapture`
    #[test]
    #[ignore = "timing, run in release"]
    fn shell_field_timing() {
        let f = ShellField {
            output: (1080.0, 1920.0),
            hollow: FieldBox::new(10.0, 60.0, 1070.0, 1910.0, 25.0),
            members: vec![
                FieldBox::new(600.0, 50.0, 1000.0, 700.0, 25.0),
                FieldBox::new(100.0, 50.0, 360.0, 400.0, 25.0),
                FieldBox::new(300.0, 1820.0, 800.0, 1915.0, 25.0),
                FieldBox::new(420.0, 1700.0, 560.0, 1830.0, 25.0),
            ],
            blend: 20.0,
        };
        EVALS.store(0, std::sync::atomic::Ordering::Relaxed);
        let t = std::time::Instant::now();
        let g = Grid::sample(&f, 4.0, 0.0);
        println!(
            "evals={} near={} grid={}",
            EVALS.load(std::sync::atomic::Ordering::Relaxed),
            g.near.len(),
            g.values.len()
        );
        let sampled = t.elapsed();
        let t = std::time::Instant::now();
        let n = g.loops(0.0).len();
        println!(
            "split cell=4 sample={sampled:?} loops={:?} ({n})",
            t.elapsed()
        );
        let levels: Vec<f32> = std::iter::once(0.0)
            .chain((0..5).map(|i| i as f32 * 3.0 + 1.5))
            .collect();
        let t = std::time::Instant::now();
        for _ in 0..20 {
            let _ = f.contours(4.0, &levels);
        }
        println!(
            "edge+5 shadow levels cell=4: {:.2} ms",
            t.elapsed().as_secs_f64() * 1000.0 / 20.0
        );
        for cell in [2.0, 3.0, 4.0] {
            let start = std::time::Instant::now();
            let mut n = 0;
            for _ in 0..20 {
                n += f.contour(cell).iter().map(Vec::len).sum::<usize>();
            }
            println!(
                "cell={cell} contour={:.2} ms points={}",
                start.elapsed().as_secs_f64() * 1000.0 / 20.0,
                n / 20
            );
        }
    }
}
