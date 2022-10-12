use super::*;
use rand::{rngs::StdRng, seq::SliceRandom, thread_rng, Rng, SeedableRng};
use std::vec::Vec;
extern crate rand;

const TCNT: usize = 0x1000;

fn test_eq<T>(a: T, b: T, seed: [u8; 32])
where
    T: Eq + core::fmt::Debug,
{
    if a != b {
        eprintln!("rhs: {:?} lhs: {:?} seed: {:?}", a, b, seed);
    }
}

#[test]
fn test_simple_insert() {
    let p1 = 1;
    let p2 = 2;
    let mut array: XArray<u64> = XArray::new();
    assert!(array.insert(1, &p1).is_none());
    assert!(array.insert(2, &p2).is_none());
    assert_eq!(array.get(1), Some(&p1));
    assert_eq!(array.get(2), Some(&p2));
}

#[test]
fn test_simple_remove() {
    let p1 = 1;
    let mut array: XArray<u64> = XArray::new();
    for i in 0..100000 {
        assert!(array.insert(i, &p1).is_none());
    }
    for i in 0..100000 {
        assert_eq!(array.remove(i), Some(&p1));
        assert_eq!(array.get(i), None);
    }
}

#[test]
fn test_simple_mark() {
    let p = 1;
    let mut array: XArray<u64> = XArray::new();
    let mut cursor = array.cursor_mut(0);
    assert_eq!(cursor.insert(&p), None);
    cursor.mark(XaMark::Mark0);
    assert!(array.is_marked(XaMark::Mark0));

    let mut cursor = array.cursor_mut(1);
    assert_eq!(cursor.insert(&p), None);
    cursor.mark(XaMark::Mark1);
    assert!(array.is_marked(XaMark::Mark0));
    assert!(array.is_marked(XaMark::Mark1));

    let mut cursor = array.cursor_mut(2);
    assert_eq!(cursor.insert(&p), None);
    cursor.mark(XaMark::Mark1);
    assert!(array.is_marked(XaMark::Mark0));
    assert!(array.is_marked(XaMark::Mark1));

    let mut cursor = array.cursor_mut(1);
    cursor.unmark(XaMark::Mark1);
    assert!(array.is_marked(XaMark::Mark0));
    assert!(array.is_marked(XaMark::Mark1));

    let mut cursor = array.cursor_mut(2);
    cursor.unmark(XaMark::Mark1);
    assert!(array.is_marked(XaMark::Mark0));
    assert!(!array.is_marked(XaMark::Mark1));
}

#[test]
fn test_range() {
    use std::vec::Vec;

    let indice = (0..u64::MAX)
        .step_by(u64::MAX as usize / TCNT)
        .collect::<Vec<_>>();
    {
        let mut array: XArray<u64> = XArray::new();
        let mut inserted = Vec::new();
        assert_eq!(array.is_empty(), true);

        for (idx, i) in indice.iter().enumerate().take(TCNT) {
            assert_eq!(array.insert(*i, &indice[idx]), None);
            assert!(array.insert(*i, &indice[idx]).is_some());
            inserted.push((idx, *i));
        }

        for (idx, v) in array.extract(0, indice[TCNT / 2]).take(3) {
            assert_eq!(array.get(idx), Some(v));
        }
        for (idx, v) in array.extract(indice[TCNT / 2] + 1, TCNT as u64) {
            assert_eq!(array.get(idx), Some(v));
        }
    }
}

#[test]
fn test_sparse_insert() {
    use std::vec::Vec;

    let indice = (0..u64::MAX)
        .step_by(u64::MAX as usize / TCNT)
        .collect::<Vec<_>>();
    {
        let mut array: XArray<u64> = XArray::new();
        let mut inserted = Vec::new();
        assert_eq!(array.is_empty(), true);
        for (idx, i) in indice.iter().enumerate().take(TCNT) {
            assert_eq!(array.insert(*i, &indice[idx]), None);
            assert!(array.insert(*i, &indice[idx]).is_some());
            inserted.push((idx, *i));
            for (_idx, _i) in &inserted {
                assert_eq!(array.get(*_i), Some(&indice[*_idx]));
            }
        }

        while let Some((idx, i)) = inserted.pop() {
            assert_eq!(array.remove(i), Some(&indice[idx]));
            assert_eq!(array.get(i), None);
            for (_idx, _i) in &inserted {
                assert_eq!(array.get(*_i), Some(&indice[*_idx]));
            }
        }
    }
}

#[test]
fn test_dense_insert() {
    let indice = (0..TCNT as u64).collect::<Vec<_>>();

    {
        let mut array: XArray<u64> = XArray::new();
        assert_eq!(array.is_empty(), true);

        let mut inserted = Vec::new();
        for (idx, i) in indice.iter().enumerate() {
            assert_eq!(array.insert(*i, &indice[idx]), None);
            assert!(array.insert(*i, &indice[idx]).is_some());
            inserted.push((idx, *i));
            for (_idx, _i) in &inserted {
                assert_eq!(array.get(*_i), Some(&indice[*_idx]));
            }
        }

        // Test remove.
        while let Some((idx, i)) = inserted.pop() {
            assert_eq!(array.remove(i), Some(&indice[idx]));
            assert_eq!(array.get(i), None);
            for (_idx, _i) in &inserted {
                assert_eq!(array.get(*_i), Some(&indice[*_idx]));
            }
        }
    }
}

fn test_random_insert() {
    let mut seed_gen = thread_rng();
    let mut seed = [0; 32];
    (0..32).for_each(|i| seed[i] = seed_gen.gen::<u8>());
    let mut rng = StdRng::from_seed(seed);

    let mut inserted = Vec::new();
    let mut arv = [0; TCNT];

    for i in 0..TCNT as u64 {
        arv[i as usize] = rng.gen::<u64>();
    }
    {
        let mut array: XArray<u64> = XArray::new();

        assert_eq!(array.is_empty(), true);

        for i in 0..TCNT as u64 {
            if rng.gen::<u8>() % 2 == 0 {
                // insert
                test_eq(array.insert(i, &arv[i as usize]), None, seed);
                test_eq(array.insert(i, &arv[i as usize]).is_some(), true, seed);
                inserted.push(i);
            } else {
                inserted.shuffle(&mut rng);
                if let Some(i) = inserted.pop() {
                    test_eq(array.remove(i), Some(&arv[i as usize]), seed);
                }
            }

            // Validate
            for _i in &inserted {
                test_eq(array.get(*_i), Some(&arv[*_i as usize]), seed);
            }
        }

        while let Some(i) = inserted.pop() {
            test_eq(array.remove(i), Some(&arv[i as usize]), seed);
            for _i in &inserted {
                test_eq(array.get(*_i), Some(&arv[*_i as usize]), seed);
            }
        }
    }
}

#[test]
fn test_random_insert_multiple() {
    for _ in 0..400 {
        test_random_insert()
    }
}

#[test]
fn test_mark() {
    use std::vec::Vec;
    let indice = (0..u64::MAX)
        .step_by(u64::MAX as usize / TCNT)
        .collect::<Vec<_>>();
    let mut array: XArray<u64> = XArray::new();
    let mut inserted = Vec::new();
    let mut marked = std::collections::BTreeSet::new();
    {
        assert_eq!(array.is_empty(), true);

        for (idx, i) in indice.iter().enumerate().take(TCNT) {
            let idx = idx as u64;
            let mut cursor = array.cursor_mut(idx);

            assert_eq!(cursor.insert(i), None);
            if idx & 1 == 0 {
                cursor.mark(XaMark::Mark0);
                marked.insert(idx);
            }
            inserted.push((idx, *i));
        }
    }
    assert!(array.is_marked(XaMark::Mark0));
    for (i, _) in array.iter().filter_mark(XaMark::Mark0) {
        assert!(marked.remove(&i));
    }
    assert!(marked.is_empty());
}

#[test]
fn test_mark2() {
    use std::vec::Vec;
    let indice = (0..u64::MAX)
        .step_by(u64::MAX as usize / TCNT)
        .collect::<Vec<_>>();
    let mut array: XArray<u64> = XArray::new();
    let mut inserted = Vec::new();
    let mut marked = std::collections::BTreeSet::new();
    {
        assert_eq!(array.is_empty(), true);

        for (idx, i) in indice.iter().enumerate().take(TCNT) {
            let idx = idx as u64;
            let mut cursor = array.cursor_mut(idx);

            assert_eq!(cursor.insert(i), None);
            if idx & 1 == 0 {
                cursor.mark(XaMark::Mark0);
                marked.insert(idx);
            }
            inserted.push((idx, *i));
        }
    }
    let mut iter = array.iter_mut().filter_mark(XaMark::Mark0);
    while let Some((i, _)) = iter.next() {
        iter.as_cursor_mut().unmark(XaMark::Mark0);
        assert!(marked.remove(&i));
    }
    assert!(marked.is_empty());
    assert!(!array.is_marked(XaMark::Mark0));
}

#[test]
fn test_mark3() {
    use std::vec::Vec;
    let indice = (0..u64::MAX)
        .step_by(u64::MAX as usize / TCNT)
        .collect::<Vec<_>>();
    let mut array: XArrayBoxed<u64> = XArrayBoxed::new();
    let mut inserted = Vec::new();
    let mut marked = std::collections::BTreeSet::new();
    {
        assert_eq!(array.is_empty(), true);

        for (idx, i) in indice.iter().enumerate().take(TCNT) {
            let idx = idx as u64;
            let mut cursor = array.cursor_mut(idx);

            assert_eq!(cursor.insert(*i), None);
            if idx & 1 == 0 {
                cursor.mark(XaMark::Mark0);
                marked.insert(idx);
            }
            inserted.push((idx, *i));
        }
    }
    let mut iter = array.iter_mut().filter_mark(XaMark::Mark0);
    while let Some((i, _)) = iter.next() {
        iter.as_cursor_mut().unmark(XaMark::Mark0);
        assert!(marked.remove(&i));
    }
    assert!(marked.is_empty());
    assert!(!array.is_marked(XaMark::Mark0));
}

#[test]
fn test_next_allocated() {
    use std::vec::Vec;

    let indice = (0..u64::MAX)
        .step_by(u64::MAX as usize / TCNT)
        .take(TCNT)
        .collect::<Vec<_>>();
    {
        let mut array: XArray<u64> = XArray::new();
        assert_eq!(array.is_empty(), true);

        for (idx, i) in indice.iter().enumerate() {
            assert_eq!(array.insert(*i, &indice[idx]), None);
            assert!(array.insert(*i, &indice[idx]).is_some());
            println!("{}", i);
        }

        let mut cursor = array.cursor(indice[0]);

        for i in indice.iter() {
            assert_eq!(cursor.key(), *i);
            assert_eq!(cursor.current(), Some(i));
            cursor.next_allocated();
        }
    }
}
