//! Settings: Swift defaults, JSON keys, shortcuts, file handling and the macOS import.

use std::path::PathBuf;
use tinyhand_core::particles::TapEffect;
use tinyhand_core::settings::*;

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("tinyhand-settings-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

#[test]
fn defaults_match_swift() {
    let s = Settings::default();
    assert_eq!(s.toggle_shortcut.to_string(), "ctrl+alt+h");
    assert_eq!(s.clear_shortcut.to_string(), "ctrl+alt+x");
    assert_eq!(s.pointer_size, 0.13);
    assert!(s.show_shadow);
    assert_eq!(s.fade_seconds, 3.0);
    assert!(s.hand_sound);
    assert!(!s.laser_sound);
    assert_eq!(s.tap_sound, TapSound::Thock);
    assert_eq!(s.tap_effect, TapEffect::FairyDust);
    assert!(!s.interact_keeps_hand);
    assert!(s.double_shift_click_through);
    assert_eq!(s.interact_key, InteractKey::Option);
    assert_eq!(s.ink_choice, InkChoice::Match);
    assert!(s.show_toolbar && s.show_ink_button);
    assert_eq!(s.pinned_styles, ["butterfly3d", "lavender", "glove", "coral", "plush", "robot", "laser"]);
    assert_eq!(s.pointer_style, "butterfly3d");
    assert!(!s.welcome_shown && !s.first_run_hint_shown && !s.login_item_set_up);
}

#[test]
fn json_uses_swift_key_names() {
    let v: serde_json::Value = serde_json::from_str(&Settings::default().to_json()).unwrap();
    let o = v.as_object().unwrap();
    for k in [
        "shortcut.toggle", "shortcut.clear", "pointerSize", "showShadow", "fadeSeconds", "sound.hands",
        "sound.laser", "tapSound", "tapEffect", "interactKeepsHand", "doubleShiftClickThrough",
        "interactKey", "inkChoice", "showToolbar", "showInkButton", "pinnedStyles", "pointerStyle",
        "welcomeShown", "firstRunHintShown", "loginItemSetUp",
    ] {
        assert!(o.contains_key(k), "missing {k}");
    }
    assert_eq!(o["shortcut.toggle"], "ctrl+alt+h");
    assert_eq!(o["tapEffect"], "fairyDust");
    assert_eq!(o["tapSound"], "thock");
    assert_eq!(o["interactKey"], "option");
    assert_eq!(o["inkChoice"], "match");
}

#[test]
fn json_round_trip() {
    let mut s = Settings::default();
    s.toggle_shortcut = "shift+cmd+space".parse().unwrap();
    s.pointer_size = 0.2;
    s.tap_effect = TapEffect::FairyDustSubtle;
    s.tap_sound = TapSound::Quack;
    s.ink_choice = InkChoice::Orange;
    s.interact_key = InteractKey::Off;
    s.pinned_styles = vec!["pink".into(), "laser".into()];
    s.pointer_style = "lemon".into();
    s.welcome_shown = true;
    assert_eq!(Settings::from_json(&s.to_json()), s);
}

#[test]
fn missing_and_unknown_keys_are_fine() {
    let s = Settings::from_json(r#"{"pointerSize": 0.1, "somethingNew": [1,2], "tapSound": "coin"}"#);
    assert_eq!(s.pointer_size, 0.1);
    assert_eq!(s.tap_sound, TapSound::Coin);
    assert!(s.show_shadow);
    assert_eq!(Settings::from_json("{}"), Settings::default());
}

#[test]
fn a_bad_value_only_loses_its_own_key() {
    let s = Settings::from_json(
        r#"{"pointerSize": "big", "tapSound": "nope", "shortcut.toggle": "ctrl+alt+bogus", "showShadow": false, "tapEffect": "off"}"#,
    );
    assert_eq!(s.pointer_size, 0.13);
    assert_eq!(s.tap_sound, TapSound::Thock);
    assert_eq!(s.toggle_shortcut, Shortcut::toggle_default());
    assert!(!s.show_shadow);
    assert_eq!(s.tap_effect, TapEffect::Off);
}

#[test]
fn corrupt_or_missing_file_gives_defaults() {
    let d = temp("corrupt");
    std::fs::create_dir_all(&d).unwrap();
    assert_eq!(Settings::load_from(&d.join("nope.json")), Settings::default());
    for junk in [&b"{not json"[..], b"", b"[1,2,3]", b"\xff\xfe\x00", b"null"] {
        std::fs::write(d.join("s.json"), junk).unwrap();
        assert_eq!(Settings::load_from(&d.join("s.json")), Settings::default());
    }
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn save_and_load_through_a_file() {
    let d = temp("save");
    let path = d.join("deep/settings.json");
    let mut s = Settings::default();
    s.fade_seconds = 0.0;
    s.save_to(&path).unwrap();
    assert_eq!(Settings::load_from(&path), s);
    s.fade_seconds = 7.5;
    s.save_to(&path).unwrap();
    assert_eq!(Settings::load_from(&path).fade_seconds, 7.5);
    let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap()).unwrap().collect();
    assert_eq!(leftovers.len(), 1, "temp file left behind");
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn pinned_styles_rules() {
    let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(normalise_pinned(&v(&["a", "b", "a", "c", "b"])), v(&["a", "b", "c"]));
    assert_eq!(normalise_pinned(&[]), Settings::default().pinned_styles);
    let many: Vec<String> = (0..12).map(|i| format!("s{i}")).collect();
    assert_eq!(normalise_pinned(&many).len(), 8);
    let s = Settings::from_json(r#"{"pinnedStyles": ["pink","pink"]}"#);
    assert_eq!(s.pinned_styles, ["pink"]);
    let s = Settings::from_json(r#"{"pinnedStyles": []}"#);
    assert_eq!(s.pinned_styles, Settings::default().pinned_styles);
    // Names that no longer exist drop out when checked against the catalogue.
    let s = Settings::from_json(r#"{"pinnedStyles": ["pink","custom-gone","laser"]}"#);
    assert_eq!(s.pinned_existing(&[]), ["pink", "laser"]);
    let s = Settings::from_json(r#"{"pinnedStyles": ["custom-gone"]}"#);
    assert_eq!(s.pinned_existing(&[]), Settings::default().pinned_styles);
}

#[test]
fn legacy_pointer_styles_migrate() {
    assert_eq!(Settings::from_json(r#"{"pointerStyle":"dot"}"#).pointer_style, "laser");
    assert_eq!(Settings::from_json(r#"{"pointerStyle":"sunny"}"#).pointer_style, "coral");
    assert_eq!(Settings::from_json(r#"{"pointerStyle":"pink"}"#).pointer_style, "pink");
    let s = Settings::from_json(r#"{"pointerStyle":"gone"}"#);
    assert_eq!(s.style_existing(&[]), "butterfly3d");
}

#[test]
fn reset_all_keeps_the_three_flags() {
    let mut s = Settings::default();
    s.pointer_size = 0.2;
    s.show_shadow = false;
    s.toggle_shortcut = "cmd+k".parse().unwrap();
    s.pinned_styles = vec!["pink".into()];
    s.pointer_style = "pink".into();
    s.tap_effect = TapEffect::Off;
    s.welcome_shown = true;
    s.first_run_hint_shown = true;
    s.login_item_set_up = true;
    s.reset_all();
    let expected = Settings {
        welcome_shown: true,
        first_run_hint_shown: true,
        login_item_set_up: true,
        ..Settings::default()
    };
    assert_eq!(s, expected);
}

#[test]
fn shortcut_text_round_trip() {
    for t in ["ctrl+alt+h", "shift+cmd+space", "ctrl+alt+shift+cmd+f12", "alt+left", "ctrl++", "cmd+/"] {
        let s: Shortcut = t.parse().unwrap();
        assert_eq!(s.to_string(), t);
    }
    // Order, case and alias forms normalise.
    let s: Shortcut = "Option+Control+H".parse().unwrap();
    assert_eq!(s.to_string(), "ctrl+alt+h");
    let s: Shortcut = "Super+Enter".parse().unwrap();
    assert_eq!(s.to_string(), "cmd+enter");
    for bad in ["", "ctrl+", "ctrl+alt+bogus", "hyper+h", "ctrl+f13", "ctrl+f0", "ctrl+ab"] {
        assert!(bad.parse::<Shortcut>().is_err(), "{bad}");
    }
}

#[test]
fn shortcut_display_both_forms() {
    let s: Shortcut = "ctrl+alt+h".parse().unwrap();
    assert_eq!(s.display(true), "⌃⌥H");
    assert_eq!(s.display(false), "Ctrl+Alt+H");
    let s: Shortcut = "ctrl+alt+shift+cmd+space".parse().unwrap();
    assert_eq!(s.display(true), "⌃⌥⇧⌘Space");
    assert_eq!(s.display(false), "Ctrl+Alt+Shift+Super+Space");
    let cases = [
        ("enter", "↩", "Enter"), ("tab", "⇥", "Tab"), ("backspace", "⌫", "Backspace"),
        ("delete", "⌦", "Delete"), ("left", "←", "Left"), ("right", "→", "Right"),
        ("up", "↑", "Up"), ("down", "↓", "Down"), ("home", "↖", "Home"), ("end", "↘", "End"),
        ("pageup", "⇞", "PageUp"), ("pagedown", "⇟", "PageDown"), ("f1", "F1", "F1"),
        ("f12", "F12", "F12"), ("escape", "⎋", "Esc"),
    ];
    for (key, mac, other) in cases {
        let s: Shortcut = format!("cmd+{key}").parse().unwrap();
        assert_eq!(s.display(true), format!("⌘{mac}"), "{key}");
        assert_eq!(s.display(false), format!("Super+{other}"), "{key}");
    }
}

#[test]
fn shortcut_required_modifier() {
    assert!("ctrl+h".parse::<Shortcut>().unwrap().has_required_modifier());
    assert!("alt+h".parse::<Shortcut>().unwrap().has_required_modifier());
    assert!("cmd+h".parse::<Shortcut>().unwrap().has_required_modifier());
    assert!(!"shift+h".parse::<Shortcut>().unwrap().has_required_modifier());
    assert!(!"h".parse::<Shortcut>().unwrap().has_required_modifier());
}

#[test]
fn enum_values_match_swift_raw_values() {
    fn j<T: serde::Serialize>(v: &T) -> String {
        serde_json::to_string(v).unwrap()
    }
    assert_eq!(j(&InteractKey::Command), "\"command\"");
    assert_eq!(j(&InkChoice::Purple), "\"purple\"");
    assert_eq!(j(&TapSound::Typewriter), "\"typewriter\"");
    assert_eq!(InteractKey::ALL.len(), 4);
    assert_eq!(InkChoice::ALL.len(), 9);
    assert_eq!(TapSound::ALL.len(), 15);
    for t in TapSound::ALL {
        assert_eq!(j(&t), format!("\"{}\"", t.id()));
    }
    assert_eq!(TapSound::Wood.title(), "Wood block");
    assert_eq!(tap_effect_title(TapEffect::FairyDustSubtle), "Fairy dust, subtle");
    assert_eq!(InteractKey::Option.symbol(), "⌥");
}

#[test]
fn ink_colours() {
    assert_eq!(InkChoice::Match.colour(), None);
    assert_eq!(InkChoice::Red.colour(), Some(0xFF291F));
    assert_eq!(InkChoice::Orange.colour(), Some(0xFF8A00));
    assert_eq!(InkChoice::Black.colour(), Some(0x1D1D21));
    assert_eq!(InkChoice::Match.resolved(0x123456), 0x123456);
    assert_eq!(InkChoice::White.resolved(0x123456), 0xFFFFFF);
}

#[test]
fn config_dir_is_per_os() {
    let d = config_dir();
    let name = d.file_name().unwrap().to_string_lossy().into_owned();
    if cfg!(target_os = "linux") {
        assert_eq!(name, "tinyhand");
    } else {
        assert_eq!(name, "TinyHand");
    }
    if cfg!(target_os = "macos") {
        assert!(d.ends_with("Library/Application Support/TinyHand"));
    }
    assert_eq!(settings_path(), d.join("settings.json"));
}

const SAMPLE_PLIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>NSWindowFrame</key>
	<string>ignored</string>
	<key>fadeSeconds</key>
	<integer>5</integer>
	<key>interactKey</key>
	<string>command</string>
	<key>inkChoice</key>
	<string>blue</string>
	<key>pinnedStyles</key>
	<array>
		<string>pink</string>
		<string>laser</string>
		<string>pink</string>
	</array>
	<key>pointerSize</key>
	<real>0.17</real>
	<key>pointerStyle</key>
	<string>dot</string>
	<key>shortcut.clear</key>
	<dict>
		<key>display</key>
		<string>⌘⇧K</string>
		<key>keyCode</key>
		<integer>40</integer>
		<key>modifiers</key>
		<integer>768</integer>
	</dict>
	<key>shortcut.toggle</key>
	<dict>
		<key>display</key>
		<string>⌃⌥Space</string>
		<key>keyCode</key>
		<integer>49</integer>
		<key>modifiers</key>
		<integer>6144</integer>
	</dict>
	<key>showShadow</key>
	<false/>
	<key>sound.hands</key>
	<false/>
	<key>tapEffect</key>
	<string>fairyDustSubtle</string>
	<key>tapSound</key>
	<string>pop</string>
	<key>welcomeShown</key>
	<true/>
</dict>
</plist>"#;

#[test]
fn imports_macos_defaults() {
    let s = import_macos_defaults(SAMPLE_PLIST.as_bytes());
    assert_eq!(s.toggle_shortcut.to_string(), "ctrl+alt+space");
    assert_eq!(s.clear_shortcut.to_string(), "shift+cmd+k");
    assert_eq!(s.fade_seconds, 5.0);
    assert_eq!(s.interact_key, InteractKey::Command);
    assert_eq!(s.ink_choice, InkChoice::Blue);
    assert_eq!(s.pinned_styles, ["pink", "laser"]);
    assert!((s.pointer_size - 0.17).abs() < 1e-6);
    assert_eq!(s.pointer_style, "laser");
    assert!(!s.show_shadow);
    assert!(!s.hand_sound);
    assert_eq!(s.tap_effect, TapEffect::FairyDustSubtle);
    assert_eq!(s.tap_sound, TapSound::Pop);
    assert!(s.welcome_shown);
    // Not in the sample: defaults.
    assert!(s.show_toolbar);
    assert!(!s.laser_sound);
}

#[test]
fn import_keeps_defaults_for_unknown_or_bad_input() {
    assert_eq!(import_macos_defaults(b"garbage"), Settings::default());
    assert_eq!(import_macos_defaults(b""), Settings::default());
    let odd = SAMPLE_PLIST
        .replace("<integer>49</integer>", "<integer>999</integer>")
        .replace("<string>command</string>", "<string>hyper</string>");
    let s = import_macos_defaults(odd.as_bytes());
    assert_eq!(s.toggle_shortcut, Shortcut::toggle_default());
    assert_eq!(s.interact_key, InteractKey::Option);
}

#[test]
fn carbon_key_table() {
    assert_eq!(carbon_key_name(0x00), Some("a"));
    assert_eq!(carbon_key_name(0x04), Some("h"));
    assert_eq!(carbon_key_name(0x07), Some("x"));
    assert_eq!(carbon_key_name(0x1D), Some("0"));
    assert_eq!(carbon_key_name(0x12), Some("1"));
    assert_eq!(carbon_key_name(0x35), Some("escape"));
    assert_eq!(carbon_key_name(0x31), Some("space"));
    assert_eq!(carbon_key_name(0x24), Some("enter"));
    assert_eq!(carbon_key_name(0x33), Some("backspace"));
    assert_eq!(carbon_key_name(0x75), Some("delete"));
    assert_eq!(carbon_key_name(0x7B), Some("left"));
    assert_eq!(carbon_key_name(0x7E), Some("up"));
    assert_eq!(carbon_key_name(0x7A), Some("f1"));
    assert_eq!(carbon_key_name(0x6F), Some("f12"));
    assert_eq!(carbon_key_name(0x7F), None);
    // Every letter and digit appears exactly once.
    let names: Vec<_> = (0..0x80).filter_map(carbon_key_name).collect();
    for c in ('a'..='z').chain('0'..='9') {
        assert_eq!(names.iter().filter(|n| **n == c.to_string()).count(), 1, "{c}");
    }
    let s = shortcut_from_carbon(0x04, 0x1000 | 0x800).unwrap();
    assert_eq!(s, Shortcut::toggle_default());
}
