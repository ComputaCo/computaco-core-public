//! PNG encode/decode and XWD (X Window Dump) parsing, into straight RGBA.
use crate::util::{err, Result};
use cw_fidelity::Image;

pub fn encode_png(img: &Image) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, img.width, img.height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let mut w = enc.write_header().map_err(|e| e.to_string())?;
        w.write_image_data(&img.rgba).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

pub fn decode_png(bytes: &[u8]) -> Result<Image> {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let (w, h) = (info.width, info.height);
    let n = (w * h) as usize;
    let rgba = match (info.color_type, info.bit_depth) {
        (png::ColorType::Rgba, png::BitDepth::Eight) => buf[..n * 4].to_vec(),
        (png::ColorType::Rgb, png::BitDepth::Eight) => {
            let mut v = Vec::with_capacity(n * 4);
            for p in buf[..n * 3].as_chunks::<3>().0 {
                v.extend_from_slice(&[p[0], p[1], p[2], 255]);
            }
            v
        }
        (png::ColorType::Grayscale, png::BitDepth::Eight) => {
            buf[..n].iter().flat_map(|&g| [g, g, g, 255]).collect()
        }
        (png::ColorType::GrayscaleAlpha, png::BitDepth::Eight) => buf[..n * 2]
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        (c, d) => return err(format!("unsupported PNG {c:?} {d:?}")),
    };
    Ok(Image::new(w, h, rgba))
}

fn be32(b: &[u8], i: usize) -> u32 {
    u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}

/// Parse an XWD file as `xwd -root` writes it (X11 header version 7, ZPixmap). Handles
/// 24 and 32 bits per pixel in either byte order, honouring the colour masks.
pub fn parse_xwd(bytes: &[u8]) -> Result<Image> {
    if bytes.len() < 100 {
        return err("xwd: short header");
    }
    let header_size = be32(bytes, 0) as usize;
    let version = be32(bytes, 4);
    if version != 7 {
        return err(format!("xwd: unsupported version {version}"));
    }
    let format = be32(bytes, 8);
    if format != 2 {
        return err(format!("xwd: not ZPixmap ({format})"));
    }
    let width = be32(bytes, 16);
    let height = be32(bytes, 20);
    // Header fields are 32-bit big-endian: 24 xoffset, 28 byte_order, 32 bitmap_unit,
    // 36 bit_order, 40 bitmap_pad, 44 bits_per_pixel, 48 bytes_per_line, 52 visual_class,
    // 56 red_mask, 60 green_mask, 64 blue_mask, 68 bits_per_rgb, 72 colormap_entries,
    // 76 ncolors, then the window geometry; the window name follows to `header_size`.
    let byte_order = be32(bytes, 28); // 0 LSBFirst, 1 MSBFirst
    let bits_per_pixel = be32(bytes, 44);
    let bytes_per_line = be32(bytes, 48) as usize;
    let red_mask = be32(bytes, 56);
    let green_mask = be32(bytes, 60);
    let blue_mask = be32(bytes, 64);
    let ncolors = be32(bytes, 76) as usize;
    let data_start = header_size + ncolors * 12;
    if bytes.len() < data_start + bytes_per_line * height as usize {
        return err("xwd: short pixel data");
    }
    let bpp = (bits_per_pixel / 8) as usize;
    if bpp != 3 && bpp != 4 {
        return err(format!("xwd: unsupported {bits_per_pixel} bits per pixel"));
    }
    let shift = |m: u32| if m == 0 { 0 } else { m.trailing_zeros() };
    let width_of = |m: u32| {
        if m == 0 {
            8
        } else {
            (m >> shift(m)).count_ones()
        }
    };
    let (rs, gs, bs) = (shift(red_mask), shift(green_mask), shift(blue_mask));
    let (rw, gw, bw) = (
        width_of(red_mask),
        width_of(green_mask),
        width_of(blue_mask),
    );
    let scale = |v: u32, w: u32| {
        if w >= 8 {
            (v >> (w - 8)) as u8
        } else {
            ((v << (8 - w)) | (v >> (2 * w).saturating_sub(8))) as u8
        }
    };
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height as usize {
        let row = &bytes[data_start + y * bytes_per_line..];
        for x in 0..width as usize {
            let p = &row[x * bpp..x * bpp + bpp];
            let v = match (bpp, byte_order) {
                (4, 0) => u32::from_le_bytes([p[0], p[1], p[2], p[3]]),
                (4, _) => u32::from_be_bytes([p[0], p[1], p[2], p[3]]),
                (3, 0) => u32::from_le_bytes([p[0], p[1], p[2], 0]),
                (_, _) => u32::from_be_bytes([0, p[0], p[1], p[2]]),
            };
            let r = if red_mask == 0 {
                (v >> 16) as u8
            } else {
                scale((v & red_mask) >> rs, rw)
            };
            let g = if green_mask == 0 {
                (v >> 8) as u8
            } else {
                scale((v & green_mask) >> gs, gw)
            };
            let b = if blue_mask == 0 {
                v as u8
            } else {
                scale((v & blue_mask) >> bs, bw)
            };
            rgba.extend_from_slice(&[r, g, b, 255]);
        }
    }
    Ok(Image::new(width, height, rgba))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn png_round_trip() {
        let img = Image::new(3, 2, (0..24).collect());
        let bytes = encode_png(&img).unwrap();
        assert_eq!(decode_png(&bytes).unwrap(), img);
    }
    #[test]
    fn xwd_minimal() {
        // One 2×1 ZPixmap, 32 bpp, LSBFirst, 0x00RRGGBB masks.
        let mut h = vec![0u8; 100];
        h[..4].copy_from_slice(&100u32.to_be_bytes());
        h[4..8].copy_from_slice(&7u32.to_be_bytes());
        h[8..12].copy_from_slice(&2u32.to_be_bytes());
        h[16..20].copy_from_slice(&2u32.to_be_bytes());
        h[20..24].copy_from_slice(&1u32.to_be_bytes());
        h[28..32].copy_from_slice(&0u32.to_be_bytes());
        h[44..48].copy_from_slice(&32u32.to_be_bytes());
        h[48..52].copy_from_slice(&8u32.to_be_bytes());
        h[56..60].copy_from_slice(&0xff0000u32.to_be_bytes());
        h[60..64].copy_from_slice(&0x00ff00u32.to_be_bytes());
        h[64..68].copy_from_slice(&0x0000ffu32.to_be_bytes());
        h.extend_from_slice(&0x00112233u32.to_le_bytes());
        h.extend_from_slice(&0x00ffffffu32.to_le_bytes());
        let img = parse_xwd(&h).unwrap();
        assert_eq!(img.px(0, 0), [0x11, 0x22, 0x33, 255]);
        assert_eq!(img.px(1, 0), [255, 255, 255, 255]);
    }
}
