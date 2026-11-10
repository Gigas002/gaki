use super::{cover_resize, halfblocks};
use crate::utils::image::ImgBuf;
use ratatui::{buffer::Buffer, layout::Rect, style::Color};

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
fn halfblocks_paint_every_cell_in_their_area() {
    let area = Rect::new(0, 0, 4, 3);
    let mut buf = Buffer::empty(area);
    halfblocks(&img(4, 6), area, &mut buf);
    for y in 0..area.height {
        for x in 0..area.width {
            let cell = buf.cell((x, y)).unwrap();
            // A flat colour gives identical halves: a blank cell whose
            // background is the art, so text drawn on top keeps it.
            assert_eq!(cell.bg, Color::Rgb(10, 20, 30), "cell ({x}, {y})");
        }
    }
}

#[test]
fn halfblocks_put_the_brighter_pixel_in_the_foreground() {
    let (dark, light) = (
        image::Rgba([0, 0, 0, 255]),
        image::Rgba([250, 250, 250, 255]),
    );
    let mut art = ImgBuf::new(2, 2);
    // Column 0: light over dark; column 1: dark over light.
    art.put_pixel(0, 0, light);
    art.put_pixel(0, 1, dark);
    art.put_pixel(1, 0, dark);
    art.put_pixel(1, 1, light);
    let area = Rect::new(3, 2, 2, 1);
    let mut buf = Buffer::empty(Rect::new(0, 0, 8, 4));
    halfblocks(&art, area, &mut buf);

    let light = Color::Rgb(250, 250, 250);
    let dark = Color::Rgb(0, 0, 0);
    let upper = buf.cell((3, 2)).unwrap();
    assert_eq!((upper.symbol(), upper.fg, upper.bg), ("▀", light, dark));
    let lower = buf.cell((4, 2)).unwrap();
    assert_eq!((lower.symbol(), lower.fg, lower.bg), ("▄", light, dark));
    assert_eq!(buf.cell((0, 0)).unwrap().symbol(), " ", "outside the area");
}

#[test]
fn halfblocks_stop_at_the_buffer_and_the_image() {
    // An image smaller than the area, in a buffer smaller than the area.
    let area = Rect::new(0, 0, 6, 4);
    let mut buf = Buffer::empty(Rect::new(0, 0, 3, 2));
    halfblocks(&img(2, 3), area, &mut buf);
    assert_eq!(buf.cell((0, 1)).unwrap().bg, Color::Rgb(10, 20, 30));
    assert_eq!(buf.cell((2, 0)).unwrap().bg, Color::Reset, "past the image");
}
