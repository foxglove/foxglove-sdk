// Remote access example: streams synthetic camera feeds through the Foxglove
// gateway. Produces a "test card" stream designed to make network degradation
// (latency, packet loss, low bandwidth) visually obvious, plus a simple
// scrolling gradient for a secondary feed.

use foxglove::{
    ChannelDescriptor,
    bytes::Bytes,
    remote_access::{Capability, Client, Gateway, Listener},
    schemas::{CameraCalibration, RawImage, Timestamp},
};
use serde_json::Value;
use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

struct MessageHandler;
impl Listener for MessageHandler {
    /// Called when a connected app publishes a message, such as from the Teleop panel.
    fn on_message_data(&self, client: Client, channel: &ChannelDescriptor, message: &[u8]) {
        let json = serde_json::from_slice::<Value>(message).expect("Failed to parse message");
        println!(
            "Teleop message from {} on topic {}: {json}",
            client.id(),
            channel.topic()
        );
    }
}

#[tokio::main]
async fn main() {
    let env = env_logger::Env::default().default_filter_or("info");
    env_logger::init_from_env(env);

    // Open a gateway for remote visualization and teleop.
    let handle = Gateway::new()
        .capabilities([Capability::ClientPublish])
        .supported_encodings(["json"])
        .listener(Arc::new(MessageHandler))
        .start()
        .expect("Failed to start remote access gateway");

    tokio::task::spawn(camera_loop());
    _ = tokio::signal::ctrl_c().await;
    _ = handle.stop().await;
}

// ---------------------------------------------------------------------------
// Test card: designed to make network degradation visually obvious.
//
// Layout (960x540):
//   +---------------------------+------------------+
//   |                           |  Frame: 00042    |
//   |    Sweeping clock hand    |  12:34:56.789    |
//   |    (1 revolution/min)     |                  |
//   |                           +------------------+
//   |                           |  ############### |
//   |                           |  # Checkerboard# |
//   +---------------------------+------------------+
//   |  <<<< scrolling ticker >>>>                  |
//   +----------------------------------------------+
//
// - Clock hand: smooth rotation makes frame drops visible as jumps.
// - Frame counter: sequential numbers reveal dropped frames.
// - Timestamp (UTC): compare with wall clock to estimate latency.
// - Checkerboard: high-frequency edges reveal compression artifacts.
// - Scrolling ticker: smooth motion becomes jerky with frame drops.
// ---------------------------------------------------------------------------

const WIDTH: usize = 960;
const HEIGHT: usize = 540;
const FPS: u32 = 30;

/// 5x7 bitmap font. Each glyph is stored as 7 rows of 5-bit patterns
/// (MSB = leftmost pixel). The full alphabet is defined so the font table
/// is reusable even though only a subset of glyphs are currently used.
const GLYPH_W: usize = 5;
const GLYPH_H: usize = 7;

fn glyph_rows(ch: u8) -> [u8; GLYPH_H] {
    match ch {
        // Digits.
        b'0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        b'1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        b'2' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
        b'3' => [0x0E, 0x11, 0x01, 0x06, 0x01, 0x11, 0x0E],
        b'4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        b'5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
        b'6' => [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        b'7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        b'8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        b'9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
        // Punctuation.
        b':' => [0x00, 0x04, 0x04, 0x00, 0x04, 0x04, 0x00],
        b'.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x0C, 0x0C],
        b' ' => [0x00; GLYPH_H],
        // Uppercase.
        b'A' => [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        b'B' => [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E],
        b'C' => [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E],
        b'D' => [0x1E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1E],
        b'E' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
        b'F' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10],
        b'G' => [0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0F],
        b'H' => [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        b'I' => [0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E],
        b'J' => [0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0C],
        b'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        b'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F],
        b'M' => [0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11],
        b'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        b'O' => [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        b'P' => [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
        b'Q' => [0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D],
        b'R' => [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11],
        b'S' => [0x0E, 0x11, 0x10, 0x0E, 0x01, 0x11, 0x0E],
        b'T' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        b'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        b'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04],
        b'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x1B, 0x11],
        b'X' => [0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11],
        b'Y' => [0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04],
        b'Z' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F],
        // Lowercase.
        b'a' => [0x00, 0x00, 0x0E, 0x01, 0x0F, 0x11, 0x0F],
        b'b' => [0x10, 0x10, 0x1E, 0x11, 0x11, 0x11, 0x1E],
        b'c' => [0x00, 0x00, 0x0E, 0x11, 0x10, 0x11, 0x0E],
        b'd' => [0x01, 0x01, 0x0F, 0x11, 0x11, 0x11, 0x0F],
        b'e' => [0x00, 0x00, 0x0E, 0x11, 0x1F, 0x10, 0x0E],
        b'f' => [0x06, 0x09, 0x08, 0x1C, 0x08, 0x08, 0x08],
        b'g' => [0x00, 0x00, 0x0F, 0x11, 0x0F, 0x01, 0x0E],
        b'h' => [0x10, 0x10, 0x16, 0x19, 0x11, 0x11, 0x11],
        b'i' => [0x04, 0x00, 0x0C, 0x04, 0x04, 0x04, 0x0E],
        b'j' => [0x02, 0x00, 0x06, 0x02, 0x02, 0x12, 0x0C],
        b'k' => [0x10, 0x10, 0x12, 0x14, 0x18, 0x14, 0x12],
        b'l' => [0x0C, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E],
        b'm' => [0x00, 0x00, 0x1A, 0x15, 0x15, 0x15, 0x15],
        b'n' => [0x00, 0x00, 0x16, 0x19, 0x11, 0x11, 0x11],
        b'o' => [0x00, 0x00, 0x0E, 0x11, 0x11, 0x11, 0x0E],
        b'p' => [0x00, 0x00, 0x1E, 0x11, 0x1E, 0x10, 0x10],
        b'q' => [0x00, 0x00, 0x0F, 0x11, 0x0F, 0x01, 0x01],
        b'r' => [0x00, 0x00, 0x16, 0x19, 0x10, 0x10, 0x10],
        b's' => [0x00, 0x00, 0x0E, 0x10, 0x0E, 0x01, 0x1E],
        b't' => [0x08, 0x08, 0x1C, 0x08, 0x08, 0x09, 0x06],
        b'u' => [0x00, 0x00, 0x11, 0x11, 0x11, 0x13, 0x0D],
        b'v' => [0x00, 0x00, 0x11, 0x11, 0x11, 0x0A, 0x04],
        b'w' => [0x00, 0x00, 0x11, 0x11, 0x15, 0x15, 0x0A],
        b'x' => [0x00, 0x00, 0x11, 0x0A, 0x04, 0x0A, 0x11],
        b'y' => [0x00, 0x00, 0x11, 0x11, 0x0F, 0x01, 0x0E],
        b'z' => [0x00, 0x00, 0x1F, 0x02, 0x04, 0x08, 0x1F],
        _ => [0x1F; GLYPH_H], // Filled block for unknown.
    }
}

/// RGB color for drawing primitives.
#[derive(Clone, Copy)]
struct Rgb(u8, u8, u8);

/// Set a single pixel in the buffer (bounds-checked).
fn set_pixel(buf: &mut [u8], x: i32, y: i32, color: Rgb) {
    if x >= 0 && (x as usize) < WIDTH && y >= 0 && (y as usize) < HEIGHT {
        let off = y as usize * (WIDTH * 3) + x as usize * 3;
        buf[off] = color.0;
        buf[off + 1] = color.1;
        buf[off + 2] = color.2;
    }
}

/// Draw a string at (x, y) with a given scale factor into an RGB buffer.
/// x is signed to support partially off-screen text (e.g., scrolling ticker).
fn draw_text(buf: &mut [u8], text: &str, x: i32, y: i32, scale: usize, color: Rgb) {
    let char_stride = (GLYPH_W + 1) * scale;
    for (ci, ch) in text.bytes().enumerate() {
        let char_x = x + (ci * char_stride) as i32;
        if char_x + (GLYPH_W * scale) as i32 <= 0 {
            continue;
        }
        if char_x >= WIDTH as i32 {
            break;
        }
        let rows = glyph_rows(ch);
        for (row_idx, &row_bits) in rows.iter().enumerate() {
            for col in 0..GLYPH_W {
                if row_bits & (1 << (GLYPH_W - 1 - col)) != 0 {
                    for sy in 0..scale {
                        for sx in 0..scale {
                            let px = char_x + (col * scale + sx) as i32;
                            let py = y + (row_idx * scale + sy) as i32;
                            set_pixel(buf, px, py, color);
                        }
                    }
                }
            }
        }
    }
}

/// Draw a line from (x0, y0) to (x1, y1) using Bresenham's algorithm.
fn draw_line(buf: &mut [u8], x0: i32, y0: i32, x1: i32, y1: i32, color: Rgb) {
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut cx = x0;
    let mut cy = y0;
    loop {
        set_pixel(buf, cx, cy, color);
        if cx == x1 && cy == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            cx += sx;
        }
        if e2 <= dx {
            err += dx;
            cy += sy;
        }
    }
}

/// Draw a thick line (multiple parallel Bresenham lines within a radius).
fn draw_thick_line(buf: &mut [u8], x0: i32, y0: i32, x1: i32, y1: i32, thickness: i32, color: Rgb) {
    for dx in -thickness..=thickness {
        for dy in -thickness..=thickness {
            if dx * dx + dy * dy <= thickness * thickness {
                draw_line(buf, x0 + dx, y0 + dy, x1 + dx, y1 + dy, color);
            }
        }
    }
}

/// Fill a circle at (cx, cy) with the given radius.
fn fill_circle(buf: &mut [u8], cx: i32, cy: i32, radius: i32, color: Rgb) {
    for py in (cy - radius)..=(cy + radius) {
        for px in (cx - radius)..=(cx + radius) {
            let dx = px - cx;
            let dy = py - cy;
            if dx * dx + dy * dy <= radius * radius {
                set_pixel(buf, px, py, color);
            }
        }
    }
}

/// Render one test card frame into an RGB buffer.
fn render_test_card(buf: &mut [u8], frame_number: u64) {
    // Background: dark gray.
    let bg = Rgb(30, 30, 30);
    for pixel in buf.chunks_exact_mut(3) {
        pixel[0] = bg.0;
        pixel[1] = bg.1;
        pixel[2] = bg.2;
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let total_secs = now.as_secs();
    let millis = now.subsec_millis();
    let hours = (total_secs / 3600) % 24;
    let minutes = (total_secs / 60) % 60;
    let seconds = total_secs % 60;

    // --- Sweeping clock hand (left portion, 660x430) ---
    let clock_cx = 330i32;
    let clock_cy = 230i32;
    let clock_radius = 190i32;

    // Draw clock face circle outline.
    let dim = Rgb(80, 80, 80);
    for angle_deg in 0..360 {
        let angle = (angle_deg as f64).to_radians();
        let px = clock_cx + (clock_radius as f64 * angle.cos()) as i32;
        let py = clock_cy + (clock_radius as f64 * angle.sin()) as i32;
        set_pixel(buf, px, py, dim);
    }

    // Tick marks at each second.
    for tick in 0..60 {
        let angle = (tick as f64 * 6.0 - 90.0).to_radians();
        let inner = if tick % 5 == 0 {
            clock_radius - 20
        } else {
            clock_radius - 10
        };
        let x0 = clock_cx + (inner as f64 * angle.cos()) as i32;
        let y0 = clock_cy + (inner as f64 * angle.sin()) as i32;
        let x1 = clock_cx + (clock_radius as f64 * angle.cos()) as i32;
        let y1 = clock_cy + (clock_radius as f64 * angle.sin()) as i32;
        draw_line(buf, x0, y0, x1, y1, Rgb(100, 100, 100));
    }

    // Clock hand: rotates once per minute, with sub-second smoothness.
    let frac = seconds as f64 + millis as f64 / 1000.0;
    let hand_angle = (frac * 6.0 - 90.0).to_radians(); // 6 degrees per second = 360 per minute
    let hand_len = clock_radius - 30;
    let hx = clock_cx + (hand_len as f64 * hand_angle.cos()) as i32;
    let hy = clock_cy + (hand_len as f64 * hand_angle.sin()) as i32;
    let cyan = Rgb(0, 200, 255);
    draw_thick_line(buf, clock_cx, clock_cy, hx, hy, 2, cyan);

    // Center dot.
    fill_circle(buf, clock_cx, clock_cy, 5, cyan);

    // --- Info panel (top-right, starting at x=680) ---
    let panel_x = 690;

    // Frame counter.
    let frame_str = format!("Frame:{:06}", frame_number);
    draw_text(buf, &frame_str, panel_x, 30, 3, Rgb(255, 255, 255));

    // Timestamp (UTC).
    let time_str = format!("{:02}:{:02}:{:02}.{:03}", hours, minutes, seconds, millis);
    draw_text(buf, &time_str, panel_x, 80, 3, Rgb(255, 200, 0));

    // --- Checkerboard (bottom-right, 260x270 starting at x=690, y=160) ---
    let check_x: usize = 690;
    let check_y: usize = 160;
    let check_w: usize = 260;
    let check_h: usize = 270;
    let cell_size: usize = 10;
    let stride = WIDTH * 3;
    for cy_off in 0..check_h {
        let row_start = (check_y + cy_off) * stride + check_x * 3;
        let row_cell = cy_off / cell_size;
        for cx_off in 0..check_w {
            // TODO: replace with .is_multiple_of(2) when MSRV >= 1.93.
            #[allow(clippy::manual_is_multiple_of)]
            let is_white = ((cx_off / cell_size) + row_cell) % 2 == 0;
            let val = if is_white { 220u8 } else { 35u8 };
            let off = row_start + cx_off * 3;
            buf[off] = val;
            buf[off + 1] = val;
            buf[off + 2] = val;
        }
    }

    // --- Scrolling ticker (bottom 80px) ---
    let ticker_y = HEIGHT - 80;
    let ticker_text = "    FOXGLOVE NETWORK TEST CARD \
        :::  Frame drops appear as jumps in the clock hand  :::  \
        Latency visible by comparing timestamp to wall clock  :::  \
        Compression artifacts visible in checkerboard  :::  \
        Frame counter gaps reveal dropped frames  :::  ";

    // Scroll speed: 3 pixels per frame.
    let text_pixel_width = ticker_text.len() * (GLYPH_W + 1) * 3;
    let scroll_offset = (frame_number as usize * 3) % text_pixel_width;

    // Draw ticker background.
    for y in ticker_y..HEIGHT {
        let row_start = y * stride;
        for x in 0..WIDTH {
            let off = row_start + x * 3;
            buf[off] = 20;
            buf[off + 1] = 20;
            buf[off + 2] = 50;
        }
    }

    // Draw ticker text twice (for seamless wrap) shifted by scroll offset.
    let ty = ticker_y + 25;
    let text_scale = 3;
    let x_start = -(scroll_offset as i32);
    for pass in 0..2i32 {
        let base_x = x_start + pass * text_pixel_width as i32;
        if base_x < WIDTH as i32 && base_x + text_pixel_width as i32 > 0 {
            draw_text(
                buf,
                ticker_text,
                base_x,
                ty as i32,
                text_scale,
                Rgb(200, 200, 255),
            );
        }
    }
}

/// Log RawImage messages that are encoded as video streams by the remote access
/// gateway. Produces a test card on `/camera/rgb` and a scrolling gradient on
/// `/camera/mono`.
async fn camera_loop() {
    let mut interval = tokio::time::interval(Duration::from_millis(1000 / FPS as u64));
    let mut frame_number: u64 = 0;
    let mut rgb_buf = vec![0u8; WIDTH * HEIGHT * 3];
    let mut mono_buf = vec![0u8; WIDTH * HEIGHT];

    // Pre-compute a double-width gradient lookup table so we can take a
    // width-sized slice at any offset without per-pixel math each frame.
    let mono_gradient: Vec<u8> = (0..WIDTH * 2)
        .map(|x| ((x % WIDTH) * 255 / WIDTH) as u8)
        .collect();

    let calibration = CameraCalibration {
        timestamp: None,
        frame_id: "camera".into(),
        width: WIDTH as u32,
        height: HEIGHT as u32,
        distortion_model: String::new(),
        d: vec![],
        k: vec![
            500.0,
            0.0,
            WIDTH as f64 / 2.0,
            0.0,
            500.0,
            HEIGHT as f64 / 2.0,
            0.0,
            0.0,
            1.0,
        ],
        r: vec![],
        p: vec![
            500.0,
            0.0,
            WIDTH as f64 / 2.0,
            0.0,
            0.0,
            500.0,
            HEIGHT as f64 / 2.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
        ],
    };

    loop {
        interval.tick().await;

        // Test card: clock hand, frame counter, timestamp, checkerboard, ticker.
        render_test_card(&mut rgb_buf, frame_number);
        let rgb_img = RawImage {
            timestamp: Some(Timestamp::now()),
            frame_id: "camera".into(),
            width: WIDTH as u32,
            height: HEIGHT as u32,
            encoding: "rgb8".into(),
            step: (WIDTH * 3) as u32,
            data: Bytes::copy_from_slice(&rgb_buf),
        };
        foxglove::log!("/camera/rgb", rgb_img);

        // Mono image: scrolling gradient (simple secondary stream).
        let offset = frame_number as usize % WIDTH;
        let mono_row = &mono_gradient[offset..offset + WIDTH];
        for row in mono_buf.chunks_exact_mut(WIDTH) {
            row.copy_from_slice(mono_row);
        }
        let mono_img = RawImage {
            timestamp: Some(Timestamp::now()),
            frame_id: "camera".into(),
            width: WIDTH as u32,
            height: HEIGHT as u32,
            encoding: "mono8".into(),
            step: WIDTH as u32,
            data: Bytes::copy_from_slice(&mono_buf),
        };
        foxglove::log!("/camera/mono", mono_img);

        foxglove::log!("/camera/info", calibration.clone());

        frame_number += 1;
    }
}
