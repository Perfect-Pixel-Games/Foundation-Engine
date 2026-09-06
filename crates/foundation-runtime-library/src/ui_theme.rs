//! Generic design-token theming for Foundation games, analogous to CSS.
//!
//! This module owns the entire theming *mechanism*: the token vocabulary
//! (which colors, spacing steps, font sizes, and typography roles exist),
//! the `.bsn`-authorable wrapper components that reference a token instead
//! of a raw literal, the systems that resolve those tokens into real Bevy UI
//! components every frame, and a generic TOML loader for a theme file.
//!
//! Foundation never bakes in a specific game's palette. Each game supplies
//! its own concrete [`FoundationUiTheme`] values (typically via
//! [`load_ui_theme_from_file`] pointed at that game's own theme file) and
//! inserts it as a resource, overriding the loud placeholder `Default`
//! installed by [`FoundationPlugin`](crate::FoundationPlugin). Every widget
//! authored against these tokens then re-skins automatically -- the same
//! reuse story a CSS stylesheet gives a set of HTML pages, and the reason a
//! second game can reuse the same widget library with an entirely different
//! look by shipping only its own theme file.

use std::path::Path;

use bevy::{prelude::*, text::FontSize};
use serde::Deserialize;

use crate::bsn_assets::apply_pending_bsn_instances;

/// Semantic color slots a widget can be authored against instead of a raw
/// literal, analogous to a CSS custom property.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Reflect)]
#[reflect(Default)]
pub enum FoundationUiColorToken {
    Background,
    SurfaceDeepest,
    SurfaceDeepestOverlay,
    SurfaceRaised,
    SurfaceRaisedOpaque,
    #[default]
    Surface,
    SurfaceStrong,
    Border,
    BorderHover,
    BorderDisabled,
    SurfaceDisabled,
    TextPrimary,
    TextBright,
    TextSecondary,
    TextMuted,
    TextSubtle,
    TextDisabled,
    TextOnAccent,
    Accent,
    AccentHover,
    AccentPressed,
    AccentSoft,
    SecondaryAccent,
    Success,
    Transparent,
}

/// Rank-ordered spacing scale for padding, margin, and gaps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Reflect)]
#[reflect(Default)]
pub enum FoundationUiSpaceToken {
    Zero,
    Xxs,
    Xs,
    #[default]
    Sm,
    Md,
    Lg,
    Xl,
    Xxl,
}

/// Rank-ordered font-size scale.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Reflect)]
#[reflect(Default)]
pub enum FoundationUiFontSizeToken {
    Xxs,
    Xs,
    #[default]
    Sm,
    Md,
    Lg,
    Xl,
    Xxl,
    XxlPlus,
    Xxxl,
    XxxlPlus,
    Xxxxl,
}

/// Border stroke-thickness scale.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Reflect)]
#[reflect(Default)]
pub enum FoundationUiBorderWidthToken {
    #[default]
    None,
    Hairline,
    Thick,
}

/// Border corner-rounding scale.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Reflect)]
#[reflect(Default)]
pub enum FoundationUiBorderRadiusToken {
    #[default]
    None,
    Sm,
}

/// Semantic typography roles bundling a font size and a text color, the same
/// way a CSS `h1 { ... }` rule bundles multiple properties under one class.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Reflect)]
#[reflect(Default)]
pub enum FoundationUiTypographyToken {
    Heading1,
    Heading2,
    Heading3,
    Heading4,
    #[default]
    Body,
    Caption,
    ButtonLabel,
}

/// Concrete design-token values for one game's UI theme.
///
/// Fields are named per-token rather than stored in a map: `Color` does not
/// implement `Hash`/`Eq`, and named fields give compile-time exhaustiveness
/// on every accessor instead of a runtime "missing token" possibility.
///
/// [`Default`] is a deliberately loud placeholder -- magenta colors and an
/// obviously-wrong, non-rank-ordered spacing/font-size value -- so a game
/// that forgets to install its own theme notices immediately instead of
/// silently inheriting a look that was never meant for it.
#[derive(Clone, Debug, Resource, Reflect)]
#[reflect(Resource)]
pub struct FoundationUiTheme {
    pub background: Color,
    pub surface_deepest: Color,
    pub surface_deepest_overlay: Color,
    pub surface_raised: Color,
    pub surface_raised_opaque: Color,
    pub surface: Color,
    pub surface_strong: Color,
    pub border: Color,
    pub border_hover: Color,
    pub border_disabled: Color,
    pub surface_disabled: Color,
    pub text_primary: Color,
    pub text_bright: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub text_subtle: Color,
    pub text_disabled: Color,
    pub text_on_accent: Color,
    pub accent: Color,
    pub accent_hover: Color,
    pub accent_pressed: Color,
    pub accent_soft: Color,
    pub secondary_accent: Color,
    pub success: Color,
    pub transparent: Color,

    pub space_zero: f32,
    pub space_xxs: f32,
    pub space_xs: f32,
    pub space_sm: f32,
    pub space_md: f32,
    pub space_lg: f32,
    pub space_xl: f32,
    pub space_xxl: f32,

    pub font_size_xxs: f32,
    pub font_size_xs: f32,
    pub font_size_sm: f32,
    pub font_size_md: f32,
    pub font_size_lg: f32,
    pub font_size_xl: f32,
    pub font_size_xxl: f32,
    pub font_size_xxl_plus: f32,
    pub font_size_xxxl: f32,
    pub font_size_xxxl_plus: f32,
    pub font_size_xxxxl: f32,

    pub border_width_none: f32,
    pub border_width_hairline: f32,
    pub border_width_thick: f32,

    pub border_radius_none: f32,
    pub border_radius_sm: f32,
}

impl FoundationUiTheme {
    /// Resolves a color token to its concrete value.
    #[must_use]
    pub fn color(&self, token: FoundationUiColorToken) -> Color {
        match token {
            FoundationUiColorToken::Background => self.background,
            FoundationUiColorToken::SurfaceDeepest => self.surface_deepest,
            FoundationUiColorToken::SurfaceDeepestOverlay => self.surface_deepest_overlay,
            FoundationUiColorToken::SurfaceRaised => self.surface_raised,
            FoundationUiColorToken::SurfaceRaisedOpaque => self.surface_raised_opaque,
            FoundationUiColorToken::Surface => self.surface,
            FoundationUiColorToken::SurfaceStrong => self.surface_strong,
            FoundationUiColorToken::Border => self.border,
            FoundationUiColorToken::BorderHover => self.border_hover,
            FoundationUiColorToken::BorderDisabled => self.border_disabled,
            FoundationUiColorToken::SurfaceDisabled => self.surface_disabled,
            FoundationUiColorToken::TextPrimary => self.text_primary,
            FoundationUiColorToken::TextBright => self.text_bright,
            FoundationUiColorToken::TextSecondary => self.text_secondary,
            FoundationUiColorToken::TextMuted => self.text_muted,
            FoundationUiColorToken::TextSubtle => self.text_subtle,
            FoundationUiColorToken::TextDisabled => self.text_disabled,
            FoundationUiColorToken::TextOnAccent => self.text_on_accent,
            FoundationUiColorToken::Accent => self.accent,
            FoundationUiColorToken::AccentHover => self.accent_hover,
            FoundationUiColorToken::AccentPressed => self.accent_pressed,
            FoundationUiColorToken::AccentSoft => self.accent_soft,
            FoundationUiColorToken::SecondaryAccent => self.secondary_accent,
            FoundationUiColorToken::Success => self.success,
            FoundationUiColorToken::Transparent => self.transparent,
        }
    }

    /// Resolves a color token, then overrides its alpha channel.
    #[must_use]
    pub fn color_alpha(&self, token: FoundationUiColorToken, alpha: f32) -> Color {
        self.color(token).with_alpha(alpha)
    }

    /// Resolves a spacing token to its pixel value.
    #[must_use]
    pub fn space(&self, token: FoundationUiSpaceToken) -> f32 {
        match token {
            FoundationUiSpaceToken::Zero => self.space_zero,
            FoundationUiSpaceToken::Xxs => self.space_xxs,
            FoundationUiSpaceToken::Xs => self.space_xs,
            FoundationUiSpaceToken::Sm => self.space_sm,
            FoundationUiSpaceToken::Md => self.space_md,
            FoundationUiSpaceToken::Lg => self.space_lg,
            FoundationUiSpaceToken::Xl => self.space_xl,
            FoundationUiSpaceToken::Xxl => self.space_xxl,
        }
    }

    /// Resolves a font-size token to its pixel value.
    #[must_use]
    pub fn font_size(&self, token: FoundationUiFontSizeToken) -> f32 {
        match token {
            FoundationUiFontSizeToken::Xxs => self.font_size_xxs,
            FoundationUiFontSizeToken::Xs => self.font_size_xs,
            FoundationUiFontSizeToken::Sm => self.font_size_sm,
            FoundationUiFontSizeToken::Md => self.font_size_md,
            FoundationUiFontSizeToken::Lg => self.font_size_lg,
            FoundationUiFontSizeToken::Xl => self.font_size_xl,
            FoundationUiFontSizeToken::Xxl => self.font_size_xxl,
            FoundationUiFontSizeToken::XxlPlus => self.font_size_xxl_plus,
            FoundationUiFontSizeToken::Xxxl => self.font_size_xxxl,
            FoundationUiFontSizeToken::XxxlPlus => self.font_size_xxxl_plus,
            FoundationUiFontSizeToken::Xxxxl => self.font_size_xxxxl,
        }
    }

    /// Resolves a border-width token to its pixel value.
    #[must_use]
    pub fn border_width(&self, token: FoundationUiBorderWidthToken) -> f32 {
        match token {
            FoundationUiBorderWidthToken::None => self.border_width_none,
            FoundationUiBorderWidthToken::Hairline => self.border_width_hairline,
            FoundationUiBorderWidthToken::Thick => self.border_width_thick,
        }
    }

    /// Resolves a border-radius token to its pixel value.
    #[must_use]
    pub fn border_radius(&self, token: FoundationUiBorderRadiusToken) -> f32 {
        match token {
            FoundationUiBorderRadiusToken::None => self.border_radius_none,
            FoundationUiBorderRadiusToken::Sm => self.border_radius_sm,
        }
    }

    /// Resolves a typography role to its `(font_size_px, color)` pair.
    #[must_use]
    pub fn typography(&self, token: FoundationUiTypographyToken) -> (f32, Color) {
        match token {
            FoundationUiTypographyToken::Heading1 => (
                self.font_size(FoundationUiFontSizeToken::Xxxxl),
                self.color(FoundationUiColorToken::TextPrimary),
            ),
            FoundationUiTypographyToken::Heading2 => (
                self.font_size(FoundationUiFontSizeToken::Xxxl),
                self.color(FoundationUiColorToken::TextPrimary),
            ),
            FoundationUiTypographyToken::Heading3 => (
                self.font_size(FoundationUiFontSizeToken::Xxl),
                self.color(FoundationUiColorToken::Accent),
            ),
            FoundationUiTypographyToken::Heading4 => (
                self.font_size(FoundationUiFontSizeToken::Lg),
                self.color(FoundationUiColorToken::TextMuted),
            ),
            FoundationUiTypographyToken::Body => (
                self.font_size(FoundationUiFontSizeToken::Md),
                self.color(FoundationUiColorToken::TextSecondary),
            ),
            FoundationUiTypographyToken::Caption => (
                self.font_size(FoundationUiFontSizeToken::Sm),
                self.color(FoundationUiColorToken::TextMuted),
            ),
            FoundationUiTypographyToken::ButtonLabel => (
                self.font_size(FoundationUiFontSizeToken::Lg),
                self.color(FoundationUiColorToken::TextPrimary),
            ),
        }
    }
}

impl Default for FoundationUiTheme {
    fn default() -> Self {
        const MISSING_THEME_COLOR: Color = Color::srgb(1.0, 0.0, 1.0);
        const MISSING_THEME_SPACE: f32 = 40.0;
        const MISSING_THEME_FONT_SIZE: f32 = 40.0;
        const MISSING_THEME_BORDER_WIDTH: f32 = 8.0;
        const MISSING_THEME_BORDER_RADIUS: f32 = 8.0;

        Self {
            background: MISSING_THEME_COLOR,
            surface_deepest: MISSING_THEME_COLOR,
            surface_deepest_overlay: MISSING_THEME_COLOR,
            surface_raised: MISSING_THEME_COLOR,
            surface_raised_opaque: MISSING_THEME_COLOR,
            surface: MISSING_THEME_COLOR,
            surface_strong: MISSING_THEME_COLOR,
            border: MISSING_THEME_COLOR,
            border_hover: MISSING_THEME_COLOR,
            border_disabled: MISSING_THEME_COLOR,
            surface_disabled: MISSING_THEME_COLOR,
            text_primary: MISSING_THEME_COLOR,
            text_bright: MISSING_THEME_COLOR,
            text_secondary: MISSING_THEME_COLOR,
            text_muted: MISSING_THEME_COLOR,
            text_subtle: MISSING_THEME_COLOR,
            text_disabled: MISSING_THEME_COLOR,
            text_on_accent: MISSING_THEME_COLOR,
            accent: MISSING_THEME_COLOR,
            accent_hover: MISSING_THEME_COLOR,
            accent_pressed: MISSING_THEME_COLOR,
            accent_soft: MISSING_THEME_COLOR,
            secondary_accent: MISSING_THEME_COLOR,
            success: MISSING_THEME_COLOR,
            transparent: MISSING_THEME_COLOR,

            space_zero: MISSING_THEME_SPACE,
            space_xxs: MISSING_THEME_SPACE,
            space_xs: MISSING_THEME_SPACE,
            space_sm: MISSING_THEME_SPACE,
            space_md: MISSING_THEME_SPACE,
            space_lg: MISSING_THEME_SPACE,
            space_xl: MISSING_THEME_SPACE,
            space_xxl: MISSING_THEME_SPACE,

            font_size_xxs: MISSING_THEME_FONT_SIZE,
            font_size_xs: MISSING_THEME_FONT_SIZE,
            font_size_sm: MISSING_THEME_FONT_SIZE,
            font_size_md: MISSING_THEME_FONT_SIZE,
            font_size_lg: MISSING_THEME_FONT_SIZE,
            font_size_xl: MISSING_THEME_FONT_SIZE,
            font_size_xxl: MISSING_THEME_FONT_SIZE,
            font_size_xxl_plus: MISSING_THEME_FONT_SIZE,
            font_size_xxxl: MISSING_THEME_FONT_SIZE,
            font_size_xxxl_plus: MISSING_THEME_FONT_SIZE,
            font_size_xxxxl: MISSING_THEME_FONT_SIZE,

            border_width_none: MISSING_THEME_BORDER_WIDTH,
            border_width_hairline: MISSING_THEME_BORDER_WIDTH,
            border_width_thick: MISSING_THEME_BORDER_WIDTH,

            border_radius_none: MISSING_THEME_BORDER_RADIUS,
            border_radius_sm: MISSING_THEME_BORDER_RADIUS,
        }
    }
}

/// Authors a themed background color in place of a raw `BackgroundColor` literal.
#[derive(Clone, Copy, Debug, Default, Component, Reflect)]
#[reflect(Component, Default)]
pub struct FoundationUiThemedBackground(pub FoundationUiColorToken);

/// Authors a themed (uniform, all-sides) border color in place of a raw `BorderColor` literal.
#[derive(Clone, Copy, Debug, Default, Component, Reflect)]
#[reflect(Component, Default)]
pub struct FoundationUiThemedBorderColor(pub FoundationUiColorToken);

/// Authors a themed text color in place of a raw `TextColor` literal.
#[derive(Clone, Copy, Debug, Default, Component, Reflect)]
#[reflect(Component, Default)]
pub struct FoundationUiThemedText(pub FoundationUiColorToken);

/// Authors a semantic typography role (heading/body/caption/...), setting
/// both font size and text color together.
///
/// Do not also author [`FoundationUiThemedText`] or [`FoundationUiThemedFontSize`]
/// on the same entity as this component: their resolver systems have no
/// ordering constraint relative to `resolve_ui_themed_typography`, so which
/// one's write wins on `TextColor`/`TextFont.font_size` would be a race. Use
/// this component alone when a role's exact (size, color) pair matches; use
/// the two standalone wrappers together (never one of each with this
/// component) when it doesn't.
#[derive(Clone, Copy, Debug, Default, Component, Reflect)]
#[reflect(Component, Default)]
pub struct FoundationUiThemedTypography(pub FoundationUiTypographyToken);

/// Authors a themed font size only, leaving text color to another system --
/// for interactive labels (buttons, tabs) whose color already comes from a
/// dynamic interaction-driven style system.
#[derive(Clone, Copy, Debug, Default, Component, Reflect)]
#[reflect(Component, Default)]
pub struct FoundationUiThemedFontSize(pub FoundationUiFontSizeToken);

/// Authors themed padding in place of raw `Node.padding` literals.
///
/// A horizontal/vertical pair (rather than full four-sided padding) covers
/// every symmetric padding block; widgets that genuinely need asymmetric
/// padding keep it as a raw literal.
#[derive(Clone, Copy, Debug, Default, Component, Reflect)]
#[reflect(Component, Default)]
pub struct FoundationUiThemedPadding {
    pub horizontal: FoundationUiSpaceToken,
    pub vertical: FoundationUiSpaceToken,
}

/// Authors themed row/column gaps in place of raw `Node.row_gap`/`column_gap` literals.
#[derive(Clone, Copy, Debug, Default, Component, Reflect)]
#[reflect(Component, Default)]
pub struct FoundationUiThemedGap {
    pub row: FoundationUiSpaceToken,
    pub column: FoundationUiSpaceToken,
}

/// Authors a themed (uniform, all-sides) border width in place of a raw `Node.border` literal.
#[derive(Clone, Copy, Debug, Default, Component, Reflect)]
#[reflect(Component, Default)]
pub struct FoundationUiThemedBorderWidth(pub FoundationUiBorderWidthToken);

/// Authors a themed (uniform, all-corners) border radius in place of a raw `Node.border_radius` literal.
#[derive(Clone, Copy, Debug, Default, Component, Reflect)]
#[reflect(Component, Default)]
pub struct FoundationUiThemedBorderRadius(pub FoundationUiBorderRadiusToken);

/// Resolves [`FoundationUiThemedBackground`] into a real `BackgroundColor`.
///
/// Runs unconditionally every frame, not gated by `Added<T>`: BSN scenes
/// spawn widgets through deferred commands, so a themed component can become
/// visible to this system on a different frame than other systems see the
/// same entity, and running unconditionally is immune to that race (see
/// `last_beacon::ui_widgets::enforce_last_beacon_button_styles` for the
/// concrete bug this pattern was introduced to fix). It also gives live
/// theme-swap support -- changing `Res<FoundationUiTheme>` re-skins every
/// already-spawned widget on the very next frame -- for free.
///
/// The write itself is still guarded by an equality check: Bevy marks a
/// component "changed" on every mutable dereference regardless of whether
/// the value actually differs, and downstream systems (UI layout in
/// particular, for the `Node`-writing resolvers below) key off that change
/// flag to decide whether to redo expensive work. Writing unconditionally
/// every frame across every themed widget in a scene forced full UI layout
/// recomputation every single frame -- a severe, entirely avoidable
/// performance regression. Skipping the write when nothing changed keeps
/// this system self-healing and frame-cheap at the same time.
pub fn resolve_ui_themed_backgrounds(
    theme: Res<FoundationUiTheme>,
    mut themed_nodes: Query<(&FoundationUiThemedBackground, &mut BackgroundColor)>,
) {
    for (themed, mut background_color) in &mut themed_nodes {
        let resolved_color = theme.color(themed.0);
        if background_color.0 != resolved_color {
            background_color.0 = resolved_color;
        }
    }
}

/// Resolves [`FoundationUiThemedBorderColor`] into a real `BorderColor`.
pub fn resolve_ui_themed_border_colors(
    theme: Res<FoundationUiTheme>,
    mut themed_nodes: Query<(&FoundationUiThemedBorderColor, &mut BorderColor)>,
) {
    for (themed, mut border_color) in &mut themed_nodes {
        let resolved = BorderColor::all(theme.color(themed.0));
        if *border_color != resolved {
            *border_color = resolved;
        }
    }
}

/// Resolves [`FoundationUiThemedText`] into a real `TextColor`.
pub fn resolve_ui_themed_text(
    theme: Res<FoundationUiTheme>,
    mut themed_texts: Query<(&FoundationUiThemedText, &mut TextColor)>,
) {
    for (themed, mut text_color) in &mut themed_texts {
        let resolved_color = theme.color(themed.0);
        if text_color.0 != resolved_color {
            text_color.0 = resolved_color;
        }
    }
}

/// Resolves [`FoundationUiThemedTypography`] into `TextFont.font_size` and `TextColor`.
pub fn resolve_ui_themed_typography(
    theme: Res<FoundationUiTheme>,
    mut themed_texts: Query<(&FoundationUiThemedTypography, &mut TextFont, &mut TextColor)>,
) {
    for (themed, mut text_font, mut text_color) in &mut themed_texts {
        let (font_size, color) = theme.typography(themed.0);
        let resolved_font_size = FontSize::Px(font_size);
        if text_font.font_size != resolved_font_size {
            text_font.font_size = resolved_font_size;
        }
        if text_color.0 != color {
            text_color.0 = color;
        }
    }
}

/// Resolves [`FoundationUiThemedFontSize`] into `TextFont.font_size`.
pub fn resolve_ui_themed_font_sizes(
    theme: Res<FoundationUiTheme>,
    mut themed_texts: Query<(&FoundationUiThemedFontSize, &mut TextFont)>,
) {
    for (themed, mut text_font) in &mut themed_texts {
        let resolved_font_size = FontSize::Px(theme.font_size(themed.0));
        if text_font.font_size != resolved_font_size {
            text_font.font_size = resolved_font_size;
        }
    }
}

/// Resolves [`FoundationUiThemedPadding`] into `Node.padding`.
pub fn resolve_ui_themed_padding(
    theme: Res<FoundationUiTheme>,
    mut themed_nodes: Query<(&FoundationUiThemedPadding, &mut Node)>,
) {
    for (themed, mut node) in &mut themed_nodes {
        let resolved_padding = UiRect::axes(
            Val::Px(theme.space(themed.horizontal)),
            Val::Px(theme.space(themed.vertical)),
        );
        if node.padding != resolved_padding {
            node.padding = resolved_padding;
        }
    }
}

/// Resolves [`FoundationUiThemedGap`] into `Node.row_gap`/`Node.column_gap`.
pub fn resolve_ui_themed_gap(
    theme: Res<FoundationUiTheme>,
    mut themed_nodes: Query<(&FoundationUiThemedGap, &mut Node)>,
) {
    for (themed, mut node) in &mut themed_nodes {
        let resolved_row_gap = Val::Px(theme.space(themed.row));
        let resolved_column_gap = Val::Px(theme.space(themed.column));
        if node.row_gap != resolved_row_gap {
            node.row_gap = resolved_row_gap;
        }
        if node.column_gap != resolved_column_gap {
            node.column_gap = resolved_column_gap;
        }
    }
}

/// Resolves [`FoundationUiThemedBorderWidth`] into `Node.border`.
pub fn resolve_ui_themed_border_width(
    theme: Res<FoundationUiTheme>,
    mut themed_nodes: Query<(&FoundationUiThemedBorderWidth, &mut Node)>,
) {
    for (themed, mut node) in &mut themed_nodes {
        let resolved_border = UiRect::all(Val::Px(theme.border_width(themed.0)));
        if node.border != resolved_border {
            node.border = resolved_border;
        }
    }
}

/// Resolves [`FoundationUiThemedBorderRadius`] into `Node.border_radius`.
pub fn resolve_ui_themed_border_radius(
    theme: Res<FoundationUiTheme>,
    mut themed_nodes: Query<(&FoundationUiThemedBorderRadius, &mut Node)>,
) {
    for (themed, mut node) in &mut themed_nodes {
        let resolved_border_radius = BorderRadius::all(Val::Px(theme.border_radius(themed.0)));
        if node.border_radius != resolved_border_radius {
            node.border_radius = resolved_border_radius;
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct TomlColor {
    r: f32,
    g: f32,
    b: f32,
    a: f32,
}

impl From<TomlColor> for Color {
    fn from(color: TomlColor) -> Self {
        Color::srgba(color.r, color.g, color.b, color.a)
    }
}

#[derive(Clone, Debug, Deserialize)]
struct FoundationUiThemeColorsFile {
    background: TomlColor,
    surface_deepest: TomlColor,
    surface_deepest_overlay: TomlColor,
    surface_raised: TomlColor,
    surface_raised_opaque: TomlColor,
    surface: TomlColor,
    surface_strong: TomlColor,
    border: TomlColor,
    border_hover: TomlColor,
    border_disabled: TomlColor,
    surface_disabled: TomlColor,
    text_primary: TomlColor,
    text_bright: TomlColor,
    text_secondary: TomlColor,
    text_muted: TomlColor,
    text_subtle: TomlColor,
    text_disabled: TomlColor,
    text_on_accent: TomlColor,
    accent: TomlColor,
    accent_hover: TomlColor,
    accent_pressed: TomlColor,
    accent_soft: TomlColor,
    secondary_accent: TomlColor,
    success: TomlColor,
    transparent: TomlColor,
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct FoundationUiThemeSpaceFile {
    zero: f32,
    xxs: f32,
    xs: f32,
    sm: f32,
    md: f32,
    lg: f32,
    xl: f32,
    xxl: f32,
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct FoundationUiThemeFontSizesFile {
    xxs: f32,
    xs: f32,
    sm: f32,
    md: f32,
    lg: f32,
    xl: f32,
    xxl: f32,
    xxl_plus: f32,
    xxxl: f32,
    xxxl_plus: f32,
    xxxxl: f32,
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct FoundationUiThemeBorderWidthsFile {
    none: f32,
    hairline: f32,
    thick: f32,
}

#[derive(Clone, Copy, Debug, Deserialize)]
struct FoundationUiThemeBorderRadiiFile {
    none: f32,
    sm: f32,
}

#[derive(Clone, Debug, Deserialize)]
struct FoundationUiThemeFile {
    colors: FoundationUiThemeColorsFile,
    space: FoundationUiThemeSpaceFile,
    font_sizes: FoundationUiThemeFontSizesFile,
    border_widths: FoundationUiThemeBorderWidthsFile,
    border_radii: FoundationUiThemeBorderRadiiFile,
}

impl From<FoundationUiThemeFile> for FoundationUiTheme {
    fn from(file: FoundationUiThemeFile) -> Self {
        Self {
            background: file.colors.background.into(),
            surface_deepest: file.colors.surface_deepest.into(),
            surface_deepest_overlay: file.colors.surface_deepest_overlay.into(),
            surface_raised: file.colors.surface_raised.into(),
            surface_raised_opaque: file.colors.surface_raised_opaque.into(),
            surface: file.colors.surface.into(),
            surface_strong: file.colors.surface_strong.into(),
            border: file.colors.border.into(),
            border_hover: file.colors.border_hover.into(),
            border_disabled: file.colors.border_disabled.into(),
            surface_disabled: file.colors.surface_disabled.into(),
            text_primary: file.colors.text_primary.into(),
            text_bright: file.colors.text_bright.into(),
            text_secondary: file.colors.text_secondary.into(),
            text_muted: file.colors.text_muted.into(),
            text_subtle: file.colors.text_subtle.into(),
            text_disabled: file.colors.text_disabled.into(),
            text_on_accent: file.colors.text_on_accent.into(),
            accent: file.colors.accent.into(),
            accent_hover: file.colors.accent_hover.into(),
            accent_pressed: file.colors.accent_pressed.into(),
            accent_soft: file.colors.accent_soft.into(),
            secondary_accent: file.colors.secondary_accent.into(),
            success: file.colors.success.into(),
            transparent: file.colors.transparent.into(),

            space_zero: file.space.zero,
            space_xxs: file.space.xxs,
            space_xs: file.space.xs,
            space_sm: file.space.sm,
            space_md: file.space.md,
            space_lg: file.space.lg,
            space_xl: file.space.xl,
            space_xxl: file.space.xxl,

            font_size_xxs: file.font_sizes.xxs,
            font_size_xs: file.font_sizes.xs,
            font_size_sm: file.font_sizes.sm,
            font_size_md: file.font_sizes.md,
            font_size_lg: file.font_sizes.lg,
            font_size_xl: file.font_sizes.xl,
            font_size_xxl: file.font_sizes.xxl,
            font_size_xxl_plus: file.font_sizes.xxl_plus,
            font_size_xxxl: file.font_sizes.xxxl,
            font_size_xxxl_plus: file.font_sizes.xxxl_plus,
            font_size_xxxxl: file.font_sizes.xxxxl,

            border_width_none: file.border_widths.none,
            border_width_hairline: file.border_widths.hairline,
            border_width_thick: file.border_widths.thick,

            border_radius_none: file.border_radii.none,
            border_radius_sm: file.border_radii.sm,
        }
    }
}

/// Loads a [`FoundationUiTheme`] from a TOML file at `theme_file_path`.
///
/// A missing file or a parse error falls back to `FoundationUiTheme::default()`
/// -- the loud magenta placeholder -- rather than silently guessing a
/// plausible-looking value, so a broken theme file is obvious at a glance
/// instead of shipping unnoticed. See the module-level docs for the expected
/// TOML shape (`[colors]`, `[space]`, `[font_sizes]`, `[border_widths]`,
/// `[border_radii]` sections).
pub fn load_ui_theme_from_file(theme_file_path: impl AsRef<Path>) -> FoundationUiTheme {
    let theme_file_path = theme_file_path.as_ref();
    let theme_file_contents = match std::fs::read_to_string(theme_file_path) {
        Ok(theme_file_contents) => theme_file_contents,
        Err(error) => {
            warn!(
                "Foundation UI theme file {} could not be read ({error}); using the placeholder theme",
                theme_file_path.display()
            );
            return FoundationUiTheme::default();
        }
    };

    match toml::from_str::<FoundationUiThemeFile>(&theme_file_contents) {
        Ok(theme_file) => theme_file.into(),
        Err(error) => {
            error!(
                "Failed to parse Foundation UI theme file {} ({error}); using the placeholder theme",
                theme_file_path.display()
            );
            FoundationUiTheme::default()
        }
    }
}

/// Plugin which registers the UI-theme token types and the resolver systems
/// that apply themed wrapper components to real Bevy UI components.
///
/// Added automatically by [`FoundationPlugin`](crate::FoundationPlugin).
/// Games only need to insert their own concrete [`FoundationUiTheme`] value
/// (typically via [`load_ui_theme_from_file`]) to override the placeholder
/// this plugin installs.
pub struct FoundationUiThemePlugin;

impl Plugin for FoundationUiThemePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<FoundationUiThemedBackground>()
            .register_type::<FoundationUiThemedBorderColor>()
            .register_type::<FoundationUiThemedText>()
            .register_type::<FoundationUiThemedTypography>()
            .register_type::<FoundationUiThemedFontSize>()
            .register_type::<FoundationUiThemedPadding>()
            .register_type::<FoundationUiThemedGap>()
            .register_type::<FoundationUiThemedBorderWidth>()
            .register_type::<FoundationUiThemedBorderRadius>()
            .add_systems(
                Update,
                (
                    resolve_ui_themed_backgrounds,
                    resolve_ui_themed_border_colors,
                    resolve_ui_themed_text,
                    resolve_ui_themed_typography,
                    resolve_ui_themed_font_sizes,
                    resolve_ui_themed_padding,
                    resolve_ui_themed_gap,
                    resolve_ui_themed_border_width,
                    resolve_ui_themed_border_radius,
                )
                    .after(apply_pending_bsn_instances),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_theme() -> FoundationUiTheme {
        FoundationUiTheme {
            background: Color::srgb(0.0, 0.0, 0.0),
            surface_deepest: Color::srgb(0.1, 0.1, 0.1),
            surface_deepest_overlay: Color::srgb(0.12, 0.12, 0.12),
            surface_raised: Color::srgb(0.2, 0.2, 0.2),
            surface_raised_opaque: Color::srgb(0.22, 0.22, 0.22),
            surface: Color::srgb(0.3, 0.3, 0.3),
            surface_strong: Color::srgb(0.4, 0.4, 0.4),
            border: Color::srgb(0.5, 0.5, 0.5),
            border_hover: Color::srgb(0.55, 0.55, 0.55),
            border_disabled: Color::srgba(0.5, 0.5, 0.5, 0.5),
            surface_disabled: Color::srgba(0.3, 0.3, 0.3, 0.5),
            text_primary: Color::srgb(0.9, 0.9, 0.9),
            text_bright: Color::srgb(0.95, 0.95, 0.95),
            text_secondary: Color::srgb(0.8, 0.8, 0.8),
            text_muted: Color::srgb(0.6, 0.6, 0.6),
            text_subtle: Color::srgb(0.5, 0.5, 0.5),
            text_disabled: Color::srgb(0.45, 0.45, 0.45),
            text_on_accent: Color::srgb(0.05, 0.05, 0.05),
            accent: Color::srgb(1.0, 0.5, 0.0),
            accent_hover: Color::srgb(1.0, 0.6, 0.1),
            accent_pressed: Color::srgb(0.8, 0.4, 0.0),
            accent_soft: Color::srgba(1.0, 0.5, 0.0, 0.2),
            secondary_accent: Color::srgb(0.0, 0.5, 1.0),
            success: Color::srgb(0.2, 0.8, 0.6),
            transparent: Color::NONE,
            space_zero: 0.0,
            space_xxs: 2.0,
            space_xs: 4.0,
            space_sm: 8.0,
            space_md: 10.0,
            space_lg: 12.0,
            space_xl: 16.0,
            space_xxl: 24.0,
            font_size_xxs: 10.0,
            font_size_xs: 11.0,
            font_size_sm: 12.0,
            font_size_md: 13.0,
            font_size_lg: 14.0,
            font_size_xl: 16.0,
            font_size_xxl: 18.0,
            font_size_xxl_plus: 20.0,
            font_size_xxxl: 22.0,
            font_size_xxxl_plus: 24.0,
            font_size_xxxxl: 28.0,
            border_width_none: 0.0,
            border_width_hairline: 1.0,
            border_width_thick: 2.0,
            border_radius_none: 0.0,
            border_radius_sm: 2.0,
        }
    }

    fn unique_test_theme_file_path(test_name: &str) -> std::path::PathBuf {
        let process_id = std::process::id();
        let thread_id = format!("{:?}", std::thread::current().id());
        std::env::temp_dir().join(format!(
            "foundation-ui-theme-{test_name}-{process_id}-{thread_id}.toml"
        ))
    }

    #[test]
    fn color_accessor_resolves_every_token() {
        let theme = sample_theme();

        assert_eq!(theme.color(FoundationUiColorToken::Accent), theme.accent);
        assert_eq!(
            theme.color(FoundationUiColorToken::TextPrimary),
            theme.text_primary
        );
        assert_eq!(
            theme.color(FoundationUiColorToken::Transparent),
            theme.transparent
        );
    }

    #[test]
    fn color_alpha_overrides_only_the_alpha_channel() {
        let theme = sample_theme();

        let translucent_accent = theme.color_alpha(FoundationUiColorToken::Accent, 0.5);

        assert_eq!(translucent_accent.alpha(), 0.5);
        assert_eq!(
            translucent_accent.to_srgba().red,
            theme.accent.to_srgba().red
        );
    }

    #[test]
    fn space_accessor_resolves_every_token_in_rank_order() {
        let theme = sample_theme();

        assert!(
            theme.space(FoundationUiSpaceToken::Zero) < theme.space(FoundationUiSpaceToken::Xxs)
        );
        assert!(theme.space(FoundationUiSpaceToken::Xxs) < theme.space(FoundationUiSpaceToken::Xs));
        assert!(theme.space(FoundationUiSpaceToken::Xs) < theme.space(FoundationUiSpaceToken::Sm));
        assert!(theme.space(FoundationUiSpaceToken::Sm) < theme.space(FoundationUiSpaceToken::Md));
        assert!(theme.space(FoundationUiSpaceToken::Md) < theme.space(FoundationUiSpaceToken::Lg));
        assert!(theme.space(FoundationUiSpaceToken::Lg) < theme.space(FoundationUiSpaceToken::Xl));
        assert!(theme.space(FoundationUiSpaceToken::Xl) < theme.space(FoundationUiSpaceToken::Xxl));
    }

    #[test]
    fn font_size_accessor_resolves_every_token_in_rank_order() {
        let theme = sample_theme();

        assert!(
            theme.font_size(FoundationUiFontSizeToken::Xxs)
                < theme.font_size(FoundationUiFontSizeToken::Xs)
        );
        assert!(
            theme.font_size(FoundationUiFontSizeToken::Xs)
                < theme.font_size(FoundationUiFontSizeToken::Sm)
        );
        assert!(
            theme.font_size(FoundationUiFontSizeToken::Sm)
                < theme.font_size(FoundationUiFontSizeToken::Md)
        );
        assert!(
            theme.font_size(FoundationUiFontSizeToken::Md)
                < theme.font_size(FoundationUiFontSizeToken::Lg)
        );
        assert!(
            theme.font_size(FoundationUiFontSizeToken::Lg)
                < theme.font_size(FoundationUiFontSizeToken::Xl)
        );
        assert!(
            theme.font_size(FoundationUiFontSizeToken::Xl)
                < theme.font_size(FoundationUiFontSizeToken::Xxl)
        );
        assert!(
            theme.font_size(FoundationUiFontSizeToken::Xxl)
                < theme.font_size(FoundationUiFontSizeToken::XxlPlus)
        );
        assert!(
            theme.font_size(FoundationUiFontSizeToken::XxlPlus)
                < theme.font_size(FoundationUiFontSizeToken::Xxxl)
        );
        assert!(
            theme.font_size(FoundationUiFontSizeToken::Xxxl)
                < theme.font_size(FoundationUiFontSizeToken::XxxlPlus)
        );
        assert!(
            theme.font_size(FoundationUiFontSizeToken::XxxlPlus)
                < theme.font_size(FoundationUiFontSizeToken::Xxxxl)
        );
    }

    #[test]
    fn typography_bundles_font_size_and_color() {
        let theme = sample_theme();

        let (heading1_size, heading1_color) =
            theme.typography(FoundationUiTypographyToken::Heading1);
        assert_eq!(heading1_size, theme.font_size_xxxxl);
        assert_eq!(heading1_color, theme.text_primary);

        let (button_label_size, button_label_color) =
            theme.typography(FoundationUiTypographyToken::ButtonLabel);
        assert_eq!(button_label_size, theme.font_size_lg);
        assert_eq!(button_label_color, theme.text_primary);
    }

    #[test]
    fn default_theme_is_a_loud_placeholder_not_a_real_palette() {
        let placeholder = FoundationUiTheme::default();

        assert_eq!(
            placeholder.color(FoundationUiColorToken::Accent),
            Color::srgb(1.0, 0.0, 1.0)
        );
        assert_eq!(
            placeholder.color(FoundationUiColorToken::Background),
            Color::srgb(1.0, 0.0, 1.0)
        );
        assert_eq!(
            placeholder.space(FoundationUiSpaceToken::Zero),
            placeholder.space(FoundationUiSpaceToken::Xxl)
        );
    }

    #[test]
    fn missing_theme_file_falls_back_to_placeholder() {
        let missing_path = unique_test_theme_file_path("missing");

        let theme = load_ui_theme_from_file(&missing_path);

        assert_eq!(
            theme.color(FoundationUiColorToken::Accent),
            Color::srgb(1.0, 0.0, 1.0)
        );
    }

    #[test]
    fn malformed_theme_file_falls_back_to_placeholder() {
        let theme_file_path = unique_test_theme_file_path("malformed");
        std::fs::write(&theme_file_path, "not valid toml {{{").expect("write malformed theme file");

        let theme = load_ui_theme_from_file(&theme_file_path);

        assert_eq!(
            theme.color(FoundationUiColorToken::Accent),
            Color::srgb(1.0, 0.0, 1.0)
        );
        let _ = std::fs::remove_file(theme_file_path);
    }

    #[test]
    fn well_formed_theme_file_loads_exact_values() {
        let theme_file_path = unique_test_theme_file_path("well-formed");
        let theme_toml = r#"
[colors]
background = { r = 0.008, g = 0.024, b = 0.09, a = 1.0 }
surface_deepest = { r = 0.015, g = 0.023, b = 0.042, a = 1.0 }
surface_deepest_overlay = { r = 0.015, g = 0.023, b = 0.042, a = 0.96 }
surface_raised = { r = 0.059, g = 0.09, b = 0.165, a = 1.0 }
surface_raised_opaque = { r = 0.059, g = 0.09, b = 0.165, a = 1.0 }
surface = { r = 0.118, g = 0.161, b = 0.231, a = 1.0 }
surface_strong = { r = 0.2, g = 0.255, b = 0.333, a = 1.0 }
border = { r = 0.278, g = 0.333, b = 0.412, a = 1.0 }
border_hover = { r = 0.796, g = 0.835, b = 0.882, a = 1.0 }
border_disabled = { r = 0.2, g = 0.255, b = 0.333, a = 0.5 }
surface_disabled = { r = 0.118, g = 0.161, b = 0.231, a = 0.5 }
text_primary = { r = 0.945, g = 0.961, b = 0.976, a = 1.0 }
text_bright = { r = 0.92, g = 0.96, b = 1.0, a = 1.0 }
text_secondary = { r = 0.78, g = 0.827, b = 0.89, a = 1.0 }
text_muted = { r = 0.58, g = 0.639, b = 0.722, a = 1.0 }
text_subtle = { r = 0.5, g = 0.58, b = 0.68, a = 1.0 }
text_disabled = { r = 0.4, g = 0.44, b = 0.5, a = 1.0 }
text_on_accent = { r = 0.008, g = 0.024, b = 0.09, a = 1.0 }
accent = { r = 0.984, g = 0.749, b = 0.141, a = 1.0 }
accent_hover = { r = 1.0, g = 0.827, b = 0.32, a = 1.0 }
accent_pressed = { r = 0.854, g = 0.55, b = 0.08, a = 1.0 }
accent_soft = { r = 0.984, g = 0.749, b = 0.141, a = 0.12 }
secondary_accent = { r = 0.133, g = 0.827, b = 0.933, a = 1.0 }
success = { r = 0.204, g = 0.827, b = 0.6, a = 1.0 }
transparent = { r = 0.0, g = 0.0, b = 0.0, a = 0.0 }

[space]
zero = 0.0
xxs = 2.0
xs = 4.0
sm = 8.0
md = 10.0
lg = 12.0
xl = 16.0
xxl = 24.0

[font_sizes]
xxs = 10.0
xs = 11.0
sm = 12.0
md = 13.0
lg = 14.0
xl = 16.0
xxl = 18.0
xxl_plus = 20.0
xxxl = 22.0
xxxl_plus = 24.0
xxxxl = 28.0

[border_widths]
none = 0.0
hairline = 1.0
thick = 2.0

[border_radii]
none = 0.0
sm = 2.0
"#;
        std::fs::write(&theme_file_path, theme_toml).expect("write well-formed theme file");

        let theme = load_ui_theme_from_file(&theme_file_path);

        assert_eq!(
            theme.color(FoundationUiColorToken::Accent),
            Color::srgb(0.984, 0.749, 0.141)
        );
        assert_eq!(theme.space(FoundationUiSpaceToken::Xl), 16.0);
        assert_eq!(theme.font_size(FoundationUiFontSizeToken::Xxxxl), 28.0);
        assert_eq!(
            theme.border_width(FoundationUiBorderWidthToken::Hairline),
            1.0
        );
        assert_eq!(theme.border_radius(FoundationUiBorderRadiusToken::Sm), 2.0);

        let _ = std::fs::remove_file(theme_file_path);
    }

    #[test]
    fn resolves_themed_background() {
        let mut app = App::new();
        app.insert_resource(sample_theme());
        app.add_systems(Update, resolve_ui_themed_backgrounds);

        let entity = app
            .world_mut()
            .spawn((
                FoundationUiThemedBackground(FoundationUiColorToken::Accent),
                BackgroundColor(Color::NONE),
            ))
            .id();
        app.update();

        assert_eq!(
            app.world().get::<BackgroundColor>(entity).unwrap().0,
            sample_theme().accent
        );
    }

    #[test]
    fn resolvers_do_not_mark_components_changed_when_the_value_is_already_correct() {
        // Regression test for a real performance bug: these resolvers run
        // unconditionally every frame (see the doc comment on
        // `resolve_ui_themed_backgrounds`), and Bevy marks a component
        // "changed" on every mutable dereference regardless of whether the
        // value actually differs. Without an equality guard, every themed
        // `Node` in every scene was marked changed on every single frame,
        // forcing Bevy's UI layout engine to recompute the entire layout
        // tree every frame -- a severe, entirely avoidable slowdown that got
        // worse the more widgets a scene had. This test proves the guard is
        // in place for the `Node`-touching resolvers, which is where the
        // cost actually came from (layout recomputation, not just a color
        // write).
        let mut app = App::new();
        app.insert_resource(sample_theme());
        app.add_systems(
            Update,
            (
                resolve_ui_themed_padding,
                resolve_ui_themed_gap,
                resolve_ui_themed_border_width,
                resolve_ui_themed_border_radius,
            ),
        );

        let entity = app
            .world_mut()
            .spawn((
                FoundationUiThemedPadding {
                    horizontal: FoundationUiSpaceToken::Xl,
                    vertical: FoundationUiSpaceToken::Sm,
                },
                FoundationUiThemedGap {
                    row: FoundationUiSpaceToken::Md,
                    column: FoundationUiSpaceToken::Lg,
                },
                FoundationUiThemedBorderWidth(FoundationUiBorderWidthToken::Thick),
                FoundationUiThemedBorderRadius(FoundationUiBorderRadiusToken::Sm),
                Node::default(),
            ))
            .id();

        // First frame: the value is wrong (freshly spawned default), so the
        // resolvers must write and Node is correctly marked changed.
        app.update();
        let node_change_tick_after_first_write = app
            .world()
            .entity(entity)
            .get_change_ticks::<Node>()
            .unwrap()
            .changed;

        // Second frame: the value already matches the theme, so nothing
        // should write to Node again, and its change tick must not advance.
        app.update();
        let node_change_tick_after_second_frame = app
            .world()
            .entity(entity)
            .get_change_ticks::<Node>()
            .unwrap()
            .changed;

        assert_eq!(
            node_change_tick_after_first_write, node_change_tick_after_second_frame,
            "Node must not be marked changed on a frame where its themed values already matched \
             the theme -- an unconditional write here forces Bevy's UI layout engine to recompute \
             the whole layout tree every frame for no reason"
        );
    }

    #[test]
    fn resolves_themed_border_color() {
        let mut app = App::new();
        app.insert_resource(sample_theme());
        app.add_systems(Update, resolve_ui_themed_border_colors);

        let entity = app
            .world_mut()
            .spawn((
                FoundationUiThemedBorderColor(FoundationUiColorToken::Border),
                BorderColor::all(Color::NONE),
            ))
            .id();
        app.update();

        let border_color = app.world().get::<BorderColor>(entity).unwrap();
        assert_eq!(border_color.top, sample_theme().border);
        assert_eq!(border_color.left, sample_theme().border);
    }

    #[test]
    fn resolves_themed_text_color() {
        let mut app = App::new();
        app.insert_resource(sample_theme());
        app.add_systems(Update, resolve_ui_themed_text);

        let entity = app
            .world_mut()
            .spawn((
                FoundationUiThemedText(FoundationUiColorToken::TextMuted),
                TextColor(Color::NONE),
            ))
            .id();
        app.update();

        assert_eq!(
            app.world().get::<TextColor>(entity).unwrap().0,
            sample_theme().text_muted
        );
    }

    #[test]
    fn resolves_themed_typography() {
        let mut app = App::new();
        app.insert_resource(sample_theme());
        app.add_systems(Update, resolve_ui_themed_typography);

        let entity = app
            .world_mut()
            .spawn((
                FoundationUiThemedTypography(FoundationUiTypographyToken::Heading3),
                TextFont::default(),
                TextColor(Color::NONE),
            ))
            .id();
        app.update();

        let (expected_size, expected_color) =
            sample_theme().typography(FoundationUiTypographyToken::Heading3);
        assert_eq!(
            app.world().get::<TextFont>(entity).unwrap().font_size,
            FontSize::Px(expected_size)
        );
        assert_eq!(
            app.world().get::<TextColor>(entity).unwrap().0,
            expected_color
        );
    }

    #[test]
    fn resolves_themed_font_size_only() {
        let mut app = App::new();
        app.insert_resource(sample_theme());
        app.add_systems(Update, resolve_ui_themed_font_sizes);

        let entity = app
            .world_mut()
            .spawn((
                FoundationUiThemedFontSize(FoundationUiFontSizeToken::Xxl),
                TextFont::default(),
            ))
            .id();
        app.update();

        assert_eq!(
            app.world().get::<TextFont>(entity).unwrap().font_size,
            FontSize::Px(sample_theme().font_size_xxl)
        );
    }

    #[test]
    fn resolves_themed_padding() {
        let mut app = App::new();
        app.insert_resource(sample_theme());
        app.add_systems(Update, resolve_ui_themed_padding);

        let entity = app
            .world_mut()
            .spawn((
                FoundationUiThemedPadding {
                    horizontal: FoundationUiSpaceToken::Xl,
                    vertical: FoundationUiSpaceToken::Sm,
                },
                Node::default(),
            ))
            .id();
        app.update();

        let node = app.world().get::<Node>(entity).unwrap();
        assert_eq!(
            node.padding,
            UiRect::axes(
                Val::Px(sample_theme().space_xl),
                Val::Px(sample_theme().space_sm)
            )
        );
    }

    #[test]
    fn resolves_themed_gap() {
        let mut app = App::new();
        app.insert_resource(sample_theme());
        app.add_systems(Update, resolve_ui_themed_gap);

        let entity = app
            .world_mut()
            .spawn((
                FoundationUiThemedGap {
                    row: FoundationUiSpaceToken::Md,
                    column: FoundationUiSpaceToken::Lg,
                },
                Node::default(),
            ))
            .id();
        app.update();

        let node = app.world().get::<Node>(entity).unwrap();
        assert_eq!(node.row_gap, Val::Px(sample_theme().space_md));
        assert_eq!(node.column_gap, Val::Px(sample_theme().space_lg));
    }

    #[test]
    fn resolves_themed_border_width() {
        let mut app = App::new();
        app.insert_resource(sample_theme());
        app.add_systems(Update, resolve_ui_themed_border_width);

        let entity = app
            .world_mut()
            .spawn((
                FoundationUiThemedBorderWidth(FoundationUiBorderWidthToken::Thick),
                Node::default(),
            ))
            .id();
        app.update();

        let node = app.world().get::<Node>(entity).unwrap();
        assert_eq!(
            node.border,
            UiRect::all(Val::Px(sample_theme().border_width_thick))
        );
    }

    #[test]
    fn resolves_themed_border_radius() {
        let mut app = App::new();
        app.insert_resource(sample_theme());
        app.add_systems(Update, resolve_ui_themed_border_radius);

        let entity = app
            .world_mut()
            .spawn((
                FoundationUiThemedBorderRadius(FoundationUiBorderRadiusToken::Sm),
                Node::default(),
            ))
            .id();
        app.update();

        let node = app.world().get::<Node>(entity).unwrap();
        assert_eq!(
            node.border_radius,
            BorderRadius::all(Val::Px(sample_theme().border_radius_sm))
        );
    }

    #[test]
    fn changing_the_theme_resource_re_skins_already_spawned_widgets() {
        let mut app = App::new();
        app.insert_resource(sample_theme());
        app.add_systems(Update, resolve_ui_themed_backgrounds);

        let entity = app
            .world_mut()
            .spawn((
                FoundationUiThemedBackground(FoundationUiColorToken::Surface),
                BackgroundColor(Color::NONE),
            ))
            .id();
        app.update();
        assert_eq!(
            app.world().get::<BackgroundColor>(entity).unwrap().0,
            sample_theme().surface
        );

        let swapped_surface_color = Color::srgb(0.99, 0.01, 0.5);
        app.world_mut().resource_mut::<FoundationUiTheme>().surface = swapped_surface_color;
        app.update();

        assert_eq!(
            app.world().get::<BackgroundColor>(entity).unwrap().0,
            swapped_surface_color,
            "live theme changes should re-skin already-spawned widgets on the next frame"
        );
    }
}
