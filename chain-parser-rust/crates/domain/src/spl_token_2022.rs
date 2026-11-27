/// Token-2022 transfer fees and account extensions supported by the quote model.
use crate::{err, Result};

pub const PROGRAM: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";
pub const PROGRAMDATA: &str = "DoU57AYuPFu2QU514RktNPG22QhApEjnKxnBcu4BHDTY";

const EXTENSION_HEADER: usize = 166;
const FEE_BASIS_POINTS: u128 = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransferFee {
    basis_points: u16,
    maximum_fee: u64,
}

impl TransferFee {
    pub fn from_mint(data: &[u8], epoch: u64) -> Result<Self> {
        if data.len() != 278
            || data[45] != 1
            || data[82..165].iter().any(|&byte| byte != 0)
            || data[165..170] != [1, 1, 0, 108, 0]
        {
            return Err(err("TOKEN2022_MINT_EXTENSIONS"));
        }
        let older_epoch = u64::from_le_bytes(data[242..250].try_into().unwrap());
        let newer_epoch = u64::from_le_bytes(data[260..268].try_into().unwrap());
        let older = Self::from_schedule(&data[250..260]);
        let newer = Self::from_schedule(&data[268..278]);
        if older_epoch > newer_epoch
            || u128::from(older.basis_points) > FEE_BASIS_POINTS
            || u128::from(newer.basis_points) > FEE_BASIS_POINTS
        {
            return Err(err("TOKEN2022_FEE_SCHEDULE"));
        }
        Ok(if epoch >= newer_epoch { newer } else { older })
    }

    fn from_schedule(data: &[u8]) -> Self {
        Self {
            maximum_fee: u64::from_le_bytes(data[..8].try_into().unwrap()),
            basis_points: u16::from_le_bytes(data[8..].try_into().unwrap()),
        }
    }

    pub fn fee(self, gross: u64) -> u64 {
        (u128::from(gross) * u128::from(self.basis_points))
            .div_ceil(FEE_BASIS_POINTS)
            .min(u128::from(self.maximum_fee)) as u64
    }

    pub fn gross(self, net: u64) -> Result<u64> {
        if net == 0 {
            return Ok(0);
        }
        let basis_points = u128::from(self.basis_points);
        let cap = u128::from(self.maximum_fee);
        let fee = if basis_points == FEE_BASIS_POINTS {
            cap
        } else {
            (u128::from(net) * basis_points)
                .div_ceil(FEE_BASIS_POINTS - basis_points)
                .min(cap)
        };
        let gross =
            u64::try_from(u128::from(net) + fee).map_err(|_| err("TOKEN2022_FEE_OVERFLOW"))?;
        if gross - self.fee(gross) != net {
            return Err(err("TOKEN2022_FEE_INVERSE"));
        }
        Ok(gross)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransferFeeAccount {
    pub withheld: u64,
    pub immutable_owner: bool,
}

impl TransferFeeAccount {
    pub fn increment_withheld(data: &mut [u8], increment: u64) -> Result<()> {
        let withholding = Self::unpack(data)?;
        let amount = withholding
            .withheld
            .checked_add(increment)
            .ok_or_else(|| err("TOKEN2022_WITHHELD_OVERFLOW"))?;
        // There are only two supported TLVs. ImmutableOwner has no payload.
        let offset = if data[EXTENSION_HEADER] == 2 {
            EXTENSION_HEADER + 4
        } else {
            EXTENSION_HEADER + 8
        };
        data[offset..offset + 8].copy_from_slice(&amount.to_le_bytes());
        Ok(())
    }

    pub fn unpack(data: &[u8]) -> Result<Self> {
        let invalid = || err("TOKEN2022_ACCOUNT_EXTENSIONS");
        if !matches!(data.len(), 178 | 182) || data[165] != 2 {
            return Err(invalid());
        }
        let mut remaining = &data[EXTENSION_HEADER..];
        let mut withheld = None;
        let mut immutable_owner = false;
        while !remaining.is_empty() {
            let (header, payload) = remaining.split_at_checked(4).ok_or_else(invalid)?;
            let kind = u16::from_le_bytes([header[0], header[1]]);
            let size = usize::from(u16::from_le_bytes([header[2], header[3]]));
            let (value, rest) = payload.split_at_checked(size).ok_or_else(invalid)?;
            match (kind, size) {
                (2, 8) if withheld.is_none() => {
                    withheld = Some(u64::from_le_bytes(value.try_into().unwrap()));
                }
                (7, 0) if !immutable_owner => immutable_owner = true,
                _ => return Err(invalid()),
            }
            remaining = rest;
        }
        Ok(Self {
            withheld: withheld.ok_or_else(invalid)?,
            immutable_owner,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_fee_rounding_cap_epoch_and_inverse_boundaries() {
        let mut mint = [0; 278];
        mint[45] = 1;
        mint[165..170].copy_from_slice(&[1, 1, 0, 108, 0]);
        mint[250..258].copy_from_slice(&2u64.to_le_bytes());
        mint[258..260].copy_from_slice(&250u16.to_le_bytes());
        mint[260..268].copy_from_slice(&2u64.to_le_bytes());
        mint[268..276].copy_from_slice(&5u64.to_le_bytes());
        mint[276..278].copy_from_slice(&10_000u16.to_le_bytes());
        let older = TransferFee::from_mint(&mint, 1).unwrap();
        assert_eq!(older.fee(1), 1);
        assert_eq!(older.fee(1_000), 2);
        assert_eq!(older.gross(1), Ok(2));
        assert_eq!(older.gross(78), Ok(80));
        let newer = TransferFee::from_mint(&mint, 2).unwrap();
        assert_eq!(newer.fee(4), 4);
        assert_eq!(newer.fee(6), 5);
        assert_eq!(newer.gross(0), Ok(0));
        assert_eq!(newer.gross(1), Ok(6));
        assert_eq!(newer.gross(u64::MAX - 5), Ok(u64::MAX));
        assert!(newer.gross(u64::MAX - 4).is_err());
        mint[258..260].copy_from_slice(&10_001u16.to_le_bytes());
        assert!(TransferFee::from_mint(&mint, 2).is_err());
    }

    #[test]
    fn fee_account_accepts_both_tlv_orders_and_rejects_unknown_or_repeated_extensions() {
        let mut account = vec![0; EXTENSION_HEADER];
        account[165] = 2;
        let fee = [2, 0, 8, 0, 9, 0, 0, 0, 0, 0, 0, 0];
        let immutable = [7, 0, 0, 0];
        let pack = |extensions: &[&[u8]]| [account.as_slice(), &extensions.concat()].concat();
        for order in [[fee.as_slice(), &immutable], [&immutable, &fee]] {
            assert_eq!(
                TransferFeeAccount::unpack(&pack(&order)),
                Ok(TransferFeeAccount {
                    withheld: 9,
                    immutable_owner: true,
                })
            );
        }
        assert!(TransferFeeAccount::unpack(&pack(&[&fee, &[8, 0, 0, 0]])).is_err());
        assert!(TransferFeeAccount::unpack(&pack(&[&fee, &[2, 0, 0, 0]])).is_err());
        assert!(TransferFeeAccount::unpack(&pack(&[&fee, &immutable[..3]])).is_err());
    }
}
