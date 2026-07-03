use std::sync::Arc;

use miden_assembly::{Assembler, Linkage};
use miden_core::{Felt, Word, deferred::DeferredState, utils::bytes_to_packed_u32_elements};
use miden_core_lib::{CoreLibrary, dsa::ecdsa_k256_keccak};
use miden_crypto::{SequentialCommit, dsa::ecdsa_k256_keccak::SigningKey};
use miden_processor::{
    DefaultHost, ExecutionError, ExecutionOptions, ExecutionOutput, FastProcessor, StackInputs,
    advice::AdviceInputs,
};
use rand_chacha::{ChaCha20Rng, rand_core::SeedableRng};

use crate::helpers::masm_store_felts;

const VERIFY_EXPECTED_CYCLES: u64 = 2_038;
const PUBKEY_PTR: u32 = 1_000;
const OUT_PTR: u32 = 1_100;

const SECP256K1_GENERATOR_QX_LIMBS_LE: [u32; 8] = [
    0x16f8_1798,
    0x59f2_815b,
    0x2dce_28d9,
    0x029b_fcdb,
    0xce87_0b07,
    0x55a0_6295,
    0xf9dc_bbac,
    0x79be_667e,
];
const SECP256K1_GENERATOR_QY_LIMBS_LE: [u32; 8] = [
    0xfb10_d4b8,
    0x9c47_d08f,
    0xa685_5419,
    0xfd17_b448,
    0x0e11_08a8,
    0x5da4_fbfc,
    0x26a3_c465,
    0x483a_da77,
];
const SECP256K1_GENERATOR_COMPRESSED_SEC1: [u8; 33] = [
    0x02, 0x79, 0xbe, 0x66, 0x7e, 0xf9, 0xdc, 0xbb, 0xac, 0x55, 0xa0, 0x62, 0x95, 0xce, 0x87, 0x0b,
    0x07, 0x02, 0x9b, 0xfc, 0xdb, 0x2d, 0xce, 0x28, 0xd9, 0x59, 0xf2, 0x81, 0x5b, 0x16, 0xf8, 0x17,
    0x98,
];

#[test]
fn core_ecdsa_k256_keccak_verify_accepts_valid_signature() {
    let fixture = valid_fixture();

    let output = run_verify(&fixture).expect("valid core ECDSA K256/Keccak signature must verify");
    assert_deferred_state_round_trips(&output);
}

#[test]
fn core_ecdsa_k256_keccak_verify_cycle_baseline() {
    let fixture = valid_fixture();

    let output = run_core_program_with_advice(&verify_cycle_source(&fixture), &fixture.advice)
        .expect("valid core ECDSA K256/Keccak signature must verify");
    let cycles = output.stack.get_element(0).expect("cycle count").as_canonical_u64();
    assert_eq!(cycles, VERIFY_EXPECTED_CYCLES);
}

#[test]
fn core_ecdsa_k256_keccak_verify_traps_on_wrong_pk_comm() {
    let mut fixture = valid_fixture();
    tamper_felt(&mut fixture.pk_comm[0]);

    run_verify(&fixture).expect_err("wrong public key commitment must trap");
}

#[test]
fn ecdsa_k256_keccak_helpers_use_native_public_key_commitment() {
    let mut rng = ChaCha20Rng::from_seed([0xe5; 32]);
    let sk = SigningKey::with_rng(&mut rng);
    let public_key = sk.public_key();
    let signature = sk.sign(fixed_message());

    let pk_elements = public_key.to_elements();
    let advice = ecdsa_k256_keccak::encode_signature(&public_key, &signature);

    assert_eq!(pk_elements.len(), 16, "public key must encode as QX[8] || QY[8]");
    assert_eq!(&advice[..16], pk_elements.as_slice());
    assert_eq!(
        ecdsa_k256_keccak::public_key_commitment(&public_key),
        public_key.to_commitment(),
    );
}

#[test]
fn core_ecdsa_k256_keccak_compress_public_key_outputs_packed_sec1() {
    let cases = [
        (SECP256K1_GENERATOR_QY_LIMBS_LE[0], SECP256K1_GENERATOR_COMPRESSED_SEC1),
        (SECP256K1_GENERATOR_QY_LIMBS_LE[0] | 1, compressed_sec1_with_prefix(0x03)),
    ];

    for (qy0, expected_sec1) in cases {
        let expected_felts = expected_compressed_public_key_felts(qy0, &expected_sec1);
        let expected_stack = [u64::from(OUT_PTR + 9)];
        let expected_memory = expected_felts.iter().map(Felt::as_canonical_u64).collect::<Vec<_>>();

        build_test!(compress_public_key_source(qy0), &[]).expect_stack_and_memory(
            &expected_stack,
            OUT_PTR,
            &expected_memory,
        );
    }
}

struct Fixture {
    pk_comm: Word,
    message: Word,
    advice: Vec<Felt>,
}

fn valid_fixture() -> Fixture {
    let mut rng = ChaCha20Rng::from_seed([0xe5; 32]);
    let sk = SigningKey::with_rng(&mut rng);
    let message = fixed_message();
    let public_key = sk.public_key();
    let signature = sk.sign(message);

    assert!(
        public_key.verify(message, &signature),
        "Rust fixture signature must verify before passing it to MASM",
    );

    Fixture {
        pk_comm: ecdsa_k256_keccak::public_key_commitment(&public_key),
        message,
        advice: ecdsa_k256_keccak::encode_signature(&public_key, &signature),
    }
}

fn fixed_message() -> Word {
    Word::new([
        Felt::new_unchecked(0x0001_0203_0405_0607),
        Felt::new_unchecked(0x0809_0a0b_0c0d_0e0f),
        Felt::new_unchecked(0x1011_1213_1415_1617),
        Felt::new_unchecked(0x1819_1a1b_1c1d_1e1f),
    ])
}

fn run_verify(fixture: &Fixture) -> Result<ExecutionOutput, ExecutionError> {
    run_core_program_with_advice(&verify_source(fixture), &fixture.advice)
}

fn verify_source(fixture: &Fixture) -> String {
    let setup = verify_setup(fixture);

    format!(
        r#"
        begin
            {setup}
            exec.::miden::core::crypto::dsa::ecdsa_k256_keccak::verify
        end
        "#,
    )
}

fn verify_cycle_source(fixture: &Fixture) -> String {
    let setup = verify_setup(fixture);

    format!(
        r#"
        begin
            {setup}
            clk
            movdn.8
            exec.::miden::core::crypto::dsa::ecdsa_k256_keccak::verify
            clk
            swap sub
            swap drop
        end
        "#,
    )
}

fn compress_public_key_source(qy0: u32) -> String {
    let mut pk = native_generator_public_key();
    pk[8] = Felt::from_u32(qy0);
    let setup = masm_store_felts(&pk, PUBKEY_PTR);

    format!(
        r#"
        begin
            {setup}
            push.{OUT_PTR}
            push.{PUBKEY_PTR}
            exec.::miden::core::crypto::dsa::ecdsa_k256_keccak::compress_public_key
            swap drop
        end
        "#,
    )
}

fn expected_compressed_public_key_felts(qy0: u32, expected_sec1: &[u8; 33]) -> Vec<Felt> {
    assert_eq!(qy0 & 1, u32::from(expected_sec1[0] - 2));
    bytes_to_packed_u32_elements(expected_sec1)
}

fn native_generator_public_key() -> Vec<Felt> {
    SECP256K1_GENERATOR_QX_LIMBS_LE
        .into_iter()
        .chain(SECP256K1_GENERATOR_QY_LIMBS_LE)
        .map(Felt::from_u32)
        .collect()
}

fn compressed_sec1_with_prefix(prefix: u8) -> [u8; 33] {
    let mut compressed = SECP256K1_GENERATOR_COMPRESSED_SEC1;
    compressed[0] = prefix;
    compressed
}

fn verify_setup(fixture: &Fixture) -> String {
    let message = masm_push_word(&fixture.message);
    let pk_comm = masm_push_word(&fixture.pk_comm);

    format!(
        r#"
        {message}
        {pk_comm}
        "#,
    )
}

fn masm_push_word(word: &Word) -> String {
    let felts = word
        .iter()
        .rev()
        .map(|felt| felt.as_canonical_u64().to_string())
        .collect::<Vec<_>>()
        .join(".");
    format!("push.{felts}")
}

fn run_core_program_with_advice(
    source: &str,
    advice: &[Felt],
) -> Result<ExecutionOutput, ExecutionError> {
    let core_lib = CoreLibrary::default();
    let program = Assembler::default()
        .with_package(core_lib.package(), Linkage::Dynamic)
        .expect("failed to link core library")
        .assemble_program("core_ecdsa_k256_keccak_test", source)
        .expect("failed to assemble core ECDSA test program")
        .unwrap_program();

    let mut host = DefaultHost::default()
        .with_library(&core_lib)
        .expect("failed to load CoreLibrary into the host");

    let processor = FastProcessor::new_with_options(
        StackInputs::default(),
        AdviceInputs::default().with_stack(advice.iter().copied()),
        ExecutionOptions::default(),
    )
    .expect("processor construction")
    .with_precompile_registry(miden_precompiles::registry())?;

    let output = processor.execute_sync(&program, &mut host);
    if let Ok(output) = &output {
        assert!(output.advice.stack().is_empty(), "core ECDSA wrapper must consume advice");
    }

    output
}

fn assert_deferred_state_round_trips(output: &ExecutionOutput) {
    let registry = Arc::new(miden_precompiles::registry());
    let wire = output.deferred_state.to_wire().expect("deferred state must encode to wire");
    let rehydrated = DeferredState::from_wire(Arc::clone(&registry), &wire, usize::MAX)
        .expect("deferred wire must rehydrate under miden-precompiles registry");
    assert_eq!(
        rehydrated.root(),
        output.deferred_state.root(),
        "wire round-trip must preserve the deferred root",
    );
}

fn tamper_felt(felt: &mut Felt) {
    let value = felt.as_canonical_u64();
    *felt = if value == 0 {
        Felt::from_u32(1)
    } else {
        Felt::new(value - 1).expect("decremented canonical field element must stay canonical")
    };
}
