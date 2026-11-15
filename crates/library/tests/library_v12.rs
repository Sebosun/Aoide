//! Schema v12: sites that needed the user.
use aoide_library::{Library, SiteRow};

fn row(host: &str) -> SiteRow {
    SiteRow {
        host: host.into(),
        signed_in_at: None,
        check_passed_at: None,
        last_wall: None,
        last_wall_at: None,
    }
}

#[test]
fn a_version_11_library_gains_an_empty_sites_table() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite");
    drop(Library::open(&path).unwrap());
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("DROP TABLE sites; PRAGMA user_version = 11;")
        .unwrap();
    drop(conn);
    let lib = Library::open(&path).unwrap();
    assert_eq!(lib.sites().unwrap(), []);
    let conn = rusqlite::Connection::open(&path).unwrap();
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, 12);
}

#[test]
fn walls_sign_ins_and_checks_are_kept_per_site_until_signed_out_or_forgotten() {
    let dir = tempfile::tempdir().unwrap();
    let lib = Library::open(&dir.path().join("library.sqlite")).unwrap();
    lib.site_wall("quay.example", "signIn", 10).unwrap();
    lib.site_wall("quay.example", "paywall", 20).unwrap();
    lib.site_signed_in("quay.example", 30).unwrap();
    lib.site_check_passed("ferry.example", 40).unwrap();
    lib.add_site("anchor.example").unwrap();
    lib.add_site("quay.example").unwrap();
    assert_eq!(
        lib.sites().unwrap(),
        [
            row("anchor.example"),
            SiteRow {
                check_passed_at: Some(40),
                ..row("ferry.example")
            },
            SiteRow {
                signed_in_at: Some(30),
                last_wall: Some("paywall".into()),
                last_wall_at: Some(20),
                ..row("quay.example")
            },
        ]
    );

    lib.sign_out_site("quay.example").unwrap();
    let quay = lib.sites().unwrap().pop().unwrap();
    assert_eq!((quay.signed_in_at, quay.last_wall_at), (None, Some(20)));
    lib.sign_out_sites().unwrap();
    assert!(lib
        .sites()
        .unwrap()
        .iter()
        .all(|s| s.check_passed_at.is_none()));
    lib.forget_site("ferry.example").unwrap();
    let hosts: Vec<_> = lib.sites().unwrap().into_iter().map(|s| s.host).collect();
    assert_eq!(hosts, ["anchor.example", "quay.example"]);
}
