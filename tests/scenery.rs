#![cfg(feature = "gui")]
use realflow::scenery::{catalog, community_from_usercfg, discover_packages, icao, matches};
use std::fs;

#[test]
fn bundled_catalog_has_valid_icao_and_sources() {
    let items = catalog().unwrap();
    assert!(items.len() >= 3);
    assert_eq!(matches(&items, "patk").len(), 1);
    assert_eq!(matches(&items, "AK44").len(), 1);
    assert_eq!(matches(&items, "SAQU").len(), 1);
    assert!(matches(&items, "UUEE").is_empty());
}

#[test]
fn airport_codes_are_normalized_and_validated() {
    assert_eq!(icao(" patk ").unwrap(), "PATK");
    assert_eq!(icao("2ak7").unwrap(), "2AK7");
    for bad in ["", "p", "PATKK", "../x", "KA F", "éééé"] {
        assert!(icao(bad).is_err(), "{bad} should be invalid");
    }
}

#[test]
fn cfg_path_picks_active_msfs_install_and_handles_spaces() {
    let data = "SomeSetting 1\nInstalledPackagesPath \"D:\\MSFS 2020\\Packages\"\n";
    assert_eq!(
        community_from_usercfg(data).unwrap().to_string_lossy(),
        std::path::PathBuf::from(r"D:\MSFS 2020\Packages").join("Community")
    );
    assert!(community_from_usercfg("SomeSetting 1").is_none());
    assert!(community_from_usercfg("InstalledPackagesPath \"\"").is_none());
}

#[test]
fn only_complete_packages_are_detected() {
    let temp = tempfile::tempdir().unwrap();
    let junk = temp.path().join("not-a-package");
    fs::create_dir(&junk).unwrap();
    fs::write(junk.join("readme.txt"), "junk").unwrap();
    let pack = temp.path().join("extra").join("author-airport-patk");
    fs::create_dir_all(&pack).unwrap();
    fs::write(pack.join("manifest.json"), "{}").unwrap();
    fs::write(pack.join("layout.json"), "{\"content\":[]}").unwrap();
    let packages = discover_packages(temp.path()).unwrap();
    assert_eq!(packages, vec![pack.clone()]);
    fs::write(pack.join("layout.json"), "broken").unwrap();
    assert!(discover_packages(temp.path()).is_err());
}

#[test]
fn duplicated_names_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    for name in ["one", "two"] {
        let path = temp.path().join(name).join("same-name");
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("manifest.json"), "{}").unwrap();
        fs::write(path.join("layout.json"), "{}").unwrap();
    }
    assert!(discover_packages(temp.path()).is_err());
}
