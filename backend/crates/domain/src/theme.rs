//! Platform appearance: the colours a white-label operator sets once and every client wears.
//!
//! The shape is deliberately small and closed. A theme is not arbitrary CSS — it is a handful of
//! decisions that must stay readable at every size, so this module owns the validation (a colour is
//! `#RRGGBB`, a mode is one of two) *and* the derived value the clients would otherwise each guess:
//! which of black or white can be read on top of the accent.
//!
//! Every rule here is pure and unit-tested, because the operator console, the API and the app all
//! have to agree about what a valid theme is. A colour that the console accepts and the API refuses
//! is a console that lies.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use kernel::AppError;

/// Long enough for a product name, short enough that a header does not wrap on a small phone.
pub const MAX_PRODUCT_NAME: usize = 32;

/// Initials, not a wordmark: the app renders them in a 42 pt circle.
pub const MAX_LOGO_TEXT: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Dark,
    Light,
}

impl ThemeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, AppError> {
        match raw {
            "dark" => Ok(Self::Dark),
            "light" => Ok(Self::Light),
            other => Err(AppError::validation(format!(
                "appearance must be dark or light, not {other}"
            ))
            .with_detail("field", "mode")),
        }
    }
}

/// What a client receives. `accent_on` is computed rather than stored: it is a consequence of the
/// accent, and a client that works it out for itself is a client that can get it wrong.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppTheme {
    pub product_name: String,
    pub logo_text: String,
    pub accent: String,
    pub accent_secondary: String,
    pub mode: ThemeMode,
    pub accent_on: String,
    #[serde(with = "time::serde::rfc3339::option")]
    pub updated_at: Option<OffsetDateTime>,
    pub updated_by: Option<String>,
}

/// What the operator sends. No `accent_on`: it is derived, and accepting it would let a caller
/// choose a text colour that cannot be read.
#[derive(Debug, Clone, Deserialize)]
pub struct ThemeInput {
    pub product_name: String,
    pub logo_text: String,
    pub accent: String,
    pub accent_secondary: String,
    pub mode: String,
}

impl Default for AppTheme {
    fn default() -> Self {
        Self {
            product_name: "Haven".into(),
            logo_text: "HV".into(),
            accent: "#0A84FF".into(),
            accent_secondary: "#8B5CF6".into(),
            mode: ThemeMode::Dark,
            accent_on: on_colour("#0A84FF").to_string(),
            updated_at: None,
            updated_by: None,
        }
    }
}

impl AppTheme {
    /// The theme an operator asked for, validated. `updated_*` is the store's business.
    pub fn from_input(input: &ThemeInput) -> Result<Self, AppError> {
        let product_name = input.product_name.trim();
        if product_name.is_empty() {
            return Err(validation(
                "the product needs a name: it is the first word on the sign-in screen",
                "product_name",
            ));
        }
        if product_name.chars().count() > MAX_PRODUCT_NAME {
            return Err(validation(
                &format!("keep the product name to {MAX_PRODUCT_NAME} characters or fewer"),
                "product_name",
            ));
        }

        let logo_text = input.logo_text.trim();
        if logo_text.is_empty() {
            return Err(validation(
                "the logo is the initials shown in the app header; one or two letters is enough",
                "logo_text",
            ));
        }
        if logo_text.chars().count() > MAX_LOGO_TEXT {
            return Err(validation(
                &format!("the logo holds at most {MAX_LOGO_TEXT} characters"),
                "logo_text",
            ));
        }

        let accent = normalise_colour(&input.accent, "accent")?;
        let accent_secondary = normalise_colour(&input.accent_secondary, "accent_secondary")?;
        let mode = ThemeMode::parse(&input.mode)?;

        Ok(Self {
            product_name: product_name.to_string(),
            logo_text: logo_text.to_string(),
            accent_on: on_colour(&accent).to_string(),
            accent,
            accent_secondary,
            mode,
            updated_at: None,
            updated_by: None,
        })
    }

    /// What changed, for the history row. Empty means "saved unchanged", which is worth recording
    /// as such rather than as a change nobody made.
    pub fn changes(&self, previous: &AppTheme) -> Vec<&'static str> {
        let mut changed = Vec::new();
        if self.product_name != previous.product_name {
            changed.push("product_name");
        }
        if self.logo_text != previous.logo_text {
            changed.push("logo_text");
        }
        if self.accent != previous.accent {
            changed.push("accent");
        }
        if self.accent_secondary != previous.accent_secondary {
            changed.push("accent_secondary");
        }
        if self.mode != previous.mode {
            changed.push("mode");
        }
        changed
    }
}

/// One shape for every refusal in this module: a validation error that names the field it is about,
/// so the console can put the message beside the input that caused it.
fn validation(message: &str, field: &str) -> AppError {
    AppError::validation(message).with_detail("field", field)
}

/// `#abc` becomes `#AABBCC`, `#aabbcc` becomes `#AABBCC`; anything else is refused by name.
///
/// Expanded and upper-cased so two operators typing the same colour produce the same row: a history
/// full of `#abc` and `#AABBCC` looks like two changes.
pub fn normalise_colour(raw: &str, field: &str) -> Result<String, AppError> {
    let value = raw.trim();
    let body = value.strip_prefix('#').ok_or_else(|| {
        validation(
            &format!("{value} is not a colour; write it as #RRGGBB"),
            field,
        )
    })?;

    let expanded = match body.len() {
        3 if body.chars().all(|c| c.is_ascii_hexdigit()) => {
            let mut out = String::with_capacity(6);
            for c in body.chars() {
                out.push(c);
                out.push(c);
            }
            out
        }
        6 if body.chars().all(|c| c.is_ascii_hexdigit()) => body.to_string(),
        _ => {
            return Err(validation(
                &format!("{value} is not a colour; write it as #RRGGBB"),
                field,
            ));
        }
    };

    Ok(format!("#{}", expanded.to_ascii_uppercase()))
}

/// Black or white, whichever can be read on this colour.
///
/// The 0.179 threshold is the WCAG cut-off for switching between the two, applied to relative
/// luminance — the same rule the app uses for chips and buttons, computed once here so a new accent
/// cannot produce unreadable text on one screen and readable text on another.
pub fn on_colour(hex: &str) -> &'static str {
    let (r, g, b) = rgb(hex);
    let lin = |c: f64| {
        let c = c / 255.0;
        if c <= 0.040_45 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance = 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
    if luminance > 0.179 {
        "#000000"
    } else {
        "#FFFFFF"
    }
}

fn rgb(hex: &str) -> (f64, f64, f64) {
    let clean = hex.trim_start_matches('#');
    let full = format!("{clean}{}", "0".repeat(6usize.saturating_sub(clean.len())));
    let channel = |i: usize| u8::from_str_radix(&full[i..i + 2], 16).unwrap_or(0) as f64;
    (channel(0), channel(2), channel(4))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(accent: &str, secondary: &str, mode: &str) -> ThemeInput {
        ThemeInput {
            product_name: "Acme Home".into(),
            logo_text: "AH".into(),
            accent: accent.into(),
            accent_secondary: secondary.into(),
            mode: mode.into(),
        }
    }

    #[test]
    fn a_short_colour_expands_and_upper_cases() {
        assert_eq!(normalise_colour("#abc", "x").unwrap(), "#AABBCC");
        assert_eq!(normalise_colour("#A0B1c2", "x").unwrap(), "#A0B1C2");
    }

    #[test]
    fn a_colour_that_is_not_one_is_refused_by_name() {
        for bad in ["0A84FF", "#12345", "#GGGGGG", "", "blue"] {
            let err = normalise_colour(bad, "accent").unwrap_err();
            assert_eq!(
                err.code_kind(),
                kernel::ErrorCode::ValidationFailed,
                "for {bad:?}"
            );
            assert!(
                err.public_message().contains("#RRGGBB"),
                "the refusal has to say what a colour looks like: {bad:?}"
            );
        }
    }

    #[test]
    fn text_on_accent_is_chosen_not_guessed() {
        // A deep accent takes white, a light one takes black. `#0A84FF` is the interesting case: it
        // looks dark and is not — white on it is 3.6:1, which fails AA, and black is 6.7:1. The rule
        // decides by luminance, so the product does not ship a pretty button nobody can read.
        assert_eq!(on_colour("#FFFFFF"), "#000000");
        assert_eq!(on_colour("#FFD400"), "#000000");
        assert_eq!(on_colour("#101820"), "#FFFFFF");
        assert_eq!(on_colour("#2563EB"), "#FFFFFF");
        assert_eq!(on_colour("#0A84FF"), "#000000");
    }

    #[test]
    fn a_valid_theme_round_trips_with_its_derived_text_colour() {
        let theme = AppTheme::from_input(&input("#8b5cf6", "#22d3ee", "light")).unwrap();
        assert_eq!(theme.accent, "#8B5CF6");
        assert_eq!(theme.mode, ThemeMode::Light);
        assert_eq!(theme.accent_on, on_colour("#8B5CF6"));
    }

    #[test]
    fn an_unreadable_theme_is_refused_with_a_reason() {
        let empty_name = AppTheme::from_input(&ThemeInput {
            product_name: "   ".into(),
            ..input("#0A84FF", "#8B5CF6", "dark")
        })
        .unwrap_err();
        assert_eq!(empty_name.code_kind(), kernel::ErrorCode::ValidationFailed);
        assert!(empty_name.public_message().contains("needs a name"));

        let long_logo = AppTheme::from_input(&ThemeInput {
            logo_text: "ABCD".into(),
            ..input("#0A84FF", "#8B5CF6", "dark")
        })
        .unwrap_err();
        assert!(long_logo.public_message().contains("at most"));

        let bad_mode = AppTheme::from_input(&input("#0A84FF", "#8B5CF6", "dusk")).unwrap_err();
        assert!(bad_mode.public_message().contains("dark or light"));
    }

    #[test]
    fn changes_names_what_moved_and_nothing_else() {
        let before = AppTheme::default();
        let same = AppTheme::from_input(&ThemeInput {
            product_name: before.product_name.clone(),
            logo_text: before.logo_text.clone(),
            accent: before.accent.clone(),
            accent_secondary: before.accent_secondary.clone(),
            mode: "dark".into(),
        })
        .unwrap();
        assert!(same.changes(&before).is_empty());

        let after = AppTheme::from_input(&ThemeInput {
            product_name: before.product_name.clone(),
            logo_text: before.logo_text.clone(),
            accent: "#FF3B30".into(),
            accent_secondary: before.accent_secondary.clone(),
            mode: "dark".into(),
        })
        .unwrap();
        assert_eq!(after.changes(&before), vec!["accent"]);
    }
}
