use rand::{prelude::random, rngs::SmallRng, Rng, SeedableRng};

use super::*;

#[test]
fn test_basic7() {
    use xorfilter::BuildHasherDefault;

    let seed: u128 = random();
    println!("test_basic7 seed {}", seed);
    let mut rng = SmallRng::from_seed(seed.to_le_bytes());

    let keys: Vec<u64> = (0..100_000).map(|_| rng.gen::<u64>()).collect();

    let filter = {
        let mut filter = Xor8::<BuildHasherDefault>::new();
        filter.populate(&keys);
        filter.build();
        filter
    };

    for key in keys.iter() {
        assert!(filter.contains(key), "key {} not present", key);
    }

    let filter = {
        let bytes = <Xor8 as Bloom>::to_bytes(&filter).unwrap();
        <Xor8 as Bloom>::from_bytes(&bytes).unwrap().0
    };

    for key in keys.iter() {
        assert!(filter.contains(key), "key {} not present", key);
    }
}
