use xorfilter::Xor8;

use std::{
    hash::{BuildHasher, Hash},
    result,
};

use crate::{
    cbor::{Cbor, FromCbor, IntoCbor},
    db::Bloom,
    Error, LocalCborize, Result,
};

// Intermediate type to serialize and de-serialized Xor8 into bytes using
// `mkit` macros.
#[derive(LocalCborize)]
struct CborXor8 {
    hash_builder: Vec<u8>,
    seed: u64,
    block_length: u32,
    finger_prints: Vec<u8>,
}

impl CborXor8 {
    const ID: &'static str = "xor8/0.0.1";
}

impl<H> IntoCbor for Xor8<H>
where
    H: BuildHasher + Into<Vec<u8>>,
{
    fn into_cbor(self) -> Result<Cbor> {
        let val = CborXor8 {
            hash_builder: self.hash_builder.into(),
            seed: self.seed,
            block_length: self.block_length,
            finger_prints: self.finger_prints,
        };
        val.into_cbor()
    }
}

impl<H> FromCbor for Xor8<H>
where
    H: Default + BuildHasher + From<Vec<u8>>,
{
    fn from_cbor(val: Cbor) -> Result<Self> {
        let val = CborXor8::from_cbor(val)?;

        let mut filter = Xor8::<H>::default();
        #[allow(clippy::field_reassign_with_default)]
        {
            filter.hash_builder = val.hash_builder.into();
            filter.seed = val.seed;
            filter.block_length = val.block_length;
            filter.finger_prints = val.finger_prints;
        }
        Ok(filter)
    }
}

impl<H> Bloom for Xor8<H>
where
    H: Default + BuildHasher + From<Vec<u8>> + Into<Vec<u8>> + Clone,
{
    type Err = Error;

    fn add_key<Q: ?Sized + Hash>(&mut self, key: &Q) {
        self.insert(key)
    }

    fn add_digest32(&mut self, digest: u32) {
        self.populate_keys(&[u64::from(digest)])
    }

    fn build(&mut self) -> Result<()> {
        err_at!(Fatal, self.build())
    }

    fn contains<Q: ?Sized + Hash>(&self, element: &Q) -> bool {
        self.contains(element)
    }

    fn to_bytes(&self) -> result::Result<Vec<u8>, Self::Err> {
        let val = CborXor8 {
            hash_builder: self.hash_builder.clone().into(),
            seed: self.seed,
            block_length: self.block_length,
            finger_prints: self.finger_prints.clone(),
        };
        let cbor_val = err_at!(IOError, val.into_cbor())?;

        let mut buf: Vec<u8> = vec![];
        err_at!(IOError, cbor_val.encode(&mut buf))?;
        Ok(buf)
    }

    fn from_bytes(mut buf: &[u8]) -> result::Result<(Self, usize), Self::Err> {
        let (val, n) = err_at!(IOError, Cbor::decode(&mut buf))?;
        Ok((err_at!(IOError, Xor8::<H>::from_cbor(val))?, n))
    }

    fn or(&self, _other: &Self) -> result::Result<Self, Self::Err> {
        unimplemented!()
    }
}

#[cfg(test)]
#[path = "xorfilter_test.rs"]
mod xorfilter_test;
