use super::*;

#[test]
fn test_entry_values() {
    let mut entry: Entry<u8, u64, u64> = Entry::new(10, 200, 1);
    entry.insert(300, 2);
    entry.insert(400, 3);
    entry.delete(4);
    entry.insert(500, 5);
    entry.delete(6);
    entry.delete(7);
    entry.insert(600, 8);

    let values = entry.to_values();
    let mut refvs = vec![
        Value::U {
            value: 200,
            seqno: 1,
        },
        Value::U {
            value: 300,
            seqno: 2,
        },
        Value::U {
            value: 400,
            seqno: 3,
        },
        Value::D { seqno: 4 },
        Value::U {
            value: 500,
            seqno: 5,
        },
        Value::D { seqno: 6 },
        Value::D { seqno: 7 },
        Value::U {
            value: 600,
            seqno: 8,
        },
    ];
    assert_eq!(values, refvs);

    entry.delete(9);
    let values = entry.to_values();
    refvs.push(Value::D { seqno: 9 });
    assert_eq!(values, refvs);
}

#[test]
fn test_entry_contains() {
    let mut one: Entry<u8, u64, u64> = Entry::new(10, 200, 1);
    one.insert(300, 3);
    one.insert(400, 5);
    one.delete(7);
    one.insert(500, 9);
    one.delete(11);
    one.delete(13);
    one.insert(600, 15);

    assert!(one.contains(&Entry::new(10, 200, 1)), "{:?}", one);
    assert!(one.contains(&Entry::new_deleted(10, 7)), "{:?}", one);
    assert!(!one.contains(&Entry::new(10, 200, 2)), "{:?}", one);

    let mut two: Entry<u8, u64, u64> = Entry::new(10, 200, 1);
    two.insert(300, 3);
    two.insert(400, 5);
    two.delete(7);
    two.insert(500, 9);
    two.delete(11);
    two.delete(13);
    assert!(one.contains(&two), "{:?} {:?}", one, two);
    two.insert(600, 15);
    assert!(one.contains(&two), "{:?} {:?}", one, two);
    two.insert(600, 16);
    assert!(!one.contains(&two), "{:?} {:?}", one, two);
}

#[test]
fn test_entry_merge() {
    let mut one: Entry<u8, u64, u64> = Entry::new(10, 200, 1);
    one.insert(300, 3);
    one.insert(400, 5);
    one.delete(7);
    one.insert(500, 9);
    one.delete(11);
    one.delete(13);
    one.insert(600, 15);

    let mut two: Entry<u8, u64, u64> = Entry::new(10, 1000, 2);
    two.insert(2000, 4);
    two.delete(6);
    two.insert(3000, 8);
    two.delete(10);
    two.insert(4000, 12);
    two.insert(5000, 14);
    two.delete(16);

    let mut entry: Entry<u8, u64, u64> = Entry::new(10, 200, 1);
    entry.insert(1000, 2);
    entry.insert(300, 3);
    entry.insert(2000, 4);
    entry.insert(400, 5);
    entry.delete(6);
    entry.delete(7);
    entry.insert(3000, 8);
    entry.insert(500, 9);
    entry.delete(10);
    entry.delete(11);
    entry.insert(4000, 12);
    entry.delete(13);
    entry.insert(5000, 14);
    entry.insert(600, 15);
    entry.delete(16);

    assert_eq!(one.merge(&two), entry);
}
