use crate::Result;

pub trait Cborize: Sized {
    fn to_cbor_data(&self) -> Vec<u8>;

    fn from_cbor_data(data: &[u8]) -> Result<(Self, &[u8])>;
}
