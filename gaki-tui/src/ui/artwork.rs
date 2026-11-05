//! Game artwork in the detail pane: the faded background art behind the
//! details and the portrait cover above them. Both are drawn as half-block
//! cells, so the detail text drawn over them stays readable.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui_image::Image;
use ratatui_image::protocol::{Protocol, halfblocks::Halfblocks};

use crate::theme;
use crate::utils::image::ImgBuf;

/// Paint the art for a detail pane whose inner (border-less) area is `inner`.
/// `background` fills the whole pane, faded toward the panel colour.
/// `cover` sits in the top-left corner, in the rows the `content_h` rows of
/// details leave free, when at least four are.
pub fn paint(
    frame: &mut Frame,
    inner: Rect,
    background: Option<&ImgBuf>,
    cover: Option<&ImgBuf>,
    content_h: u16,
) {
    if inner.width < 2 || inner.height < 1 {
        return;
    }
    if let Some(image) = background {
        let mut bg = cover_resize(image, inner.width as u32, inner.height as u32 * 2);
        fade_bottom(&mut bg, rgb(theme::BG_DARK));
        if let Some(art) = halfblock_art(bg, inner) {
            frame.render_widget(Image::new(&art), inner);
        }
    }

    let gap = inner.height.saturating_sub(content_h);
    if gap >= 4
        && let Some(image) = cover
        && image.width() > 0
        && image.height() > 0
    {
        // Width from the art's aspect: box_w px = 2*gap * W/H.
        let box_w =
            ((gap as u32 * 2 * image.width() / image.height()) as u16).clamp(1, inner.width);
        let thumb = cover_resize(image, box_w as u32, gap as u32 * 2);
        let area = Rect {
            x: inner.x,
            y: inner.y,
            width: box_w,
            height: gap,
        };
        if let Some(art) = halfblock_art(thumb, area) {
            frame.render_widget(Image::new(&art), area);
        }
    }
}

/// Blend `img` toward `bg` so the top-aligned cover art reads as a translucent
/// background: a uniform base blend keeps the overlaid text legible everywhere,
/// and the lower part fades further until it dissolves fully into the panel
/// background at the bottom row. Applied in source-pixel space, so it maps to
/// the same proportion of the rendered art regardless of later resizing.
fn fade_bottom(img: &mut ImgBuf, bg: (u8, u8, u8)) {
    let h = img.height();
    if h == 0 {
        return;
    }
    // How far every pixel is pulled toward the background (overall translucency).
    let base = 0.45f32;
    // Below this row the art fades the rest of the way to full background.
    let start = h * 9 / 20;
    let span = (h - start).max(1) as f32;
    let (br, bgc, bb) = bg;
    for y in 0..h {
        let t = if y < start {
            base
        } else {
            base + (1.0 - base) * ((y - start) as f32 / span)
        };
        for x in 0..img.width() {
            let px = img.get_pixel_mut(x, y);
            px[0] = (px[0] as f32 * (1.0 - t) + br as f32 * t) as u8;
            px[1] = (px[1] as f32 * (1.0 - t) + bgc as f32 * t) as u8;
            px[2] = (px[2] as f32 * (1.0 - t) + bb as f32 * t) as u8;
        }
    }
}

/// Scale `src` to fully cover a `tw`×`th` pixel target, preserving its aspect
/// ratio by centre-cropping the overflow (`object-fit: cover`): the result is
/// exactly `tw`×`th`, with the excess trimmed off the top/bottom or left/right.
fn cover_resize(src: &ImgBuf, tw: u32, th: u32) -> ImgBuf {
    use image::imageops::{FilterType, crop_imm, resize};
    let (sw, sh) = (src.width(), src.height());
    if sw == 0 || sh == 0 || tw == 0 || th == 0 {
        return src.clone();
    }
    // Largest centred sub-rectangle of `src` matching the target aspect ratio.
    // `sw/sh` vs `tw/th` compared as cross-products to stay in integers.
    let (cw, ch) = if sw as u64 * th as u64 >= sh as u64 * tw as u64 {
        // Source is wider than the target — crop left/right (full height kept).
        let cw = ((sh as u64 * tw as u64) / th as u64) as u32;
        (cw.clamp(1, sw), sh)
    } else {
        // Source is taller than the target — crop top/bottom (full width kept).
        let ch = ((sw as u64 * th as u64) / tw as u64) as u32;
        (sw, ch.clamp(1, sh))
    };
    let ox = (sw - cw) / 2;
    let oy = (sh - ch) / 2;
    let cropped = crop_imm(src, ox, oy, cw, ch).to_image();
    resize(&cropped, tw, th, FilterType::Triangle)
}

/// Encode `img` (already sized to `area`: one pixel per column, two per row) as
/// half-block cells. Half-blocks colour each cell's foreground and background,
/// so the detail text drawn on top keeps the art behind it; graphics protocols
/// (kitty, sixel) would hide that text.
fn halfblock_art(img: image::RgbaImage, area: Rect) -> Option<Protocol> {
    Halfblocks::new(image::DynamicImage::ImageRgba8(img), area)
        .ok()
        .map(Protocol::Halfblocks)
}

/// The RGB components of a theme [`ratatui::style::Color::Rgb`] (falls back to black).
fn rgb(color: ratatui::style::Color) -> (u8, u8, u8) {
    match color {
        ratatui::style::Color::Rgb(r, g, b) => (r, g, b),
        _ => (0, 0, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::{cover_resize, halfblock_art};
    use crate::utils::image::ImgBuf;
    use ratatui::{buffer::Buffer, layout::Rect, style::Color, widgets::Widget};

    fn img(w: u32, h: u32) -> ImgBuf {
        image::ImageBuffer::from_pixel(w, h, image::Rgba([10, 20, 30, 255]))
    }

    #[test]
    fn cover_resize_fills_exact_target_dimensions() {
        // Wide source into a taller target -> crops left/right, fills exactly.
        let out = cover_resize(&img(200, 100), 20, 40);
        assert_eq!((out.width(), out.height()), (20, 40));

        // Tall source into a wider target -> crops top/bottom, fills exactly.
        let out = cover_resize(&img(100, 200), 40, 20);
        assert_eq!((out.width(), out.height()), (40, 20));

        // Source already at the target aspect -> still the exact target size.
        let out = cover_resize(&img(80, 160), 10, 20);
        assert_eq!((out.width(), out.height()), (10, 20));
    }

    #[test]
    fn cover_resize_returns_clone_on_degenerate_input() {
        let zero = ImgBuf::new(0, 0);
        let out = cover_resize(&zero, 10, 10);
        assert_eq!((out.width(), out.height()), (0, 0));
    }

    #[test]
    fn halfblock_art_paints_every_cell_in_its_area() {
        let area = Rect::new(0, 0, 4, 3);
        let art = halfblock_art(img(4, 6), area).expect("encodes");
        let mut buf = Buffer::empty(area);
        ratatui_image::Image::new(&art).render(area, &mut buf);
        for y in 0..area.height {
            for x in 0..area.width {
                let cell = buf.cell((x, y)).unwrap();
                // A flat colour gives identical halves: a blank cell whose
                // background is the art, so text drawn on top keeps it.
                assert_eq!(cell.bg, Color::Rgb(10, 20, 30), "cell ({x}, {y})");
            }
        }
    }
}
