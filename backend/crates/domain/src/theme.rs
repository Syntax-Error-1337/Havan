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

/// Long enough for a real address, short enough that a settings row can show it without wrapping twice.
pub const MAX_SUPPORT_EMAIL: usize = 120;

/// Long enough for any status page URL, short enough to be a link and not a payload.
pub const MAX_STATUS_URL: usize = 200;

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
    /// Who a customer writes to when the app cannot help. `None` means the row is not drawn at all.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub support_email: Option<String>,
    /// The operator's status page. https only — the app hands it to the system browser.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_url: Option<String>,
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
    /// Optional: existing operators do not send these, and a missing field must not empty a working row.
    #[serde(default)]
    pub support_email: Option<String>,
    #[serde(default)]
    pub status_url: Option<String>,
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
            support_email: None,
            status_url: None,
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
        let support_email = support_email(input.support_email.as_deref())?;
        let status_url = status_url(input.status_url.as_deref())?;

        Ok(Self {
            product_name: product_name.to_string(),
            logo_text: logo_text.to_string(),
            accent_on: on_colour(&accent).to_string(),
            accent,
            accent_secondary,
            mode,
            support_email,
            status_url,
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
        if self.support_email != previous.support_email {
            changed.push("support_email");
        }
        if self.status_url != previous.status_url {
            changed.push("status_url");
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

/// Contrast between two colours, kept for the tests that describe the rule `on_colour` applies.
///
/// There was going to be a validation gate here refusing an accent whose own button label is unreadable. It
/// cannot fire: `on_colour` picks the *better* of black and white, and the worst any solid colour can do is
/// ~4.58:1 — the point where both choices are equally bad is exactly where both still pass. A check that can
/// never fail is worse than no check, so the rule stays a test on `on_colour` instead of a gate on input.
#[cfg(test)]
fn contrast(a: &str, b: &str) -> f64 {
    let luminance = |hex: &str| {
        let (r, g, b) = rgb(hex);
        let lin = |c: f64| {
            let c = c / 255.0;
            if c <= 0.040_45 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
    };
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

fn rgb(hex: &str) -> (f64, f64, f64) {
    let clean = hex.trim_start_matches('#');
    let full = format!("{clean}{}", "0".repeat(6usize.saturating_sub(clean.len())));
    let channel = |i: usize| u8::from_str_radix(&full[i..i + 2], 16).unwrap_or(0) as f64;
    (channel(0), channel(2), channel(4))
}

/// An address a customer can write to, or nothing.
///
/// Deliberately shallow: an email is checked by sending one, and a regex that rejects a valid address is worse than no
/// regex. What is refused is the shape that would render as a broken row — no `@`, whitespace, or a URL where an address
/// belongs.
fn support_email(raw: Option<&str>) -> Result<Option<String>, AppError> {
    let Some(raw) = raw.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    let valid = raw.len() <= MAX_SUPPORT_EMAIL
        && raw.contains('@')
        && !raw.split('@').next().unwrap_or_default().is_empty()
        && raw.split('@').nth(1).is_some_and(|d| d.contains('.'))
        && !raw.chars().any(char::is_whitespace);
    if !valid {
        return Err(validation(
            "that does not look like an email address — it is the one a customer writes to",
            "support_email",
        ));
    }
    Ok(Some(raw.to_string()))
}

/// The operator's status page, or nothing.
///
/// https only. The app hands this straight to the system browser, so `javascript:` or a plain `http://` link is not a
/// cosmetic problem — one is code, the other is a page that cannot be trusted to be the operator's.
fn status_url(raw: Option<&str>) -> Result<Option<String>, AppError> {
    let Some(raw) = raw.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    let valid = raw.len() <= MAX_STATUS_URL
        && raw.starts_with("https://")
        && raw.len() > "https://".len()
        && !raw.chars().any(char::is_whitespace);
    if !valid {
        return Err(validation(
            "the status link has to start with https:// — the app opens it in the browser",
            "status_url",
        ));
    }
    Ok(Some(raw.to_string()))
}

#[cfg(test)]
mod support_tests {
    use super::*;

    fn input() -> ThemeInput {
        ThemeInput {
            product_name: "Haven".into(),
            logo_text: "HV".into(),
            accent: "#146EF5".into(),
            accent_secondary: "#8B5CF6".into(),
            mode: "dark".into(),
            support_email: None,
            status_url: None,
        }
    }

    #[test]
    fn a_support_address_is_stored_trimmed_or_not_at_all() {
        let mut asked = input();
        asked.support_email = Some("  help@haven.example  ".into());
        assert_eq!(
            AppTheme::from_input(&asked)
                .expect("valid")
                .support_email
                .as_deref(),
            Some("help@haven.example")
        );

        // Empty means "not set", not "an address that is the empty string".
        asked.support_email = Some("   ".into());
        assert_eq!(
            AppTheme::from_input(&asked).expect("valid").support_email,
            None
        );

        for bad in ["help", "help@", "help@haven", "help haven@x.dev"] {
            asked.support_email = Some(bad.into());
            assert!(
                AppTheme::from_input(&asked).is_err(),
                "{bad:?} is not an address a customer can write to"
            );
        }
    }

    #[test]
    fn a_status_link_is_https_or_it_is_not_stored() {
        let mut asked = input();
        asked.status_url = Some("https://status.haven.example".into());
        assert_eq!(
            AppTheme::from_input(&asked)
                .expect("valid")
                .status_url
                .as_deref(),
            Some("https://status.haven.example")
        );

        // The app hands this to the system browser: anything that is not https is either code or an untrusted page.
        for bad in [
            "http://status.example",
            "javascript:alert(1)",
            "https://",
            "status.example",
        ] {
            asked.status_url = Some(bad.into());
            assert!(
                AppTheme::from_input(&asked).is_err(),
                "{bad:?} must be refused"
            );
        }
    }
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
            support_email: None,
            status_url: None,
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
    fn every_solid_accent_has_a_readable_label_on_it() {
        // The property that makes a validation gate unnecessary: whichever of black or white is chosen, the
        // worst case across the whole colour cube is ~4.58:1. Sampled on a grid rather than proved, because a
        // test that states the rule is what a future editor reads before adding a gate that cannot fire.
        let mut worst = f64::INFINITY;
        let mut worst_hex = String::from("#000000");
        for r in (0..=255).step_by(15) {
            for g in (0..=255).step_by(15) {
                for b in (0..=255).step_by(15) {
                    let hex = format!("#{r:02X}{g:02X}{b:02X}");
                    let ratio = contrast(&hex, on_colour(&hex));
                    if ratio < worst {
                        worst = ratio;
                        worst_hex = hex;
                    }
                }
            }
        }
        assert!(
            worst >= 4.5,
            "the worst solid colour is {worst_hex} at {worst:.2}:1 — a gate would be needed after all"
        );
    }

    #[test]
    fn the_accents_the_platform_actually_uses_still_save() {
        // The live brand and the two built-ins. A guard that refuses the product's own accent is a guard that
        // gets deleted in the first week.
        for accent in ["#146EF5", "#0062CC", "#8B5CF6", "#0A84FF", "#157F3C"] {
            assert!(
                AppTheme::from_input(&input(accent, "#22d3ee", "dark")).is_ok(),
                "{accent} has to remain a valid brand accent"
            );
        }
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
            support_email: None,
            status_url: None,
        })
        .unwrap();
        assert!(same.changes(&before).is_empty());

        let after = AppTheme::from_input(&ThemeInput {
            product_name: before.product_name.clone(),
            logo_text: before.logo_text.clone(),
            accent: "#FF3B30".into(),
            accent_secondary: before.accent_secondary.clone(),
            mode: "dark".into(),
            support_email: None,
            status_url: None,
        })
        .unwrap();
        assert_eq!(after.changes(&before), vec!["accent"]);
    }
}
