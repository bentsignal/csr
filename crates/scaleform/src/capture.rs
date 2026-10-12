//! Full-resolution readback byte handling. Flash output uses premultiplied
//! sRGB bytes; Bevy's UI image expects straight-alpha bytes.

/// Strip transfer-row padding without copying any padding into the image.
pub(crate) fn copy_rows(
    mapped: &[u8],
    width: u32,
    height: u32,
    stride: usize,
) -> Result<Vec<u8>, &'static str> {
    let row = (width as usize)
        .checked_mul(4)
        .ok_or("capture row overflow")?;
    let bytes = row
        .checked_mul(height as usize)
        .ok_or("capture size overflow")?;
    let mapped_bytes = stride
        .checked_mul(height as usize)
        .ok_or("capture stride overflow")?;
    if width == 0 || height == 0 || stride < row || mapped.len() < mapped_bytes {
        return Err("invalid capture dimensions or mapped buffer");
    }
    let mut out = Vec::with_capacity(bytes);
    for source in mapped[..mapped_bytes].chunks_exact(stride) {
        out.extend_from_slice(&source[..row]);
    }
    Ok(out)
}

/// Preserve Ruffle's existing float conversion exactly for partial alpha.
/// Division by one changes no byte, so opaque pixels need no arithmetic.
/// Zero-alpha RGB is retained, matching the existing capture contract.
pub(crate) fn straight_alpha(rgba: &mut [u8]) {
    for pixel in rgba.chunks_exact_mut(4) {
        let alpha = pixel[3];
        if alpha != 0 && alpha != 255 {
            let scale = f32::from(alpha) / 255.0;
            for channel in &mut pixel[..3] {
                *channel = (f32::from(*channel) / scale) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_fast_path_and_partial_pixels_match_existing_float_capture_exhaustively() {
        let mut actual = Vec::with_capacity(256 * 256 * 4);
        for alpha in 0..=255u8 {
            for channel in 0..=255u8 {
                actual.extend_from_slice(&[
                    channel,
                    255 - channel,
                    channel.wrapping_add(alpha),
                    alpha,
                ]);
            }
        }
        let mut expected = actual.clone();
        // The existing Ruffle readback's public straight-alpha formula.
        for pixel in expected.chunks_exact_mut(4) {
            if pixel[3] != 0 {
                let scale = f32::from(pixel[3]) / 255.0;
                pixel[0] = (f32::from(pixel[0]) / scale) as u8;
                pixel[1] = (f32::from(pixel[1]) / scale) as u8;
                pixel[2] = (f32::from(pixel[2]) / scale) as u8;
            }
        }
        straight_alpha(&mut actual);
        assert_eq!(actual, expected);
    }

    #[test]
    fn padded_transfer_rows_keep_pixel_order_and_exact_dimensions() {
        let first = [1, 2, 3, 255, 32, 16, 8, 64];
        let second = [90, 80, 70, 0, 3, 4, 5, 255];
        let mut mapped = vec![0xcc; 512];
        mapped[..8].copy_from_slice(&first);
        mapped[256..264].copy_from_slice(&second);
        let mut pixels = copy_rows(&mapped, 2, 2, 256).unwrap();
        assert_eq!(pixels, [first, second].concat());
        straight_alpha(&mut pixels);
        assert_eq!(&pixels[8..12], &[90, 80, 70, 0]);
        assert_eq!(&pixels[12..16], &[3, 4, 5, 255]);
        assert_eq!(pixels.len(), 2 * 2 * 4);
        assert!(!pixels.contains(&0xcc));
    }

    #[test]
    fn truncated_or_inconsistent_transfer_layout_fails_without_partial_image() {
        assert!(copy_rows(&[0; 511], 2, 2, 256).is_err());
        assert!(copy_rows(&[0; 512], 2, 2, 4).is_err());
        assert!(copy_rows(&[0; 512], 0, 2, 256).is_err());
        assert!(copy_rows(&[0; 512], 2, 0, 256).is_err());
        assert!(copy_rows(&[0; 512], 2, 2, usize::MAX).is_err());
    }
}
