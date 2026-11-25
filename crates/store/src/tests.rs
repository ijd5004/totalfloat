use std::path::PathBuf;

use super::*;

fn new_store() -> (tempfile::TempDir, PathBuf, Store) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("plan.tfdb");
    let store = Store::create(&path).unwrap();
    (directory, path, store)
}

fn change(parent: Option<&ChangesetId>, message: &str, file: &[u8]) -> NewChangeset {
    NewChangeset {
        parent: parent.cloned(),
        second_parent: None,
        kind: if parent.is_some() {
            Kind::Edit
        } else {
            Kind::Import
        },
        state: State::Accepted,
        author: "Pat".into(),
        author_kind: AuthorKind::Human,
        message: message.into(),
        created: "2026-11-02T09:00:00Z".into(),
        tool_version: "0.0.0".into(),
        commands: "{}".into(),
        skipped: "[]".into(),
        report: "{}".into(),
        snapshot: file.to_vec(),
    }
}

fn on_main(expected: Option<&ChangesetId>) -> Moving<'_> {
    Moving {
        name: "main",
        kind: RefKind::Branch,
        expected,
    }
}

/// Three changesets on `main`, oldest first.
fn three(store: &mut Store) -> Vec<Changeset> {
    let first = store
        .record(&change(None, "start", b"one"), Some(on_main(None)))
        .unwrap();
    let second = store
        .record(
            &change(Some(&first.id), "two", b"two"),
            Some(on_main(Some(&first.id))),
        )
        .unwrap();
    let third = store
        .record(
            &change(Some(&second.id), "three", b"three"),
            Some(on_main(Some(&second.id))),
        )
        .unwrap();
    vec![first, second, third]
}

#[test]
fn a_store_is_made_once_and_opens_again() {
    let (_directory, path, store) = new_store();
    assert!(looks_like_store(&path));
    drop(store);
    assert!(matches!(Store::create(&path), Err(StoreError::Exists(_))));
    let store = Store::open(&path).unwrap();
    assert_eq!(store.current().unwrap(), None);
}

#[test]
fn other_files_and_formats_are_refused() {
    let directory = tempfile::tempdir().unwrap();
    let text = directory.path().join("plan.xer");
    std::fs::write(&text, "ERMHDR\t23.12\n").unwrap();
    assert!(!looks_like_store(&text));
    assert!(matches!(Store::open(&text), Err(StoreError::NotAStore(_))));
    let other = directory.path().join("other.db");
    Connection::open(&other)
        .unwrap()
        .execute_batch("CREATE TABLE things (x INTEGER);")
        .unwrap();
    assert!(matches!(Store::open(&other), Err(StoreError::NotAStore(_))));
    let (_keep, path, store) = new_store();
    store
        .connection
        .execute(
            "UPDATE meta SET value = 'totalfloat.store/9' WHERE key = 'format'",
            [],
        )
        .unwrap();
    drop(store);
    assert!(
        matches!(Store::open(&path), Err(StoreError::UnknownFormat(f)) if f == "totalfloat.store/9")
    );
}

#[test]
fn the_same_file_is_stored_once() {
    let (_directory, _path, store) = new_store();
    let a = store.put_blob(b"ERMHDR same").unwrap();
    let b = store.put_blob(b"ERMHDR same").unwrap();
    assert_eq!(a, b);
    let rows: i64 = store
        .connection
        .query_row("SELECT COUNT(*) FROM blobs", [], |row| row.get(0))
        .unwrap();
    assert_eq!(rows, 1);
    assert_eq!(store.blob(&a).unwrap(), b"ERMHDR same");
}

#[test]
fn history_reads_back_newest_first() {
    let (_directory, _path, mut store) = new_store();
    let made = three(&mut store);
    let head = store.resolve("main").unwrap();
    assert_eq!(head, made[2].id);
    let log = store.log(&head, 10).unwrap();
    let messages: Vec<&str> = log.iter().map(|c| c.message.as_str()).collect();
    assert_eq!(messages, ["three", "two", "start"]);
    assert_eq!(log[2].seq, 1);
    assert_eq!(store.log(&head, 2).unwrap().len(), 2);
    assert_eq!(store.blob(&log[1].snapshot).unwrap(), b"two");
    assert_eq!(store.changeset(&made[1].id).unwrap(), made[1]);
}

#[test]
fn a_reader_finds_what_was_recorded_since_it_looked() {
    let (_directory, path, mut store) = new_store();
    assert_eq!(store.latest_seq().unwrap(), 0);
    assert_eq!(store.since(0).unwrap(), []);
    let made = three(&mut store);
    // Another handle on the same file, as another process would have.
    let other = Store::open(&path).unwrap();
    assert_eq!(other.latest_seq().unwrap(), 3);
    let later: Vec<String> = other
        .since(1)
        .unwrap()
        .into_iter()
        .map(|c| c.message)
        .collect();
    assert_eq!(later, ["two", "three"]);
    assert_eq!(other.since(3).unwrap(), []);
    store
        .record(
            &change(Some(&made[2].id), "four", b"four"),
            Some(on_main(Some(&made[2].id))),
        )
        .unwrap();
    assert_eq!(other.latest_seq().unwrap(), 4);
    assert_eq!(other.since(3).unwrap()[0].message, "four");
}

#[test]
fn ids_resolve_from_a_prefix() {
    let (_directory, _path, mut store) = new_store();
    let made = three(&mut store);
    let id = &made[1].id;
    assert_eq!(store.resolve(&id.as_str()[..6]).unwrap(), *id);
    assert_eq!(
        store.resolve(&id.as_str()[..12].to_uppercase()).unwrap(),
        *id
    );
    assert!(matches!(
        store.resolve(&id.as_str()[..5]),
        Err(StoreError::TooShort(_))
    ));
    assert!(matches!(
        store.resolve("nowhere"),
        Err(StoreError::NotFound(_))
    ));
    assert!(matches!(
        store.resolve("ffffff00"),
        Err(StoreError::NotFound(_))
    ));
    // Two IDs sharing a prefix, put straight in, since hashes won't make one.
    for last in ['1', '2'] {
        let crafted = format!("abcdef{}{last}", "0".repeat(57));
        store
            .connection
            .execute(
                "INSERT INTO changesets SELECT ?1, seq + 100 + ?2, parent, second_parent, kind,
                 state, author, author_kind, message, created, tool_version, commands, skipped,
                 report, snapshot FROM changesets WHERE seq = 1",
                params![crafted, i64::from(u8::try_from(last).unwrap())],
            )
            .unwrap();
    }
    assert!(matches!(
        store.resolve("abcdef"),
        Err(StoreError::Ambiguous(_, 2))
    ));
}

#[test]
fn a_ref_that_moved_refuses_the_write() {
    let (_directory, _path, mut store) = new_store();
    let made = three(&mut store);
    let stale = change(Some(&made[1].id), "late", b"late");
    let refused = store.record(&stale, Some(on_main(Some(&made[1].id))));
    assert!(
        matches!(refused, Err(StoreError::Moved { .. })),
        "{refused:?}"
    );
    assert_eq!(store.resolve("main").unwrap(), made[2].id);
    // Nothing of the refused changeset was kept.
    assert_eq!(store.all().unwrap().len(), 3);
    let again = store.record(&change(None, "start", b"one"), Some(on_main(None)));
    assert!(matches!(again, Err(StoreError::Moved { .. })));
}

#[test]
fn a_second_writer_is_told_the_branch_moved() {
    let (_directory, path, mut first) = new_store();
    let start = first
        .record(&change(None, "start", b"one"), Some(on_main(None)))
        .unwrap();
    let mut second = Store::open(&path).unwrap();
    // Both read main at `start`; the first to write wins.
    first
        .record(
            &change(Some(&start.id), "mine", b"mine"),
            Some(on_main(Some(&start.id))),
        )
        .unwrap();
    let theirs = second.record(
        &change(Some(&start.id), "theirs", b"theirs"),
        Some(on_main(Some(&start.id))),
    );
    assert!(
        matches!(theirs, Err(StoreError::Moved { .. })),
        "{theirs:?}"
    );
}

#[test]
fn the_same_contents_are_recorded_once() {
    let (_directory, _path, mut store) = new_store();
    let made = three(&mut store);
    let twin = change(Some(&made[1].id), "three", b"three");
    let refused = store.record(&twin, None);
    assert!(
        matches!(refused, Err(StoreError::Duplicate(_))),
        "{refused:?}"
    );
}

#[test]
fn verify_reads_past_a_row_it_can_t_read() {
    let (_directory, _path, mut store) = new_store();
    let made = three(&mut store);
    store.set_current("main").unwrap();
    store
        .connection
        .execute(
            "UPDATE changesets SET kind = 'edti' WHERE id = ?1",
            [made[1].id.as_str()],
        )
        .unwrap();
    store
        .connection
        .execute("UPDATE meta SET value = 'gone' WHERE key = 'current'", [])
        .unwrap();
    let damage = store.verify().unwrap();
    let subjects: Vec<&str> = damage.iter().map(|d| d.subject.as_str()).collect();
    assert!(subjects.contains(&made[1].id.as_str()), "{damage:?}");
    assert!(subjects.contains(&"gone"), "{damage:?}");
}

#[test]
fn a_blob_row_with_a_wrong_size_is_damage_not_a_crash() {
    let (_directory, _path, store) = new_store();
    let hash = store.put_blob(b"ERMHDR small").unwrap();
    store
        .connection
        .execute(
            "UPDATE blobs SET size = 1000000000000000 WHERE hash = ?1",
            [hash.as_str()],
        )
        .unwrap();
    assert!(matches!(store.blob(&hash), Err(StoreError::Damaged(_))));
}

#[test]
fn a_parent_must_be_recorded() {
    let (_directory, _path, mut store) = new_store();
    let made = three(&mut store);
    let mut orphan = change(Some(&made[0].id), "x", b"x");
    orphan.parent = Some(ChangesetId::from_stored("0".repeat(64)).unwrap());
    assert!(matches!(
        store.record(&orphan, None),
        Err(StoreError::NotFound(_))
    ));
}

#[test]
fn the_merge_base_is_the_latest_shared_changeset() {
    let (_directory, _path, mut store) = new_store();
    let made = three(&mut store);
    let fork = Moving {
        name: "what-if",
        kind: RefKind::Branch,
        expected: None,
    };
    store.set_ref(fork, &made[1].id).unwrap();
    let mut side = change(Some(&made[1].id), "steel late", b"side");
    side.created = "2026-11-03T09:00:00Z".into();
    let expected = Moving {
        expected: Some(&made[1].id),
        ..fork
    };
    let side = store.record(&side, Some(expected)).unwrap();
    assert_eq!(
        store.merge_base(&made[2].id, &side.id).unwrap(),
        Some(made[1].id.clone())
    );
    assert!(store.is_ancestor(&made[0].id, &side.id).unwrap());
    assert!(!store.is_ancestor(&made[2].id, &side.id).unwrap());
    let refs: Vec<String> = store.refs().unwrap().into_iter().map(|r| r.name).collect();
    assert_eq!(refs, ["main", "what-if"]);
    store.delete_ref("what-if").unwrap();
    assert!(store.get_ref("what-if").unwrap().is_none());
    assert_eq!(store.changeset(&side.id).unwrap().message, "steel late");
}

#[test]
fn state_changes_keep_the_id() {
    let (_directory, _path, mut store) = new_store();
    let made = three(&mut store);
    store.set_state(&made[2].id, State::Proposed).unwrap();
    assert_eq!(store.changeset(&made[2].id).unwrap().state, State::Proposed);
    assert_eq!(store.verify().unwrap(), Vec::<Damage>::new());
}

#[test]
fn the_current_branch_must_be_a_branch() {
    let (_directory, _path, mut store) = new_store();
    let made = three(&mut store);
    store.set_current("main").unwrap();
    assert_eq!(store.current().unwrap().as_deref(), Some("main"));
    let base = Moving {
        name: "contract",
        kind: RefKind::Baseline,
        expected: None,
    };
    store.set_ref(base, &made[0].id).unwrap();
    assert!(matches!(
        store.set_current("contract"),
        Err(StoreError::WrongKind { .. })
    ));
    assert!(matches!(
        store.set_current("nowhere"),
        Err(StoreError::NotFound(_))
    ));
    // A baseline never moves, and the checked-out branch can't go.
    let again = Moving {
        expected: Some(&made[0].id),
        ..base
    };
    assert!(matches!(
        store.set_ref(again, &made[2].id),
        Err(StoreError::WrongKind { .. })
    ));
    let as_branch = Moving {
        name: "contract",
        kind: RefKind::Branch,
        expected: Some(&made[0].id),
    };
    let moved = store.record(&change(Some(&made[0].id), "x", b"x"), Some(as_branch));
    assert!(
        matches!(moved, Err(StoreError::WrongKind { .. })),
        "{moved:?}"
    );
    assert!(matches!(
        store.delete_ref("main"),
        Err(StoreError::Current(_))
    ));
    store.delete_ref("contract").unwrap();
}

#[test]
fn verify_finds_damage() {
    let (_directory, _path, mut store) = new_store();
    let made = three(&mut store);
    assert_eq!(store.verify().unwrap(), Vec::<Damage>::new());
    store
        .connection
        .execute(
            "UPDATE blobs SET data = ?1 WHERE hash = ?2",
            params![b"not deflate".to_vec(), made[1].snapshot.as_str()],
        )
        .unwrap();
    store
        .connection
        .execute(
            "UPDATE changesets SET message = 'rewritten' WHERE id = ?1",
            [made[2].id.as_str()],
        )
        .unwrap();
    let damage = store.verify().unwrap();
    let subjects: Vec<&str> = damage.iter().map(|d| d.subject.as_str()).collect();
    assert!(subjects.contains(&made[1].snapshot.as_str()), "{damage:?}");
    assert!(subjects.contains(&made[2].id.as_str()), "{damage:?}");
    assert!(matches!(
        store.blob(&made[1].snapshot),
        Err(StoreError::Damaged(_))
    ));
}

#[test]
fn a_schedule_file_compresses() {
    let file = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/xer/batches/001/loe.xer"
    ))
    .unwrap();
    let (_directory, _path, store) = new_store();
    let hash = store.put_blob(&file).unwrap();
    let stored: i64 = store
        .connection
        .query_row(
            "SELECT length(data) FROM blobs WHERE hash = ?1",
            [hash.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    eprintln!("{} bytes stored as {stored}", file.len());
    assert!(usize::try_from(stored).unwrap() < file.len());
    assert_eq!(store.blob(&hash).unwrap(), file);
}

#[test]
fn a_review_is_kept_and_moves_refs_with_it() {
    let (_directory, _path, mut store) = new_store();
    let made = three(&mut store);
    let mut proposal = change(Some(&made[2].id), "agent idea", b"idea");
    proposal.state = State::Proposed;
    proposal.author_kind = AuthorKind::Agent;
    let ref_name = "proposal/x";
    let proposed = store
        .record(
            &proposal,
            Some(Moving {
                name: ref_name,
                kind: RefKind::Proposal,
                expected: None,
            }),
        )
        .unwrap();
    let review = Review {
        changeset: proposed.id.clone(),
        state: State::Accepted,
        author: "Pat".into(),
        author_kind: AuthorKind::Human,
        message: "fine".into(),
        created: "2026-11-03T09:00:00Z".into(),
    };
    let stale = Reviewing {
        review: review.clone(),
        drop_ref: Some(ref_name),
        advance: Some(on_main(Some(&made[1].id))),
    };
    assert!(matches!(
        store.review(&stale),
        Err(StoreError::Moved { .. })
    ));
    assert_eq!(
        store.changeset(&proposed.id).unwrap().state,
        State::Proposed
    );
    let reviewing = Reviewing {
        review,
        drop_ref: Some(ref_name),
        advance: Some(on_main(Some(&made[2].id))),
    };
    store.review(&reviewing).unwrap();
    assert_eq!(store.resolve("main").unwrap(), proposed.id);
    assert!(store.get_ref(ref_name).unwrap().is_none());
    let reviews = store.reviews(&proposed.id).unwrap();
    assert_eq!(reviews.len(), 1);
    assert_eq!(reviews[0].message, "fine");
    assert_eq!(
        store.changeset(&proposed.id).unwrap().state,
        State::Accepted
    );
}
