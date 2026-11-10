//! The QR sign-in page draws a complete, scannable code: decoding the drawn
//! cells gives back exactly the QR matrix, quiet zone included, black on white.

use gaki_tui::ui::login;
use qrcode::{Color as Module, QrCode};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Color;

/// A challenge URL of the shape Steam issues.
const URL: &str = "https://s.team/q/1/2914470936914853312";

/// Draw the sign-in screen (QR pane above the 3-row help box) at `w`×`h`.
fn draw(qr: Option<&str>, w: u16, h: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal
        .draw(|f| {
            let pane = login::layout().split(f.area())[0];
            f.render_widget(login::qr(qr.map(str::to_string), pane), pane);
        })
        .unwrap();
    terminal.backend().buffer().clone()
}

fn text(buf: &Buffer) -> String {
    let area = buf.area;
    (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buf.cell((x, y)).unwrap().symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every dark module of the code, with the 4-module quiet zone, as a grid.
fn expected_modules(url: &str) -> Vec<Vec<bool>> {
    let code = QrCode::new(url.as_bytes()).unwrap();
    let (n, quiet) = (code.width(), 4);
    let colors = code.to_colors();
    let size = n + 2 * quiet;
    (0..size)
        .map(|y| {
            (0..size)
                .map(|x| {
                    let inside = (quiet..quiet + n).contains(&x) && (quiet..quiet + n).contains(&y);
                    inside && colors[(y - quiet) * n + (x - quiet)] == Module::Dark
                })
                .collect()
        })
        .collect()
}

/// Read the code back out of the buffer: find the black-on-white block, then
/// turn each cell's half-block glyph into its two modules.
fn decode(buf: &Buffer) -> Vec<Vec<bool>> {
    let area = buf.area;
    let qr_cells: Vec<(u16, u16)> = (0..area.height)
        .flat_map(|y| (0..area.width).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let cell = buf.cell((x, y)).unwrap();
            cell.fg == Color::Rgb(0, 0, 0) && cell.bg == Color::Rgb(255, 255, 255)
        })
        .collect();
    assert!(!qr_cells.is_empty(), "no black-on-white QR cells drawn");
    let (x0, x1) = (
        qr_cells.iter().map(|c| c.0).min().unwrap(),
        qr_cells.iter().map(|c| c.0).max().unwrap(),
    );
    let (y0, y1) = (
        qr_cells.iter().map(|c| c.1).min().unwrap(),
        qr_cells.iter().map(|c| c.1).max().unwrap(),
    );
    let mut rows = Vec::new();
    for y in y0..=y1 {
        let (mut top, mut bottom) = (Vec::new(), Vec::new());
        for x in x0..=x1 {
            let cell = buf.cell((x, y)).unwrap();
            assert_eq!(
                (cell.fg, cell.bg),
                (Color::Rgb(0, 0, 0), Color::Rgb(255, 255, 255)),
                "cell ({x},{y}) inside the code isn't black on white"
            );
            let (t, b) = match cell.symbol() {
                " " => (false, false),
                "▀" => (true, false),
                "▄" => (false, true),
                "█" => (true, true),
                other => panic!("unexpected glyph {other:?} at ({x},{y})"),
            };
            top.push(t);
            bottom.push(b);
        }
        rows.push(top);
        rows.push(bottom);
    }
    rows
}

#[test]
fn the_drawn_code_is_the_whole_qr_matrix() {
    // A standard 80×24 terminal, the tightest common size.
    let buf = draw(Some(URL), 80, 24);
    let mut expected = expected_modules(URL);
    let decoded = decode(&buf);
    // An odd module count leaves the last half-row empty (light).
    if expected.len() % 2 == 1 {
        expected.push(vec![false; expected[0].len()]);
    }
    assert_eq!(decoded.len(), expected.len(), "rows (quiet zone included)");
    assert_eq!(decoded[0].len(), expected[0].len(), "columns");
    assert!(
        decoded == expected,
        "drawn modules differ from the QR matrix"
    );
}

#[test]
fn a_new_challenge_redraws_the_code() {
    let rotated = "https://s.team/q/1/5050505050505050505";
    let mut expected = expected_modules(rotated);
    if expected.len() % 2 == 1 {
        expected.push(vec![false; expected[0].len()]);
    }
    assert!(decode(&draw(Some(rotated), 100, 40)) == expected);
}

#[test]
fn a_terminal_too_small_says_so_instead_of_clipping() {
    let buf = draw(Some(URL), 80, 18);
    let screen = text(&buf);
    assert!(
        screen.contains("needs a terminal of at least"),
        "size hint shown:\n{screen}"
    );
    let any_qr = (0..18).any(|y| {
        (0..80).any(|x| {
            let c = buf.cell((x, y)).unwrap();
            c.fg == Color::Rgb(0, 0, 0) && c.bg == Color::Rgb(255, 255, 255)
        })
    });
    assert!(!any_qr, "no partial code is drawn");
}

#[test]
fn waiting_for_the_first_challenge_shows_progress() {
    let screen = text(&draw(None, 80, 24));
    assert!(screen.contains("Generating QR code"));
    assert!(screen.contains("Scan with the Steam Mobile app"));
}
