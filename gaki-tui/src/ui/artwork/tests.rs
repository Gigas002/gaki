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
