//! Authored ordinary detail layers, before lighting and without opacity changes.
//! Other detail operators/transforms remain explicit unsupported cases.
use crate::Material;

#[derive(Clone, Debug, PartialEq)]
pub struct DetailLayer<'a> {
    pub texture: &'a str,
    pub scale: [f32; 2],
    pub factor: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailError {
    Shader,
    Texture,
    Mode,
    Scale,
    Factor,
    Transform,
    Tint,
    Animation,
}

impl Material {
    /// The authored VertexLitGeneric ordinary/default detail candidate. Explicit
    /// unsupported declarations are errors, never silently accepted defaults.
    pub fn detail_layer(&self) -> Result<Option<DetailLayer<'_>>, DetailError> {
        let Some(texture) = self.str("$detail") else {
            return Ok(None);
        };
        if self.shader != "vertexlitgeneric" {
            return Err(DetailError::Shader);
        }
        if texture.trim().is_empty() {
            return Err(DetailError::Texture);
        }
        if self
            .str("$detailblendmode")
            .is_some_and(|v| v.trim().parse::<i32>() != Ok(0))
        {
            return Err(DetailError::Mode);
        }
        if [
            "$detailtexturetransform",
            "$separatedetailuvs",
            "$seamless_detail",
            "$seamless_scale",
        ]
        .iter()
        .any(|key| self.str(key).is_some())
        {
            return Err(DetailError::Transform);
        }
        if self.str("$detailtint").is_some() {
            return Err(DetailError::Tint);
        }
        if self.str("$detailframe").is_some() || self.params.block("proxies").is_some() {
            return Err(DetailError::Animation);
        }
        let scale = match self.str("$detailscale") {
            None => [4.0; 2],
            Some(text) => {
                let text = text.trim();
                let values = if text.starts_with('[') && text.ends_with(']') {
                    text[1..text.len() - 1]
                        .split_whitespace()
                        .map(str::parse::<f32>)
                        .collect::<Result<Vec<_>, _>>()
                } else {
                    text.parse::<f32>().map(|v| vec![v])
                }
                .map_err(|_| DetailError::Scale)?;
                let scale = match values.as_slice() {
                    [v] if !text.starts_with('[') => [*v; 2],
                    [s, t] => [*s, *t],
                    _ => return Err(DetailError::Scale),
                };
                if scale.iter().any(|v| !v.is_finite() || *v <= 0.0) {
                    return Err(DetailError::Scale);
                }
                scale
            }
        };
        let factor = self
            .str("$detailblendfactor")
            .map_or(Ok(1.0), |v| v.trim().parse::<f32>())
            .map_err(|_| DetailError::Factor)?;
        if !factor.is_finite() || factor < 0.0 {
            return Err(DetailError::Factor);
        }
        Ok(Some(DetailLayer {
            texture,
            scale,
            factor,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn material(params: &str) -> Material {
        Material::parse_with(&format!("VertexLitGeneric {{ {params} }}"), &|_| None, 0).unwrap()
    }
    #[test]
    fn independent_vector_scalar_and_absent_defaults() {
        let m = material(r#""$detail" "grain" "$detailscale" "[3 7]" "$detailblendfactor" ".25""#);
        assert_eq!(
            m.detail_layer().unwrap(),
            Some(DetailLayer {
                texture: "grain",
                scale: [3., 7.],
                factor: 0.25
            })
        );
        let m = material(
            r#""$detail" "grain" "$detailscale" "2.5" "$detailblendfactor" "0" "$detailblendmode" "0""#,
        );
        assert_eq!(m.detail_layer().unwrap().unwrap().scale, [2.5; 2]);
        assert_eq!(m.detail_layer().unwrap().unwrap().factor, 0.);
        let m = material(r#""$detail" "grain""#);
        assert_eq!(
            m.detail_layer().unwrap(),
            Some(DetailLayer {
                texture: "grain",
                scale: [4.; 2],
                factor: 1.
            })
        );
        assert!(material("").detail_layer().unwrap().is_none());
    }
    #[test]
    fn malformed_or_unsupported_declarations_do_not_become_defaults() {
        for scale in [
            "nan",
            "inf",
            "0",
            "-1",
            "[1 2 3]",
            "[2]",
            "[1 garbage]",
            "[1 2] trailing",
            "[1 -2]",
            "[1 NaN]",
        ] {
            assert_eq!(
                material(&format!(r#""$detail" "grain" "$detailscale" "{scale}""#)).detail_layer(),
                Err(DetailError::Scale)
            );
        }
        for factor in ["NaN", "inf", "-1", "nonsense"] {
            assert_eq!(
                material(&format!(
                    r#""$detail" "grain" "$detailblendfactor" "{factor}""#
                ))
                .detail_layer(),
                Err(DetailError::Factor)
            );
        }
        for (declaration, error) in [
            (r#""$detailblendmode" "1""#, DetailError::Mode),
            (r#""$detailblendmode" "nonsense""#, DetailError::Mode),
            (
                r#""$detailtexturetransform" "identity""#,
                DetailError::Transform,
            ),
            (r#""$separatedetailuvs" "1""#, DetailError::Transform),
            (r#""$detailtint" "[1 1 1]""#, DetailError::Tint),
            (r#""$detailframe" "0""#, DetailError::Animation),
            (
                r#"Proxies { AnimatedTexture { animatedtexturevar "$detail" } }"#,
                DetailError::Animation,
            ),
        ] {
            assert_eq!(
                material(&format!(r#""$detail" "grain" {declaration}"#)).detail_layer(),
                Err(error)
            );
        }
    }
}
