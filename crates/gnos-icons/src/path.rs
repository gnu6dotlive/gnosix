//! SVG path data to absolute segments on the icon grid.
//!
//! Covers the whole path grammar Tabler uses (and the rest of SVG 1.1's):
//! move, line, horizontal, vertical, cubic and quadratic curves with their
//! smooth forms, elliptical arcs and close, absolute or relative, with
//! implicit repeats and compact numbers (`1.5.5`, `-1-2`, `1e-3`).

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    fn add(self, o: Point) -> Point {
        Point::new(self.x + o.x, self.y + o.y)
    }

    fn reflect(self, about: Point) -> Point {
        Point::new(2.0 * about.x - self.x, 2.0 * about.y - self.y)
    }

    fn distance(self, o: Point) -> f32 {
        (self.x - o.x).hypot(self.y - o.y)
    }
}

/// One absolute segment. Smooth curves are already expanded, `h`/`v` are
/// lines.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Segment {
    MoveTo(Point),
    LineTo(Point),
    Quad {
        ctrl: Point,
        to: Point,
    },
    Cubic {
        ctrl1: Point,
        ctrl2: Point,
        to: Point,
    },
    Arc {
        radii: Point,
        rotation: f32,
        large_arc: bool,
        sweep: bool,
        to: Point,
    },
    Close,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub at: usize,
    pub reason: &'static str,
}

struct Lexer<'a> {
    s: &'a [u8],
    i: usize,
}

impl Lexer<'_> {
    fn skip(&mut self) {
        while self.i < self.s.len()
            && (self.s[self.i].is_ascii_whitespace() || self.s[self.i] == b',')
        {
            self.i += 1;
        }
    }

    fn command(&mut self) -> Option<u8> {
        self.skip();
        let c = *self.s.get(self.i)?;
        c.is_ascii_alphabetic().then(|| {
            self.i += 1;
            c
        })
    }

    fn at_number(&mut self) -> bool {
        self.skip();
        self.s
            .get(self.i)
            .is_some_and(|c| c.is_ascii_digit() || matches!(c, b'-' | b'+' | b'.'))
    }

    fn number(&mut self) -> Result<f32, ParseError> {
        self.skip();
        let start = self.i;
        let s = self.s;
        let mut i = self.i;
        if i < s.len() && matches!(s[i], b'-' | b'+') {
            i += 1;
        }
        let mut dot = false;
        while i < s.len() && (s[i].is_ascii_digit() || (s[i] == b'.' && !dot)) {
            dot |= s[i] == b'.';
            i += 1;
        }
        if i < s.len() && matches!(s[i], b'e' | b'E') {
            let mut j = i + 1;
            if j < s.len() && matches!(s[j], b'-' | b'+') {
                j += 1;
            }
            if j < s.len() && s[j].is_ascii_digit() {
                i = j;
                while i < s.len() && s[i].is_ascii_digit() {
                    i += 1;
                }
            }
        }
        self.i = i;
        std::str::from_utf8(&s[start..i])
            .ok()
            .and_then(|t| t.parse().ok())
            .ok_or(ParseError {
                at: start,
                reason: "number expected",
            })
    }

    /// An arc flag: a single 0 or 1, possibly glued to what follows.
    fn flag(&mut self) -> Result<bool, ParseError> {
        self.skip();
        let at = self.i;
        match self.s.get(self.i) {
            Some(b'0') => {
                self.i += 1;
                Ok(false)
            }
            Some(b'1') => {
                self.i += 1;
                Ok(true)
            }
            _ => Err(ParseError {
                at,
                reason: "arc flag expected",
            }),
        }
    }
}

/// Parse path data into absolute segments.
pub fn parse(d: &str) -> Result<Vec<Segment>, ParseError> {
    let mut lx = Lexer {
        s: d.as_bytes(),
        i: 0,
    };
    let mut out = Vec::new();
    let mut cur = Point::new(0.0, 0.0);
    let mut start = cur;
    // The last control point, for smooth curves.
    let mut last_cubic: Option<Point> = None;
    let mut last_quad: Option<Point> = None;
    let mut command: Option<u8> = None;
    loop {
        let c = match lx.command() {
            Some(c) => c,
            None if lx.at_number() => match command {
                // Coordinates after a move are lines.
                Some(b'M') => b'L',
                Some(b'm') => b'l',
                Some(c) if !matches!(c, b'Z' | b'z') => c,
                _ => {
                    return Err(ParseError {
                        at: lx.i,
                        reason: "coordinates without a command",
                    });
                }
            },
            None => {
                lx.skip();
                if lx.i < lx.s.len() {
                    return Err(ParseError {
                        at: lx.i,
                        reason: "unexpected character",
                    });
                }
                break;
            }
        };
        command = Some(c);
        let rel = c.is_ascii_lowercase();
        let base = if rel { cur } else { Point::new(0.0, 0.0) };
        let point = |lx: &mut Lexer| -> Result<Point, ParseError> {
            Ok(Point::new(lx.number()?, lx.number()?).add(base))
        };
        let (mut cubic, mut quad) = (None, None);
        match c.to_ascii_uppercase() {
            b'M' => {
                cur = point(&mut lx)?;
                start = cur;
                out.push(Segment::MoveTo(cur));
            }
            b'L' => {
                cur = point(&mut lx)?;
                out.push(Segment::LineTo(cur));
            }
            b'H' => {
                let x = lx.number()?;
                cur = Point::new(if rel { cur.x + x } else { x }, cur.y);
                out.push(Segment::LineTo(cur));
            }
            b'V' => {
                let y = lx.number()?;
                cur = Point::new(cur.x, if rel { cur.y + y } else { y });
                out.push(Segment::LineTo(cur));
            }
            b'C' => {
                let (ctrl1, ctrl2, to) = (point(&mut lx)?, point(&mut lx)?, point(&mut lx)?);
                out.push(Segment::Cubic { ctrl1, ctrl2, to });
                cubic = Some(ctrl2);
                cur = to;
            }
            b'S' => {
                let ctrl1 = last_cubic.map_or(cur, |p| p.reflect(cur));
                let (ctrl2, to) = (point(&mut lx)?, point(&mut lx)?);
                out.push(Segment::Cubic { ctrl1, ctrl2, to });
                cubic = Some(ctrl2);
                cur = to;
            }
            b'Q' => {
                let (ctrl, to) = (point(&mut lx)?, point(&mut lx)?);
                out.push(Segment::Quad { ctrl, to });
                quad = Some(ctrl);
                cur = to;
            }
            b'T' => {
                let ctrl = last_quad.map_or(cur, |p| p.reflect(cur));
                let to = point(&mut lx)?;
                out.push(Segment::Quad { ctrl, to });
                quad = Some(ctrl);
                cur = to;
            }
            b'A' => {
                let radii = Point::new(lx.number()?.abs(), lx.number()?.abs());
                let rotation = lx.number()?;
                let (large_arc, sweep) = (lx.flag()?, lx.flag()?);
                let to = point(&mut lx)?;
                out.push(Segment::Arc {
                    radii,
                    rotation,
                    large_arc,
                    sweep,
                    to,
                });
                cur = to;
            }
            b'Z' => {
                out.push(Segment::Close);
                cur = start;
            }
            _ => {
                return Err(ParseError {
                    at: lx.i - 1,
                    reason: "unknown command",
                });
            }
        }
        last_cubic = cubic;
        last_quad = quad;
    }
    Ok(out)
}

const SAMPLES: usize = 16;

fn cubic_at(p0: Point, c1: Point, c2: Point, p1: Point, t: f32) -> Point {
    let u = 1.0 - t;
    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    Point::new(
        a * p0.x + b * c1.x + c * c2.x + d * p1.x,
        a * p0.y + b * c1.y + c * c2.y + d * p1.y,
    )
}

/// An SVG arc's points (endpoint to centre parametrisation, SVG 1.1 F.6.5).
fn arc_points(
    from: Point,
    radii: Point,
    rotation: f32,
    large: bool,
    sweep: bool,
    to: Point,
) -> Vec<Point> {
    let (mut rx, mut ry) = (radii.x, radii.y);
    if rx == 0.0 || ry == 0.0 || from == to {
        return vec![from, to];
    }
    let (s, c) = rotation.to_radians().sin_cos();
    let (dx, dy) = ((from.x - to.x) / 2.0, (from.y - to.y) / 2.0);
    let (x1, y1) = (c * dx + s * dy, -s * dx + c * dy);
    let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if lambda > 1.0 {
        rx *= lambda.sqrt();
        ry *= lambda.sqrt();
    }
    let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut k = (num / den).max(0.0).sqrt();
    if large == sweep {
        k = -k;
    }
    let (cx1, cy1) = (k * rx * y1 / ry, -k * ry * x1 / rx);
    let (cx, cy) = (
        c * cx1 - s * cy1 + (from.x + to.x) / 2.0,
        s * cx1 + c * cy1 + (from.y + to.y) / 2.0,
    );
    let angle = |ux: f32, uy: f32| uy.atan2(ux);
    let theta = angle((x1 - cx1) / rx, (y1 - cy1) / ry);
    let mut delta = angle((-x1 - cx1) / rx, (-y1 - cy1) / ry) - theta;
    let tau = std::f32::consts::TAU;
    if sweep && delta < 0.0 {
        delta += tau;
    } else if !sweep && delta > 0.0 {
        delta -= tau;
    }
    (0..=SAMPLES)
        .map(|i| {
            let a = theta + delta * i as f32 / SAMPLES as f32;
            let (x, y) = (rx * a.cos(), ry * a.sin());
            Point::new(c * x - s * y + cx, s * x + c * y + cy)
        })
        .collect()
}

/// The stroke's length on the grid, curves and arcs sampled: what a
/// progressive draw-on dashes over.
pub fn length(segments: &[Segment]) -> f32 {
    let mut total = 0.0;
    let mut cur = Point::new(0.0, 0.0);
    let mut start = cur;
    let poly = |pts: &[Point]| pts.windows(2).map(|w| w[0].distance(w[1])).sum::<f32>();
    for s in segments {
        match *s {
            Segment::MoveTo(p) => {
                cur = p;
                start = p;
            }
            Segment::LineTo(p) => {
                total += cur.distance(p);
                cur = p;
            }
            Segment::Quad { ctrl, to } => {
                // As a cubic.
                let c1 = Point::new(
                    cur.x + 2.0 / 3.0 * (ctrl.x - cur.x),
                    cur.y + 2.0 / 3.0 * (ctrl.y - cur.y),
                );
                let c2 = Point::new(
                    to.x + 2.0 / 3.0 * (ctrl.x - to.x),
                    to.y + 2.0 / 3.0 * (ctrl.y - to.y),
                );
                let pts: Vec<Point> = (0..=SAMPLES)
                    .map(|i| cubic_at(cur, c1, c2, to, i as f32 / SAMPLES as f32))
                    .collect();
                total += poly(&pts);
                cur = to;
            }
            Segment::Cubic { ctrl1, ctrl2, to } => {
                let pts: Vec<Point> = (0..=SAMPLES)
                    .map(|i| cubic_at(cur, ctrl1, ctrl2, to, i as f32 / SAMPLES as f32))
                    .collect();
                total += poly(&pts);
                cur = to;
            }
            Segment::Arc {
                radii,
                rotation,
                large_arc,
                sweep,
                to,
            } => {
                total += poly(&arc_points(cur, radii, rotation, large_arc, sweep, to));
                cur = to;
            }
            Segment::Close => {
                total += cur.distance(start);
                cur = start;
            }
        }
    }
    total
}
