use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridLayout {
    pub columns: usize,
    pub rows: usize,
    pub cell_width: f32,
    pub cell_height: f32,
    pub spacing: f32,
    pub count: usize,
}

pub const CELL_PADDING: f32 = 3.0;

impl GridLayout {
    pub fn new(width: f32, thumb: f32, label_height: f32, spacing: f32, count: usize) -> GridLayout {
        let width = if width.is_finite() { width.max(1.0) } else { 1.0 };
        let thumb = if thumb.is_finite() { thumb.clamp(8.0, 1024.0) } else { 96.0 };
        let spacing = spacing.max(0.0);
        let min_cell = thumb + 2.0 * CELL_PADDING;
        let columns = (((width + spacing) / (min_cell + spacing)).floor() as usize).max(1);
        let cell_width = ((width - spacing * (columns as f32 - 1.0)) / columns as f32).max(min_cell.min(width));
        let rows = count.div_ceil(columns);
        let cell_height = thumb + 2.0 * CELL_PADDING + label_height;
        GridLayout { columns, rows, cell_width, cell_height, spacing, count }
    }

    pub fn row_stride(&self) -> f32 {
        self.cell_height + self.spacing
    }

    pub fn content_height(&self) -> f32 {
        if self.rows == 0 { 0.0 } else { self.rows as f32 * self.row_stride() - self.spacing }
    }

    pub fn visible_rows(&self, top: f32, bottom: f32) -> Range<usize> {
        if self.rows == 0 || !top.is_finite() || !bottom.is_finite() || bottom <= top {
            return 0..0;
        }
        let stride = self.row_stride().max(1.0);
        let first = (top.max(0.0) / stride).floor() as usize;
        let last = ((bottom.max(0.0) / stride).ceil() as usize).min(self.rows);
        first.min(last)..last
    }

    pub fn row_items(&self, row: usize) -> Range<usize> {
        let start = (row * self.columns).min(self.count);
        let end = ((row + 1) * self.columns).min(self.count);
        start..end
    }

    pub fn cell_of(&self, index: usize) -> (usize, usize) {
        (index / self.columns, index % self.columns)
    }

    pub fn cell_offset(&self, index: usize) -> (f32, f32) {
        let (row, col) = self.cell_of(index);
        (col as f32 * (self.cell_width + self.spacing), row as f32 * self.row_stride())
    }

    pub fn scroll_to_show(&self, index: usize, offset: f32, viewport: f32) -> Option<f32> {
        if index >= self.count {
            return None;
        }
        let (_, y) = self.cell_offset(index);
        let bottom = y + self.cell_height;
        if y < offset {
            Some(y)
        } else if bottom > offset + viewport {
            Some((bottom - viewport).max(0.0))
        } else {
            None
        }
    }

    pub fn center_offset(&self, index: usize, viewport: f32) -> f32 {
        let (_, y) = self.cell_offset(index.min(self.count.saturating_sub(1)));
        let max = (self.content_height() - viewport).max(0.0);
        (y + self.cell_height * 0.5 - viewport * 0.5).clamp(0.0, max)
    }

    pub fn step(&self, index: usize, dx: isize, dy: isize) -> usize {
        if self.count == 0 {
            return 0;
        }
        let cols = self.columns as isize;
        let target = index as isize + dx + dy * cols;
        target.clamp(0, self.count as isize - 1) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_and_rows() {
        let g = GridLayout::new(420.0, 96.0, 14.0, 4.0, 10);
        assert_eq!(g.columns, 4);
        assert_eq!(g.rows, 3);
        assert!((g.cell_width - 102.0).abs() < 1e-3);
        assert!((g.cell_height - (96.0 + 6.0 + 14.0)).abs() < 1e-3);
        let g = GridLayout::new(419.0, 96.0, 14.0, 4.0, 10);
        assert_eq!(g.columns, 3);
        assert_eq!(g.rows, 4);
        assert!(g.cell_width > 102.0);
        let g = GridLayout::new(20.0, 96.0, 14.0, 4.0, 3);
        assert_eq!(g.columns, 1);
        assert_eq!(g.rows, 3);
        let g = GridLayout::new(500.0, 64.0, 14.0, 4.0, 0);
        assert_eq!(g.rows, 0);
        assert_eq!(g.content_height(), 0.0);
        assert_eq!(g.visible_rows(0.0, 500.0), 0..0);
    }

    #[test]
    fn visible_rows_only() {
        let g = GridLayout::new(420.0, 96.0, 14.0, 4.0, 40_000);
        assert_eq!(g.rows, 10_000);
        let stride = g.row_stride();
        assert_eq!(g.visible_rows(0.0, stride * 2.5), 0..3);
        assert_eq!(g.visible_rows(stride * 100.0, stride * 101.0), 100..101);
        assert_eq!(g.visible_rows(stride * 100.5, stride * 102.2), 100..103);
        assert_eq!(g.visible_rows(stride * 9_999.5, stride * 20_000.0), 9_999..10_000);
        assert_eq!(g.visible_rows(5.0, 5.0), 0..0);
        assert_eq!(g.row_items(9_999), 39_996..40_000);
        let g2 = GridLayout::new(420.0, 96.0, 14.0, 4.0, 10);
        assert_eq!(g2.row_items(2), 8..10);
        assert_eq!(g2.row_items(5), 10..10);
    }

    #[test]
    fn scrolling_and_steps() {
        let g = GridLayout::new(420.0, 96.0, 14.0, 4.0, 100);
        let stride = g.row_stride();
        assert_eq!(g.scroll_to_show(0, 0.0, 300.0), None);
        let off = g.scroll_to_show(40, 0.0, 300.0).unwrap();
        assert!((off - (10.0 * stride + g.cell_height - 300.0)).abs() < 1e-3);
        assert_eq!(g.scroll_to_show(4, stride * 5.0, 300.0), Some(stride));
        assert_eq!(g.scroll_to_show(1000, 0.0, 300.0), None);
        assert_eq!(g.step(5, 1, 0), 6);
        assert_eq!(g.step(5, 0, 1), 9);
        assert_eq!(g.step(1, 0, -1), 0);
        assert_eq!(g.step(98, 0, 1), 99);
        assert_eq!(g.cell_of(9), (2, 1));
        assert_eq!(g.center_offset(0, 300.0), 0.0);
        let mid = g.center_offset(48, 300.0);
        assert!((mid - (12.0 * stride + g.cell_height * 0.5 - 150.0)).abs() < 1e-3);
        assert!((g.center_offset(99, 300.0) - (g.content_height() - 300.0)).abs() < 1e-3);
    }
}
