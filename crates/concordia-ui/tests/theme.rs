//! Themes: `theme_resolves_inline_then_state_then_base_then_default` and
//! `theme_file_reads_validates_and_imports`.
//!
//! Resolution, per field: inline `UiStyle`, then the class's style for the widget's state, then
//! the class's base, then (text fields only) the parent's resolved value, then the built-in
//! default (`ResolvedStyle::default()`: opacity 1, radius 0, Noto Sans 16 px, weight 400, white).

use concordia_ui::*;

const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

fn class() -> ThemeClass {
    ThemeClass {
        base: UiStyle {
            background: Some(Background::Solid(RED)),
            opacity: Some(0.5),
            font_size: Some(20.0),
            radius: Some([4.0; 4]),
            ..UiStyle::default()
        },
        hover: UiStyle {
            background: Some(Background::Solid(GREEN)),
            ..UiStyle::default()
        },
        ..ThemeClass::default()
    }
}

#[test]
fn theme_resolves_inline_then_state_then_base_then_default() {
    let class = class();
    // Normal: the base's red, 0.5, 20 px, radius 4; the rest are defaults.
    let normal = resolve(None, Some(&class), WidgetState::Normal, None);
    assert_eq!(normal.background, Some(Background::Solid(RED)));
    assert_eq!(normal.opacity, 0.5);
    assert_eq!(normal.font_size, 20.0);
    assert_eq!(normal.radius, [4.0; 4]);
    assert_eq!(normal.weight, 400.0);
    assert_eq!(normal.font.to_string(), SANS);
    // Hover: the state's green wins over the base's red.
    let hover = resolve(None, Some(&class), WidgetState::Hover, None);
    assert_eq!(hover.background, Some(Background::Solid(GREEN)));
    assert_eq!(hover.opacity, 0.5);
    // Inline blue wins over the state's green; inline opacity over the base's.
    let inline = UiStyle {
        background: Some(Background::Solid(BLUE)),
        opacity: Some(0.25),
        ..UiStyle::default()
    };
    let both = resolve(Some(&inline), Some(&class), WidgetState::Hover, None);
    assert_eq!(both.background, Some(Background::Solid(BLUE)));
    assert_eq!(both.opacity, 0.25);
    assert_eq!(both.font_size, 20.0);
    // No class, no inline: every default.
    assert_eq!(
        resolve(None, None, WidgetState::Hover, None),
        ResolvedStyle::default()
    );
}

/// Text fields inherit from the parent; boxes don't (CSS inheritance). The parent's 30 px, bold,
/// blue reach the child; its background and opacity 0.2 don't.
#[test]
fn text_fields_inherit_from_the_parent_box_fields_do_not() {
    let parent = ResolvedStyle {
        background: Some(Background::Solid(RED)),
        opacity: 0.2,
        font_size: 30.0,
        weight: 700.0,
        color: BLUE,
        ..ResolvedStyle::default()
    };
    let child = resolve(None, None, WidgetState::Normal, Some(&parent));
    assert_eq!(child.font_size, 30.0);
    assert_eq!(child.weight, 700.0);
    assert_eq!(child.color, BLUE);
    assert_eq!(child.background, None);
    assert_eq!(child.opacity, 1.0);
    // A class's base still beats the parent: 20 px.
    let themed = resolve(None, Some(&class()), WidgetState::Normal, Some(&parent));
    assert_eq!(themed.font_size, 20.0);
}

const THEME: &str = r#"Theme(
    classes: {
        "button": (
            base: (background: Some(Solid((0.1, 0.1, 0.1, 1.0))), radius: Some((6.0, 6.0, 6.0, 6.0))),
            hover: (background: Some(Solid((0.2, 0.2, 0.2, 1.0)))),
        ),
    },
)
"#;

#[test]
fn theme_file_reads_validates_and_imports() {
    let theme = read_theme(THEME).unwrap();
    let button = &theme.classes["button"];
    assert_eq!(button.base.radius, Some([6.0; 4]));
    assert_eq!(
        button.hover.background,
        Some(Background::Solid([0.2, 0.2, 0.2, 1.0]))
    );
    // Canonical text reads back to the same theme.
    let text = write_theme(&theme).unwrap();
    assert_eq!(read_theme(&text).unwrap(), theme);
    // A bad style names its class, state and field.
    let bad = THEME.replace("hover: (", "hover: (opacity: Some(2.0), ");
    let err = read_theme(&bad).unwrap_err();
    assert_eq!(
        err.field_name(),
        Some("classes/button/hover/opacity"),
        "{err}"
    );
    // An empty class name is refused.
    let empty = THEME.replace("\"button\"", "\"\"");
    assert_eq!(
        read_theme(&empty).unwrap_err().field_name(),
        Some("classes")
    );
}

#[test]
fn theme_importer_writes_a_theme_artifact() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("ui")).unwrap();
    std::fs::write(dir.path().join("ui/main.theme.ron"), THEME).unwrap();
    let ids = Box::new(concordia_core::id::DeterministicIdGen::new(7));
    let mut db = concordia_assets::AssetDb::new(dir.path(), ids).unwrap();
    db.register(std::sync::Arc::new(ThemeImporter));
    let id = db
        .import(std::path::Path::new("ui/main.theme.ron"))
        .unwrap()
        .id;
    let reference: concordia_assets::AssetRef = format!("asset:{id}").parse().unwrap();
    let theme: Theme = db
        .load(&reference, concordia_assets::AssetKind::Theme)
        .unwrap();
    assert_eq!(theme, read_theme(THEME).unwrap());
}
