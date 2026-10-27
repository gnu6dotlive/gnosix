//! BLOB-AC-03: a keyboard cursor over a list, and the scroll that keeps it
//! visible when the rows are uniform.
//!
//! Pure policy. The consumer owns the rows, the text field that feeds them,
//! and the painting; this module only answers "which row is current" and
//! "where must the list scroll so that row is seen".

/// What a single step does at either end of the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ends {
    /// Stay on the first or last row.
    Clamp,
    /// Previous from the first row goes to the last, and Next from the last
    /// goes to the first. Page steps still clamp.
    Wrap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorStep {
    Previous,
    Next,
    First,
    Last,
    /// Move back by a page of rows (the caller gives the page size).
    PageUp,
    PageDown,
}

/// The current row of a list of `len` rows. An empty list has no current row;
/// any other list always has one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListCursor {
    len: usize,
    current: Option<usize>,
    ends: Ends,
}

impl ListCursor {
    pub fn new(ends: Ends) -> Self {
        Self {
            len: 0,
            current: None,
            ends,
        }
    }

    /// New contents: the first row becomes current.
    pub fn reset(&mut self, len: usize) {
        self.len = len;
        self.current = (len > 0).then_some(0);
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn current(&self) -> Option<usize> {
        self.current
    }

    /// Make `index` current. Out of range is refused. Returns whether the
    /// current row changed.
    pub fn select(&mut self, index: usize) -> bool {
        if index >= self.len || self.current == Some(index) {
            return false;
        }
        self.current = Some(index);
        true
    }

    /// Apply one step. `page` is the number of rows a page step moves (at
    /// least one). Returns whether the current row changed.
    pub fn step(&mut self, step: CursorStep, page: usize) -> bool {
        let Some(current) = self.current else {
            return false;
        };
        let last = self.len - 1;
        let page = page.max(1);
        let next = match (step, self.ends) {
            (CursorStep::Previous, Ends::Wrap) if current == 0 => last,
            (CursorStep::Next, Ends::Wrap) if current == last => 0,
            (CursorStep::Previous, _) => current.saturating_sub(1),
            (CursorStep::Next, _) => (current + 1).min(last),
            (CursorStep::First, _) => 0,
            (CursorStep::Last, _) => last,
            (CursorStep::PageUp, _) => current.saturating_sub(page),
            (CursorStep::PageDown, _) => current.saturating_add(page).min(last),
        };
        self.select(next)
    }
}

/// Rows of one height stacked from the top with one gap between them: row
/// `i` covers `[i * pitch, i * pitch + extent)` in content coordinates. A
/// negative or non-finite height or gap counts as zero.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UniformRows {
    pub extent: f32,
    pub gap: f32,
}

fn finite_non_negative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

impl UniformRows {
    pub fn new(extent: f32, gap: f32) -> Self {
        Self {
            extent: finite_non_negative(extent),
            gap: finite_non_negative(gap),
        }
    }

    fn extent(self) -> f32 {
        finite_non_negative(self.extent)
    }

    fn gap(self) -> f32 {
        finite_non_negative(self.gap)
    }

    pub fn pitch(self) -> f32 {
        self.extent() + self.gap()
    }

    /// Height of `len` rows: no gap after the last one.
    pub fn content_extent(self, len: usize) -> f32 {
        if len == 0 {
            return 0.0;
        }
        len as f32 * self.pitch() - self.gap()
    }

    pub fn max_scroll(self, len: usize, viewport: f32) -> f32 {
        (self.content_extent(len) - finite_non_negative(viewport)).max(0.0)
    }

    pub fn clamp_scroll(self, scroll: f32, len: usize, viewport: f32) -> f32 {
        finite_non_negative(scroll).min(self.max_scroll(len, viewport))
    }

    /// The smallest scroll change that shows all of row `index`. A row taller
    /// than the viewport is aligned to the top.
    pub fn keep_visible(self, scroll: f32, index: usize, len: usize, viewport: f32) -> f32 {
        let scroll = self.clamp_scroll(scroll, len, viewport);
        if index >= len {
            return scroll;
        }
        let viewport = finite_non_negative(viewport);
        let top = index as f32 * self.pitch();
        let bottom = top + self.extent();
        let scroll = if top < scroll || bottom - top > viewport {
            top
        } else if bottom > scroll + viewport {
            bottom - viewport
        } else {
            scroll
        };
        self.clamp_scroll(scroll, len, viewport)
    }

    /// The row under `y`, measured from the top of the viewport. A point in
    /// a gap, above the list, or past the last row is no row.
    pub fn row_at(self, scroll: f32, y: f32, len: usize) -> Option<usize> {
        let pitch = self.pitch();
        if !y.is_finite() || pitch <= 0.0 {
            return None;
        }
        let content_y = finite_non_negative(scroll) + y;
        if content_y < 0.0 {
            return None;
        }
        let index = (content_y / pitch).floor() as usize;
        let within = content_y - index as f32 * pitch;
        (index < len && within < self.extent()).then_some(index)
    }

    /// Height that shows at most `rows` rows of a `len`-row list.
    pub fn viewport_for(self, rows: usize, len: usize) -> f32 {
        self.content_extent(rows.min(len))
    }

    /// Rows a page step moves: the rows that fit wholly in the viewport, at
    /// least one.
    pub fn page(self, viewport: f32) -> usize {
        let pitch = self.pitch();
        if pitch <= 0.0 {
            return 1;
        }
        (((finite_non_negative(viewport) + self.gap()) / pitch).floor() as usize).max(1)
    }
}
