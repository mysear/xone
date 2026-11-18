#[path = "support/op_user.rs"]
mod support;
use revm::primitives::{AccessListItem, Address, Bytes, TxKind, B256, U256};
use support::*;
use xone_simulation::op_transaction::{authenticate_user, MAX_WIRE_BYTES};
fn accept(wire: &[u8]) -> xone_domain::Result<xone_simulation::op_transaction::AuthenticatedUser> {
    authenticate_user(10, hash(wire), payer(), wire, MAX_WIRE_BYTES)
}
#[test]
fn signed_envelopes_derive_every_vm_field_and_preserve_complete_wire() {
    for kind in 0..=2 {
        let mut f = fields(kind);
        let offset = usize::from(kind > 0);
        f[offset] = number(U256::from(42));
        let value = if kind == 2 {
            6
        } else if kind == 1 {
            5
        } else {
            4
        };
        f[value] = number(U256::from(123456));
        if kind > 0 {
            let slot = encoded(&[0x11; 32], false);
            let entry = encoded(
                &[encoded(&[0x33; 20], false), encoded(&slot, true)].concat(),
                true,
            );
            *f.last_mut().unwrap() = encoded(&entry, true);
        }
        let wire = signed(kind, &f);
        let t = accept(&wire).unwrap();
        let e = t.environment();
        assert_eq!(t.transaction_hash(), hash(&wire));
        assert_eq!(t.transaction_type(), kind);
        assert_eq!(e.caller, payer());
        assert_eq!(e.nonce, Some(42));
        assert_eq!(e.chain_id, Some(10));
        assert_eq!(e.gas_limit, 60000);
        assert_eq!(e.gas_price, U256::from(3_000_000_000u64));
        assert_eq!(e.gas_priority_fee, (kind == 2).then_some(U256::from(2)));
        assert_eq!(e.transact_to, TxKind::Call(Address::repeat_byte(0x22)));
        assert_eq!(e.value, U256::from(123456));
        assert_eq!(e.data, Bytes::from(vec![0, 1, 2]));
        assert_eq!(
            e.optimism.enveloped_tx.as_ref().map(|b| b.as_ref()),
            Some(wire.as_slice())
        );
        assert_eq!(
            e.access_list,
            if kind == 0 {
                vec![]
            } else {
                vec![AccessListItem {
                    address: Address::repeat_byte(0x33),
                    storage_keys: vec![B256::repeat_byte(0x11)],
                }]
            }
        );
        let to = if kind == 2 {
            5
        } else if kind == 1 {
            4
        } else {
            3
        };
        f[to] = vec![0x80];
        assert_eq!(
            accept(&signed(kind, &f)).unwrap().environment().transact_to,
            TxKind::Create
        );
    }
}
#[test]
fn signature_source_identity_budget_and_canonical_rlp_fail_closed() {
    for kind in 0..=2 {
        let f = fields(kind);
        let wire = signed(kind, &f);
        assert!(authenticate_user(1, hash(&wire), payer(), &wire, wire.len()).is_err());
        assert!(authenticate_user(8453, hash(&wire), payer(), &wire, wire.len()).is_err());
        assert!(authenticate_user(10, B256::ZERO, payer(), &wire, wire.len()).is_err());
        assert!(authenticate_user(10, hash(&wire), Address::ZERO, &wire, wire.len()).is_err());
        assert!(authenticate_user(10, hash(&wire), payer(), &wire, wire.len() - 1).is_err());
        for i in 0..wire.len() {
            assert!(accept(&wire[..i]).is_err(), "truncation {kind}/{i}");
        }
        for i in 1..wire.len() {
            let mut w = wire.clone();
            w[i] ^= 1;
            assert!(accept(&w).is_err(), "byte substitution {kind}/{i}");
        }
        let mut w = wire.clone();
        w.push(0);
        assert!(accept(&w).is_err());
        let nonce = usize::from(kind > 0);
        for bad in [
            vec![0],
            vec![0x81, 1],
            encoded(&[1; 33], false),
            number(U256::from(u64::MAX)),
        ] {
            let mut changed = f.clone();
            changed[nonce] = bad;
            assert!(accept(&signed(kind, &changed)).is_err());
        }
        let gas = if kind == 2 {
            4
        } else if kind == 1 {
            3
        } else {
            2
        };
        for g in [U256::from(20999), U256::from(u64::MAX) + U256::from(1)] {
            let mut changed = f.clone();
            changed[gas] = number(g);
            assert!(accept(&signed(kind, &changed)).is_err());
        }
        let to = gas + 1;
        for b in [vec![0; 19], vec![0; 21]] {
            let mut changed = f.clone();
            changed[to] = encoded(&b, false);
            assert!(accept(&signed(kind, &changed)).is_err());
        }
        if kind == 2 {
            let mut changed = f.clone();
            changed[2] = number(U256::from(4_000_000_000u64));
            assert!(accept(&signed(kind, &changed)).is_err());
        }
    }
    for prefix in [0, 3, 4, 0x7e, 0x80] {
        assert!(accept(&[prefix, 0xc0]).is_err());
    }
    assert!(accept(&[]).is_err());
    assert!(accept(&[1, 0xf8, 1, 0x80]).is_err());
    assert!(accept(&[1, 0xf9, 0, 56]).is_err());
    assert!(accept(&[1, 0xff, 255, 255, 255, 255, 255, 255, 255, 255]).is_err());
}
#[test]
fn signed_access_list_width_depth_and_aggregate_allocations_are_bounded() {
    let mut f = fields(2);
    for bad in [
        vec![0x80],
        encoded(&encoded(&[0; 19], false), true),
        encoded(
            &encoded(
                &[
                    encoded(&[0; 20], false),
                    encoded(&encoded(&[0; 31], false), true),
                ]
                .concat(),
                true,
            ),
            true,
        ),
        encoded(&vec![0xc0; 4097], true),
    ] {
        *f.last_mut().unwrap() = bad;
        assert!(accept(&signed(2, &f)).is_err());
    }
    let entry = encoded(
        &[
            encoded(&[0; 20], false),
            encoded(&vec![encoded(&[0; 32], false); 4096].concat(), true),
        ]
        .concat(),
        true,
    );
    *f.last_mut().unwrap() = encoded(&entry, true);
    assert!(accept(&signed(2, &f)).is_err());
    let entry = encoded(
        &[
            encoded(&[0; 20], false),
            encoded(&vec![encoded(&[0; 32], false); 4095].concat(), true),
        ]
        .concat(),
        true,
    );
    *f.last_mut().unwrap() = encoded(&entry, true);
    assert_eq!(
        accept(&signed(2, &f)).unwrap().environment().access_list[0]
            .storage_keys
            .len(),
        4095
    );
}
#[test]
fn signature_malleability_parity_scalars_and_field_counts_are_rejected() {
    for kind in 0..=2 {
        let f = fields(kind);
        let parts = signed_fields(kind, &f);
        let sig = parts.len() - 3;
        let wrap = |p: &[Vec<u8>]| {
            let mut w = encoded(&p.concat(), true);
            if kind > 0 {
                w.insert(0, kind);
            }
            w
        };
        for index in [sig + 1, sig + 2] {
            for n in [U256::ZERO, U256::MAX] {
                let mut p = parts.clone();
                p[index] = number(n);
                assert!(accept(&wrap(&p)).is_err());
            }
        }
        let mut p = parts.clone();
        p[sig + 2] = number(
            U256::from_str_radix(
                "7fffffffffffffffffffffffffffffff5d576e7357a4501ddfe92f46681b20a1",
                16,
            )
            .unwrap(),
        );
        assert!(accept(&wrap(&p)).is_err());
        for parity in [2, 27, 28, 54, 57] {
            let mut p = parts.clone();
            p[sig] = number(U256::from(parity as u64));
            assert!(accept(&wrap(&p)).is_err());
        }
        let mut p = parts.clone();
        p.push(vec![0x80]);
        assert!(accept(&wrap(&p)).is_err());
        let mut p = parts.clone();
        p.pop();
        assert!(accept(&wrap(&p)).is_err());
        if kind > 0 {
            let mut f = f.clone();
            f[0] = number(U256::from(8453));
            let wire = signed(kind, &f);
            assert!(authenticate_user(8453, hash(&wire), payer(), &wire, wire.len()).is_ok());
        }
    }
}
#[test]
#[ignore = "requires archived normal-source signed public users"]
fn archived_normal_sources_authenticate_all_original_public_signed_users() {
    let v: serde_json::Value = serde_json::from_slice(
        &std::fs::read(std::env::var("XONE_OP_PUBLIC_USERS").unwrap()).unwrap(),
    )
    .unwrap();
    let users = v["users"].as_array().unwrap();
    assert_eq!(users.len(), 180);
    for u in users {
        let wire = hex::decode(
            u["encoded_transaction"]
                .as_str()
                .unwrap()
                .trim_start_matches("0x"),
        )
        .unwrap();
        let expected = u["hash"].as_str().unwrap().parse().unwrap();
        let caller = u["sender"].as_str().unwrap().parse().unwrap();
        assert_eq!(u["chain_key"], "evm:10");
        let t = authenticate_user(10, expected, caller, &wire, wire.len())
            .unwrap_or_else(|e| panic!("{}: {e}", u["hash"]));
        assert_eq!(t.transaction_hash(), expected);
        assert_eq!(t.environment().caller, caller);
        assert_eq!(
            t.environment()
                .optimism
                .enveloped_tx
                .as_ref()
                .unwrap()
                .as_ref(),
            wire.as_slice()
        );
        assert!(authenticate_user(10, expected, Address::ZERO, &wire, wire.len()).is_err());
    }
}
