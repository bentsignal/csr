//! The axis-aligned subset of authored VMT texture transforms. Coordinates are
//! transformed around `center`, then translated. Rotation, reflection, partial
//! declarations and matrix syntax remain unsupported rather than being guessed.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxisAlignedTextureTransform {
    /// `uv * xy + zw`, after folding the authored center into the offset.
    pub scale_offset: [f32; 4],
}

impl Default for AxisAlignedTextureTransform {
    fn default() -> Self {
        Self {
            scale_offset: [1.0, 1.0, 0.0, 0.0],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureTransformError {
    UnsupportedSyntax,
    UnsupportedRotation,
    UnsupportedReflection,
    NonFinite,
}

impl AxisAlignedTextureTransform {
    /// Accept a complete, finite `center x y scale x y rotate 0 translate x y`
    /// declaration, in any field order. An absent parameter is handled by the
    /// caller's identity default; a partial declaration is not that default.
    pub fn parse(text: &str) -> Result<Self, TextureTransformError> {
        use TextureTransformError::*;
        let tokens: Vec<_> = text.split_ascii_whitespace().take(12).collect();
        if tokens.len() != 11 {
            return Err(UnsupportedSyntax);
        }
        let (mut center, mut scale, mut rotation, mut translate) = (None, None, None, None);
        let mut i = 0;
        while i < tokens.len() {
            let field = tokens[i].to_ascii_lowercase();
            i += 1;
            let width = match field.as_str() {
                "center" | "scale" | "translate" => 2,
                "rotate" => 1,
                _ => return Err(UnsupportedSyntax),
            };
            let values = tokens.get(i..i + width).ok_or(UnsupportedSyntax)?;
            let mut parsed = [0.0; 2];
            for (j, value) in values.iter().enumerate() {
                parsed[j] = value.parse::<f32>().map_err(|_| UnsupportedSyntax)?;
                if !parsed[j].is_finite() {
                    return Err(NonFinite);
                }
            }
            let duplicate = match field.as_str() {
                "center" => center.replace(parsed).is_some(),
                "scale" => scale.replace(parsed).is_some(),
                "translate" => translate.replace(parsed).is_some(),
                "rotate" => rotation.replace(parsed[0]).is_some(),
                _ => unreachable!(),
            };
            if duplicate {
                return Err(UnsupportedSyntax);
            }
            i += width;
        }
        let (center, scale, rotation, translate) = (
            center.ok_or(UnsupportedSyntax)?,
            scale.ok_or(UnsupportedSyntax)?,
            rotation.ok_or(UnsupportedSyntax)?,
            translate.ok_or(UnsupportedSyntax)?,
        );
        if rotation != 0.0 {
            return Err(UnsupportedRotation);
        }
        if scale.iter().any(|v| *v <= 0.0) {
            return Err(UnsupportedReflection);
        }
        let result = [
            scale[0],
            scale[1],
            center[0] * (1.0 - scale[0]) + translate[0],
            center[1] * (1.0 - scale[1]) + translate[1],
        ];
        if result.iter().any(|v| !v.is_finite()) {
            return Err(NonFinite);
        }
        Ok(Self {
            scale_offset: result,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(t: AxisAlignedTextureTransform, uv: [f32; 2]) -> [f32; 2] {
        let [sx, sy, x, y] = t.scale_offset;
        [uv[0] * sx + x, uv[1] * sy + y]
    }

    #[test]
    fn missing_parameter_identity_and_authored_double_tiling() {
        assert_eq!(at(Default::default(), [0.25, 0.75]), [0.25, 0.75]);
        let t = AxisAlignedTextureTransform::parse("center 0 0 scale 2 2 rotate 0 translate 0 0")
            .unwrap();
        assert_eq!(at(t, [0.5, 1.0]), [1.0, 2.0]);
        assert_eq!(at(t, [-0.25, 0.125]), [-0.5, 0.25]);
    }

    #[test]
    fn centered_scale_and_translation_are_independent_of_field_order() {
        let t = AxisAlignedTextureTransform::parse(
            "TRANSLATE .125 -.25 rotate 0 scale 2 .5 center .5 .5",
        )
        .unwrap();
        // The center maps only by translation; halfway toward the left edge
        // reaches that edge after horizontal doubling, while Y halves.
        assert_eq!(at(t, [0.5, 0.5]), [0.625, 0.25]);
        assert_eq!(at(t, [0.25, 0.75]), [0.125, 0.375]);
    }

    #[test]
    fn unsupported_or_hostile_forms_are_not_admitted_as_identity() {
        use TextureTransformError::*;
        for text in [
            "",
            "scale 2 2",
            "center 0 0 scale 2 2 rotate 0 scale 3 3",
            "[1 0 0 0 0 1 0 0]",
            "center 0 0 scale 2 2 rotate 0 translate 0 0 extra",
        ] {
            assert_eq!(
                AxisAlignedTextureTransform::parse(text),
                Err(UnsupportedSyntax),
                "{text}"
            );
        }
        assert_eq!(
            AxisAlignedTextureTransform::parse("center 0 0 scale 2 2 rotate 90 translate 0 0"),
            Err(UnsupportedRotation)
        );
        assert_eq!(
            AxisAlignedTextureTransform::parse("center 0 0 scale -2 2 rotate 0 translate 0 0"),
            Err(UnsupportedReflection)
        );
        assert_eq!(
            AxisAlignedTextureTransform::parse("center 0 0 scale 0 2 rotate 0 translate 0 0"),
            Err(UnsupportedReflection)
        );
        for value in ["NaN", "inf", "-inf", "1e99"] {
            let text = format!("center 0 0 scale {value} 2 rotate 0 translate 0 0");
            assert_eq!(AxisAlignedTextureTransform::parse(&text), Err(NonFinite));
        }
        assert_eq!(
            AxisAlignedTextureTransform::parse("center 3e38 0 scale 3e38 2 rotate 0 translate 0 0"),
            Err(NonFinite)
        );
    }
}
