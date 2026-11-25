use tinyhand_core::automation::*;
use tinyhand_core::custom_hands::CustomHand;

fn args(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

#[test]
fn simple_commands() {
    assert_eq!(parse_url("tinyhand://on"), Some(Command::On));
    assert_eq!(parse_url("tinyhand://off"), Some(Command::Off));
    assert_eq!(parse_url("tinyhand://toggle"), Some(Command::Toggle));
    assert_eq!(parse_url("tinyhand://clear"), Some(Command::Clear));
    assert_eq!(parse_url("tinyhand://toggle/"), Some(Command::Toggle));
}

#[test]
fn case_and_decoration() {
    assert_eq!(parse_url("TinyHand://TOGGLE"), Some(Command::Toggle));
    assert_eq!(parse_url("tinyhand://on?x=1"), Some(Command::On));
    assert_eq!(parse_url("tinyhand://clear#frag"), Some(Command::Clear));
}

#[test]
fn style_commands() {
    assert_eq!(parse_url("tinyhand://style/lavender"), Some(Command::Style("lavender".into())));
    // The id keeps its case, as in Swift.
    assert_eq!(parse_url("tinyhand://style/Custom-AB"), Some(Command::Style("Custom-AB".into())));
    assert_eq!(parse_url("tinyhand://style/custom-0a1b2c3d/extra"), Some(Command::Style("custom-0a1b2c3d".into())));
    assert_eq!(parse_url("tinyhand://style/my%20hand"), Some(Command::Style("my hand".into())));
    assert_eq!(parse_url("tinyhand://style"), None);
    assert_eq!(parse_url("tinyhand://style/"), None);
}

#[test]
fn sound_commands() {
    assert_eq!(parse_url("tinyhand://sound/on"), Some(Command::Sound(SoundAction::On)));
    assert_eq!(parse_url("tinyhand://sound/off"), Some(Command::Sound(SoundAction::Off)));
    assert_eq!(parse_url("tinyhand://sound/toggle"), Some(Command::Sound(SoundAction::Toggle)));
    assert_eq!(parse_url("tinyhand://sound"), Some(Command::Sound(SoundAction::Toggle)));
    assert_eq!(parse_url("tinyhand://sound/loud"), None);
}

#[test]
fn rejects_other_urls() {
    assert_eq!(parse_url("http://toggle"), None);
    assert_eq!(parse_url("tinyhand://"), None);
    assert_eq!(parse_url("tinyhand://explode"), None);
    assert_eq!(parse_url("toggle"), None);
    assert_eq!(parse_url(""), None);
    assert_eq!(parse_url("tinyhand://on/%zz/%"), Some(Command::On));
}

#[test]
fn cli_words() {
    assert_eq!(parse_args(&args(&["toggle"])), Some(Command::Toggle));
    assert_eq!(parse_args(&args(&["ON"])), Some(Command::On));
    assert_eq!(parse_args(&args(&["clear"])), Some(Command::Clear));
    assert_eq!(parse_args(&args(&["style", "robot"])), Some(Command::Style("robot".into())));
    assert_eq!(parse_args(&args(&["style"])), None);
    assert_eq!(parse_args(&args(&["sound", "off"])), Some(Command::Sound(SoundAction::Off)));
    assert_eq!(parse_args(&args(&["sound"])), Some(Command::Sound(SoundAction::Toggle)));
    assert_eq!(parse_args(&args(&["sound", "x"])), None);
    assert_eq!(parse_args(&args(&["tinyhand://style/glove"])), Some(Command::Style("glove".into())));
    assert_eq!(parse_args(&[]), None);
    assert_eq!(parse_args(&args(&["dance"])), None);
}

#[test]
fn state_file_round_trip() {
    let dir = std::env::temp_dir().join(format!("tinyhand-state-{}", std::process::id()));
    let path = dir.join("nested/state.json");
    let custom = vec![CustomHand { id: "custom-00000001".into(), name: "Mine".into(), ink: 5 }];
    let state = State::new(true, "lavender", false, &custom);
    assert_eq!(state.pid, std::process::id());
    assert_eq!(state.styles.len(), 41);
    write_state(&path, &state).unwrap();

    let text = std::fs::read_to_string(&path).unwrap();
    // Sorted keys, pretty printed.
    let keys: Vec<usize> = ["\"active\"", "\"pid\"", "\"soundOn\"", "\"style\"", "\"styles\""]
        .iter()
        .map(|k| text.find(k).unwrap())
        .collect();
    assert!(keys.windows(2).all(|w| w[0] < w[1]), "{text}");
    assert!(text.contains('\n'));

    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["active"], true);
    assert_eq!(v["soundOn"], false);
    assert_eq!(v["style"], "lavender");
    assert_eq!(v["styles"][0], serde_json::json!({"id": "lavender", "name": "Lavender toy", "type": "hand"}));
    assert_eq!(v["styles"][39], serde_json::json!({"id": "custom-00000001", "name": "Mine", "type": "custom"}));
    assert_eq!(v["styles"][40], serde_json::json!({"id": "laser", "name": "Laser dot", "type": "laser"}));
    assert_eq!(serde_json::from_str::<State>(&text).unwrap(), state);

    remove_state(&path).unwrap();
    assert!(!path.exists());
    remove_state(&path).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
}
