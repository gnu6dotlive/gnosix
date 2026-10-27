//! DICTATION-ANCHOR : l'ancre de l'élément de dictation.
//!
//! Une ancre dit *à quoi* l'élément est accroché ; la résolution dit *où*.
//! Les modes sont des exemples, pas une taxonomie : une ancre inconnue
//! retombe sur son mode par défaut au lieu de casser.
//!
//! L'ancre remplace la fraction que le profil stockait pour le module : une
//! fraction est une position, et une position ignore le mouvement.

use gnos_ux_surface::grammar::GeometryRect;

/// Ce à quoi l'élément est accroché.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AnchorMode {
    /// Au bord le plus proche, à une distance de ce bord.
    #[default]
    Border,
    /// Dans un quadrant de l'output, à une position dans ce quadrant.
    Quadrant,
    /// Libre, mais accroché au plus proche des ancres disponibles.
    Snap,
}

impl AnchorMode {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            AnchorMode::Border => "border",
            AnchorMode::Quadrant => "quadrant",
            AnchorMode::Snap => "snap",
        }
    }

    /// Read a wire spelling. An unknown spelling is refused, and the caller
    /// falls back on the default rather than failing.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "border" => Some(AnchorMode::Border),
            "quadrant" => Some(AnchorMode::Quadrant),
            "snap" => Some(AnchorMode::Snap),
            _ => None,
        }
    }
}

/// Which edge an anchor hangs from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AnchorEdge {
    #[default]
    Left,
    Right,
    Top,
    Bottom,
}

impl AnchorEdge {
    pub fn as_str(self) -> &'static str {
        match self {
            AnchorEdge::Left => "left",
            AnchorEdge::Right => "right",
            AnchorEdge::Top => "top",
            AnchorEdge::Bottom => "bottom",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "left" => Some(AnchorEdge::Left),
            "right" => Some(AnchorEdge::Right),
            "top" => Some(AnchorEdge::Top),
            "bottom" => Some(AnchorEdge::Bottom),
            _ => None,
        }
    }
}

/// The element's anchor: what it hangs from, and where along that.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct DictationAnchor {
    pub mode: AnchorMode,
    pub edge: AnchorEdge,
    /// The distance from the edge, in pixels.
    pub offset: f32,
    /// The position along the edge, in thousandths of the free area (the
    /// output less what the shell reserves), as `resolve` and `moved_to`
    /// both read it.
    pub along: u16,
}

impl DictationAnchor {
    /// The anchor the element starts from: the lower-left, on the left border.
    pub fn default_anchor() -> Self {
        Self {
            mode: AnchorMode::Border,
            edge: AnchorEdge::Left,
            offset: 48.0,
            along: 120,
        }
    }

    /// Resolve the anchor into a rectangle, inside the area the shell leaves
    /// free.
    ///
    /// The insets are the shell's reservation: the element never lands under
    /// the Bar or the Dock.
    pub fn resolve(
        &self,
        viewport: (f32, f32),
        insets: [f32; 4],
        size: (f32, f32),
    ) -> Option<GeometryRect> {
        let [top, right, bottom, left] = insets;
        let free_width = (viewport.0 - left - right).max(0.0);
        let free_height = (viewport.1 - top - bottom).max(0.0);
        if free_width <= size.0 || free_height <= size.1 {
            return None;
        }
        let along = f32::from(self.along.min(1000)) / 1000.0;
        let (x, y) = match self.mode {
            AnchorMode::Border => match self.edge {
                AnchorEdge::Left => (left + self.offset.max(0.0), top + free_height * along),
                AnchorEdge::Right => (
                    left + free_width - size.0 - self.offset.max(0.0),
                    top + free_height * along,
                ),
                AnchorEdge::Top => (left + free_width * along, top + self.offset.max(0.0)),
                AnchorEdge::Bottom => (
                    left + free_width * along,
                    top + free_height - size.1 - self.offset.max(0.0),
                ),
            },
            // A quadrant anchors in its own quarter of the free area.
            AnchorMode::Quadrant => {
                let (column, row) = (
                    if along < 0.5 { 0.0 } else { 0.5 },
                    if self.along < 500 { 0.0 } else { 0.5 },
                );
                (
                    left + free_width * column + self.offset.max(0.0),
                    top + free_height * row + self.offset.max(0.0),
                )
            }
            // A free anchor still lands inside the free area.
            AnchorMode::Snap => (left + free_width * along, top + free_height * along),
        };
        GeometryRect::try_new(x, y, x + size.0, y + size.1, 0.0).ok()
    }

    /// Move the anchor to the edge nearest a point, and slide it along that
    /// edge: the gesture of a surface that follows the pointer.
    ///
    /// `point` and `viewport` are in the free area (its origin and size), the
    /// frame `resolve` places the element in, so a drop resolves back where it
    /// was made.
    pub fn moved_to(&self, point: (f32, f32), viewport: (f32, f32)) -> Self {
        let distances = [
            (point.0, AnchorEdge::Left),
            (viewport.0 - point.0, AnchorEdge::Right),
            (point.1, AnchorEdge::Top),
            (viewport.1 - point.1, AnchorEdge::Bottom),
        ];
        let (distance, edge) = distances
            .iter()
            .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(distance, edge)| (*distance, *edge))
            .unwrap_or((0.0, AnchorEdge::Left));
        let along = match edge {
            AnchorEdge::Left | AnchorEdge::Right => {
                if viewport.1 > 0.0 {
                    (point.1 / viewport.1 * 1000.0).round().clamp(0.0, 1000.0) as u16
                } else {
                    0
                }
            }
            AnchorEdge::Top | AnchorEdge::Bottom => {
                if viewport.0 > 0.0 {
                    (point.0 / viewport.0 * 1000.0).round().clamp(0.0, 1000.0) as u16
                } else {
                    0
                }
            }
        };
        Self {
            mode: AnchorMode::Border,
            edge,
            offset: distance.round().max(0.0),
            along,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEWPORT: (f32, f32) = (1000.0, 800.0);
    const INSETS: [f32; 4] = [60.0, 0.0, 0.0, 0.0];

    #[test]
    fn a_border_anchor_hangs_from_its_edge() {
        let anchor = DictationAnchor {
            mode: AnchorMode::Border,
            edge: AnchorEdge::Left,
            offset: 40.0,
            along: 500,
        };
        let rect = anchor
            .resolve(VIEWPORT, INSETS, (32.0, 32.0))
            .expect("the anchor resolves");
        assert_eq!(rect.left(), 40.0);
        // Half-way down the free area (800 less the 60 reserved).
        assert_eq!(rect.top(), 60.0 + 370.0);
    }

    #[test]
    fn the_element_never_lands_under_the_reservation() {
        let anchor = DictationAnchor {
            mode: AnchorMode::Border,
            edge: AnchorEdge::Top,
            offset: 0.0,
            along: 0,
        };
        let rect = anchor
            .resolve(VIEWPORT, INSETS, (32.0, 32.0))
            .expect("the anchor resolves");
        assert!(rect.top() >= INSETS[0]);
    }

    #[test]
    fn an_unknown_mode_falls_back_on_the_default() {
        assert_eq!(AnchorMode::parse("nope"), None);
        assert_eq!(AnchorMode::default(), AnchorMode::Border);
    }

    #[test]
    fn a_drop_resolves_back_where_it_was_made() {
        let [top, right, bottom, left] = INSETS;
        let free = (VIEWPORT.0 - left - right, VIEWPORT.1 - top - bottom);
        for point in [(12.0, 400.0), (700.0, 30.0), (980.0, 500.0), (300.0, 730.0)] {
            let moved = DictationAnchor::default_anchor().moved_to(point, free);
            let rect = moved
                .resolve(VIEWPORT, INSETS, (0.5, 0.5))
                .expect("the anchor resolves");
            // The element's edge on the anchored side lands on the point (in
            // the scene's coordinates), within the per-mille's rounding.
            let (x, y) = (point.0 + left, point.1 + top);
            let hit = match moved.edge {
                AnchorEdge::Left | AnchorEdge::Right => (rect.top() - y).abs() <= 1.0,
                AnchorEdge::Top | AnchorEdge::Bottom => (rect.left() - x).abs() <= 1.0,
            };
            assert!(hit, "{point:?} -> {moved:?} -> {rect:?}");
        }
    }

    #[test]
    fn dragging_picks_the_nearest_edge_and_slides_along_it() {
        let anchor = DictationAnchor::default_anchor();
        let moved = anchor.moved_to((12.0, 400.0), VIEWPORT);
        assert_eq!(moved.edge, AnchorEdge::Left);
        assert_eq!(moved.offset, 12.0);
        assert_eq!(moved.along, 500);
    }
}
