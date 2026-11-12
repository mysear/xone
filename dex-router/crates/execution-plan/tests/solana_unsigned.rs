use base64::{engine::general_purpose::STANDARD, Engine};
use xone_execution_plan::solana_unsigned::{
    compile_message, compile_message_many, unsigned_packet, SolanaAccountMeta as Meta,
    SolanaInstruction as Instruction,
};
const PAYER: &str = "9SNtBf6xU8NtKksWQcw4pHqhNguzuc5jvjg68gzTQmso";
fn ix(program: &str, writable: bool) -> Instruction {
    Instruction {
        program_id: program.into(),
        accounts: vec![Meta {
            pubkey: PAYER.into(),
            is_signer: true,
            is_writable: writable,
        }],
        data_base64: STANDARD.encode([1, 2]),
    }
}
#[test]
fn legacy_multi_instruction_merges_signer_permissions_and_preserves_order() {
    let a = ix("11111111111111111111111111111111", false);
    let b = ix("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA", true);
    let one = compile_message(PAYER, PAYER, &a).unwrap();
    assert_eq!(
        one,
        compile_message_many(PAYER, PAYER, std::slice::from_ref(&a)).unwrap()
    );
    let many = compile_message_many(PAYER, PAYER, &[a, b]).unwrap();
    assert_eq!(many[0], 1);
    assert_eq!(many[1], 0);
    assert_eq!(many[2], 2);
    assert_eq!(many[3], 3);
    let tx = unsigned_packet(&many).unwrap();
    assert_eq!(tx[0], 1);
    assert!(tx[1..65].iter().all(|v| *v == 0));
    assert_eq!(&tx[65..], many);
}
#[test]
fn wire_rejects_empty_multiple_signer_program_permission_escalation_and_oversize() {
    assert!(compile_message_many(PAYER, PAYER, &[]).is_err());
    let mut a = ix(PAYER, false);
    assert!(compile_message(PAYER, PAYER, &a).is_err());
    a = ix("11111111111111111111111111111111", false);
    a.accounts[0].pubkey = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".into();
    assert!(compile_message(PAYER, PAYER, &a).is_err());
    a.accounts[0].is_signer = false;
    a.data_base64 = STANDARD.encode(vec![0u8; 1300]);
    let msg = compile_message(PAYER, PAYER, &a).unwrap();
    assert!(unsigned_packet(&msg).is_err());
}
