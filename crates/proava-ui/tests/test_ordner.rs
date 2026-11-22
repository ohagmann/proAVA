//! Testordner (wackelnder Test 18.11.2026): Zwei Zufallstests fanden ihren
//! frisch angelegten Temp-Ordner nicht mehr, während ein zweiter Testlauf
//! lief. Jeder Test bekommt einen eigenen Ordner – auch wenn die Tests eines
//! Programms nebeneinander laufen oder mehrere Testläufe gleichzeitig –, und
//! beim Aufräumen geht nur dieser.

mod common;

use common::TempDir;

#[test]
fn every_test_folder_is_its_own_and_only_it_goes() {
    // A folder with the same label (another test, another run) stays.
    let neighbour = TempDir::new("eigen");
    let threads: Vec<_> = (0..8)
        .map(|_| std::thread::spawn(|| (0..50).map(|_| TempDir::new("eigen")).collect::<Vec<_>>()))
        .collect();
    let folders: Vec<TempDir> = threads
        .into_iter()
        .flat_map(|thread| thread.join().unwrap())
        .collect();
    let mut paths: Vec<_> = folders.iter().map(|folder| folder.0.clone()).collect();
    assert!(paths.iter().all(|path| path.is_dir()));
    paths.sort();
    paths.dedup();
    assert_eq!(paths.len(), 400, "kein Ordner doppelt vergeben");
    assert!(!paths.contains(&neighbour.0));
    drop(folders);
    assert!(
        paths.iter().all(|path| !path.exists()),
        "die eigenen sind weg"
    );
    assert!(neighbour.0.is_dir(), "der des Nachbarn bleibt");
}
