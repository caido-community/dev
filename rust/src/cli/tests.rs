use super::parse;

#[test]
fn rejects_unknown_subcommands() {
    assert!(parse(vec!["unknown".into()]).is_err());
    assert!(parse(vec![]).is_err());
    assert!(parse(vec!["build".into(), "--config".into()]).is_err());
}
