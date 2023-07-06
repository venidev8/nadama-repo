use std::collections::{BTreeMap, BTreeSet};

use borsh::BorshSerialize;
use criterion::{criterion_group, criterion_main, Criterion};
use namada::core::types::address::{self, Address};
use namada::core::types::key::{
    common, SecretKey as SecretKeyInterface, SigScheme,
};
use namada::core::types::token::{Amount, Transfer};
use namada::ledger::gas::VpGasMeter;
use namada::ledger::storage::Sha256Hasher;
use namada::proto::{Code, Signature, Tx};
use namada::types::chain::ChainId;
use namada::types::governance::{ProposalVote, VoteType};
use namada::types::hash::Hash;
use namada::types::key::ed25519;
use namada::types::masp::{TransferSource, TransferTarget};
use namada::types::storage::{Key, TxIndex};
use namada::types::transaction::governance::VoteProposalData;
use namada::types::transaction::pos::{Bond, CommissionChange};
use namada::types::transaction::UpdateVp;
use namada::vm::wasm::run;
use namada_apps::wallet::defaults;
use namada_apps::wasm_loader;
use namada_benches::{
    generate_foreign_key_tx, generate_tx, BenchShell, BenchShieldedCtx,
    ALBERT_PAYMENT_ADDRESS, ALBERT_SPENDING_KEY, BERTHA_PAYMENT_ADDRESS,
    TX_BOND_WASM, TX_CHANGE_VALIDATOR_COMMISSION_WASM, TX_REVEAL_PK_WASM,
    TX_TRANSFER_WASM, TX_UNBOND_WASM, TX_UPDATE_VP_WASM, TX_VOTE_PROPOSAL_WASM,
    VP_VALIDATOR_WASM, WASM_DIR,
};
use rust_decimal::Decimal;
use sha2::Digest;

const VP_USER_WASM: &str = "vp_user.wasm";
const VP_TOKEN_WASM: &str = "vp_token.wasm";
const VP_IMPLICIT_WASM: &str = "vp_implicit.wasm";
const VP_MASP_WASM: &str = "vp_masp.wasm";

fn vp_user(c: &mut Criterion) {
    let mut group = c.benchmark_group("vp_user");

    let foreign_key_write =
        generate_foreign_key_tx(&defaults::albert_keypair());

    let transfer = generate_tx(
        TX_TRANSFER_WASM,
        Transfer {
            source: defaults::albert_address(),
            target: defaults::bertha_address(),
            token: address::nam(),
            sub_prefix: None,
            amount: Amount::whole(1000),
            key: None,
            shielded: None,
        },
        None,
        &defaults::albert_keypair(),
    );

    let received_transfer = generate_tx(
        TX_TRANSFER_WASM,
        Transfer {
            source: defaults::bertha_address(),
            target: defaults::albert_address(),
            token: address::nam(),
            sub_prefix: None,
            amount: Amount::whole(1000),
            key: None,
            shielded: None,
        },
        None,
        &defaults::bertha_keypair(),
    );

    let shell = BenchShell::default();
    let vp_validator_hash = shell
        .read_storage_key(&Key::wasm_hash(VP_VALIDATOR_WASM))
        .unwrap();
    //FIXME: shared function?
    let mut vp = Tx::new(namada::types::transaction::TxType::Decrypted(
        namada::types::transaction::DecryptedTx::Decrypted {
            #[cfg(not(feature = "mainnet"))]
            has_valid_pow: true,
        },
    ));
    let extra_section = vp.add_section(namada::proto::Section::ExtraData(
        Code::from_hash(vp_validator_hash),
    ));
    let data = UpdateVp {
        addr: defaults::albert_address(),
        vp_code_hash: Hash(
            extra_section
                .hash(&mut sha2::Sha256::new())
                .finalize_reset()
                .into(),
        ),
    };
    vp.set_data(namada::proto::Data::new(data.try_to_vec().unwrap()));
    vp.set_code(Code::new(wasm_loader::read_wasm_or_exit(
        WASM_DIR,
        TX_UPDATE_VP_WASM,
    )));
    vp.add_section(namada::proto::Section::Signature(Signature::new(
        vp.data_sechash(),
        &defaults::albert_keypair(),
    )));

    let vote = generate_tx(
        TX_VOTE_PROPOSAL_WASM,
        VoteProposalData {
            id: 0,
            vote: ProposalVote::Yay(VoteType::Default),
            voter: defaults::albert_address(),
            delegations: vec![defaults::validator_address()],
        },
        None,
        &defaults::albert_keypair(),
    );

    let pos = generate_tx(
        TX_UNBOND_WASM,
        Bond {
            validator: defaults::validator_address(),
            amount: Amount::whole(1000),
            source: Some(defaults::albert_address()),
        },
        None,
        &defaults::albert_keypair(),
    );

    for (signed_tx, bench_name) in [
        foreign_key_write,
        transfer,
        received_transfer,
        vote,
        pos,
        vp,
    ]
    .iter()
    .zip([
        "foreign_key_write",
        "transfer",
        "received_transfer",
        "governance_vote",
        "pos",
        "vp",
    ]) {
        let mut shell = BenchShell::default();
        let vp_code_hash: Hash = shell
            .read_storage_key(&Key::wasm_hash(VP_USER_WASM))
            .unwrap();
        shell.execute_tx(signed_tx);
        let (verifiers, keys_changed) = shell
            .wl_storage
            .write_log
            .verifiers_and_changed_keys(&BTreeSet::default());

        group.bench_function(bench_name, |b| {
            b.iter(|| {
                assert!(run::vp(
                    &vp_code_hash,
                    signed_tx,
                    &TxIndex(0),
                    &defaults::albert_address(),
                    &shell.wl_storage.storage,
                    &shell.wl_storage.write_log,
                    &mut VpGasMeter::new(u64::MAX, 0),
                    &BTreeMap::default(),
                    &keys_changed,
                    &verifiers,
                    shell.vp_wasm_cache.clone(),
                    #[cfg(not(feature = "mainnet"))]
                    false,
                )
                .unwrap());
            })
        });
    }

    group.finish();
}

fn vp_implicit(c: &mut Criterion) {
    let mut group = c.benchmark_group("vp_implicit");

    let mut csprng = rand::rngs::OsRng {};
    let implicit_account: common::SecretKey =
        ed25519::SigScheme::generate(&mut csprng)
            .try_to_sk()
            .unwrap();

    let foreign_key_write =
        generate_foreign_key_tx(&defaults::albert_keypair());

    let transfer = generate_tx(
        TX_TRANSFER_WASM,
        Transfer {
            source: Address::from(&implicit_account.to_public()),
            target: defaults::bertha_address(),
            token: address::nam(),
            sub_prefix: None,
            amount: Amount::whole(500),
            key: None,
            shielded: None,
        },
        None,
        &implicit_account,
    );

    let received_transfer = generate_tx(
        TX_TRANSFER_WASM,
        Transfer {
            source: defaults::bertha_address(),
            target: Address::from(&implicit_account.to_public()),
            token: address::nam(),
            sub_prefix: None,
            amount: Amount::whole(1000),
            key: None,
            shielded: None,
        },
        None,
        &defaults::bertha_keypair(),
    );

    let mut reveal_pk = Tx::new(namada::types::transaction::TxType::Decrypted(
        namada::types::transaction::DecryptedTx::Decrypted {
            #[cfg(not(feature = "mainnet"))]
            has_valid_pow: true,
        },
    ));
    reveal_pk.set_code(namada::proto::Code::new(
        wasm_loader::read_wasm_or_exit(WASM_DIR, TX_REVEAL_PK_WASM),
    ));
    reveal_pk.set_data(namada::proto::Data::new(
        implicit_account.to_public().try_to_vec().unwrap(),
    ));

    let pos = generate_tx(
        TX_BOND_WASM,
        Bond {
            validator: defaults::validator_address(),
            amount: Amount::whole(1000),
            source: Some(Address::from(&implicit_account.to_public())),
        },
        None,
        &implicit_account,
    );

    let vote = generate_tx(
        TX_VOTE_PROPOSAL_WASM,
        VoteProposalData {
            id: 0,
            vote: ProposalVote::Yay(VoteType::Default),
            voter: Address::from(&implicit_account.to_public()),
            delegations: vec![], /* NOTE: no need to bond tokens because the
                                  * implicit vp doesn't check that */
        },
        None,
        &implicit_account,
    );

    for (tx, bench_name) in [
        &foreign_key_write,
        &reveal_pk,
        &transfer,
        &received_transfer,
        &pos,
        &vote,
    ]
    .into_iter()
    .zip([
        "foreign_key_write",
        "reveal_pk",
        "transfer",
        "received_transfer",
        "pos",
        "governance_vote",
    ]) {
        let mut shell = BenchShell::default();
        let vp_code_hash: Hash = shell
            .read_storage_key(&Key::wasm_hash(VP_IMPLICIT_WASM))
            .unwrap();

        if bench_name != "reveal_pk" {
            // Reveal publick key
            shell.execute_tx(&reveal_pk);
            shell.wl_storage.commit_tx();
            shell.commit();
        }

        if bench_name == "transfer" {
            // Transfer some tokens to the implicit address
            shell.execute_tx(&received_transfer);
            shell.wl_storage.commit_tx();
            shell.commit();
        }

        // Run the tx to validate
        shell.execute_tx(tx);
        let (verifiers, keys_changed) = shell
            .wl_storage
            .write_log
            .verifiers_and_changed_keys(&BTreeSet::default());

        group.bench_function(bench_name, |b| {
            b.iter(|| {
                assert!(run::vp(
                    &vp_code_hash,
                    tx,
                    &TxIndex(0),
                    &Address::from(&implicit_account.to_public()),
                    &shell.wl_storage.storage,
                    &shell.wl_storage.write_log,
                    &mut VpGasMeter::new(u64::MAX, 0),
                    &BTreeMap::default(),
                    &keys_changed,
                    &verifiers,
                    shell.vp_wasm_cache.clone(),
                    #[cfg(not(feature = "mainnet"))]
                    false,
                )
                .unwrap())
            })
        });
    }

    group.finish();
}

fn vp_validator(c: &mut Criterion) {
    let shell = BenchShell::default();
    let vp_code_hash: Hash = shell
        .read_storage_key(&Key::wasm_hash(VP_VALIDATOR_WASM))
        .unwrap();
    let mut group = c.benchmark_group("vp_validator");

    let foreign_key_write =
        generate_foreign_key_tx(&defaults::albert_keypair());

    let transfer = generate_tx(
        TX_TRANSFER_WASM,
        Transfer {
            source: defaults::validator_address(),
            target: defaults::bertha_address(),
            token: address::nam(),
            sub_prefix: None,
            amount: Amount::whole(1000),
            key: None,
            shielded: None,
        },
        None,
        &defaults::validator_keypair(),
    );

    let received_transfer = generate_tx(
        TX_TRANSFER_WASM,
        Transfer {
            source: defaults::bertha_address(),
            target: defaults::validator_address(),
            token: address::nam(),
            sub_prefix: None,
            amount: Amount::whole(1000),
            key: None,
            shielded: None,
        },
        None,
        &defaults::bertha_keypair(),
    );

    //FIXME: shared function?
    let mut vp = Tx::new(namada::types::transaction::TxType::Decrypted(
        namada::types::transaction::DecryptedTx::Decrypted {
            #[cfg(not(feature = "mainnet"))]
            has_valid_pow: true,
        },
    ));
    let extra_section = vp.add_section(namada::proto::Section::ExtraData(
        Code::from_hash(vp_code_hash),
    ));
    let data = UpdateVp {
        addr: defaults::validator_address(),
        vp_code_hash: Hash(
            extra_section
                .hash(&mut sha2::Sha256::new())
                .finalize_reset()
                .into(),
        ),
    };
    vp.set_data(namada::proto::Data::new(data.try_to_vec().unwrap()));
    vp.set_code(Code::new(wasm_loader::read_wasm_or_exit(
        WASM_DIR,
        TX_UPDATE_VP_WASM,
    )));
    vp.add_section(namada::proto::Section::Signature(Signature::new(
        vp.data_sechash(),
        &defaults::validator_keypair(),
    )));

    let commission_rate = generate_tx(
        TX_CHANGE_VALIDATOR_COMMISSION_WASM,
        CommissionChange {
            validator: defaults::validator_address(),
            new_rate: Decimal::new(6, 2),
        },
        None,
        &defaults::validator_keypair(),
    );

    let vote = generate_tx(
        TX_VOTE_PROPOSAL_WASM,
        VoteProposalData {
            id: 0,
            vote: ProposalVote::Yay(VoteType::Default),
            voter: defaults::validator_address(),
            delegations: vec![],
        },
        None,
        &defaults::validator_keypair(),
    );

    let pos = generate_tx(
        TX_UNBOND_WASM,
        Bond {
            validator: defaults::validator_address(),
            amount: Amount::whole(1000),
            source: None,
        },
        None,
        &defaults::validator_keypair(),
    );

    for (signed_tx, bench_name) in [
        foreign_key_write,
        transfer,
        received_transfer,
        vote,
        pos,
        commission_rate,
        vp,
    ]
    .iter()
    .zip([
        "foreign_key_write",
        "transfer",
        "received_transfer",
        "governance_vote",
        "pos",
        "commission_rate",
        "vp",
    ]) {
        let mut shell = BenchShell::default();

        shell.execute_tx(signed_tx);
        let (verifiers, keys_changed) = shell
            .wl_storage
            .write_log
            .verifiers_and_changed_keys(&BTreeSet::default());

        group.bench_function(bench_name, |b| {
            b.iter(|| {
                assert!(run::vp(
                    &vp_code_hash,
                    signed_tx,
                    &TxIndex(0),
                    &defaults::validator_address(),
                    &shell.wl_storage.storage,
                    &shell.wl_storage.write_log,
                    &mut VpGasMeter::new(u64::MAX, 0),
                    &BTreeMap::default(),
                    &keys_changed,
                    &verifiers,
                    shell.vp_wasm_cache.clone(),
                    #[cfg(not(feature = "mainnet"))]
                    false,
                )
                .unwrap());
            })
        });
    }

    group.finish();
}

fn vp_token(c: &mut Criterion) {
    let mut group = c.benchmark_group("vp_token");

    let foreign_key_write =
        generate_foreign_key_tx(&defaults::albert_keypair());

    let transfer = generate_tx(
        TX_TRANSFER_WASM,
        Transfer {
            source: defaults::albert_address(),
            target: defaults::bertha_address(),
            token: address::nam(),
            sub_prefix: None,
            amount: Amount::whole(1000),
            key: None,
            shielded: None,
        },
        None,
        &defaults::albert_keypair(),
    );

    for (signed_tx, bench_name) in [foreign_key_write, transfer]
        .iter()
        .zip(["foreign_key_write", "transfer"])
    {
        let mut shell = BenchShell::default();
        let vp_code_hash: Hash = shell
            .read_storage_key(&Key::wasm_hash(VP_TOKEN_WASM))
            .unwrap();
        shell.execute_tx(signed_tx);
        let (verifiers, keys_changed) = shell
            .wl_storage
            .write_log
            .verifiers_and_changed_keys(&BTreeSet::default());

        group.bench_function(bench_name, |b| {
            b.iter(|| {
                assert!(run::vp(
                    &vp_code_hash,
                    signed_tx,
                    &TxIndex(0),
                    &defaults::albert_address(),
                    &shell.wl_storage.storage,
                    &shell.wl_storage.write_log,
                    &mut VpGasMeter::new(u64::MAX, 0),
                    &BTreeMap::default(),
                    &keys_changed,
                    &verifiers,
                    shell.vp_wasm_cache.clone(),
                    #[cfg(not(feature = "mainnet"))]
                    false,
                )
                .unwrap());
            })
        });
    }
}

fn vp_masp(c: &mut Criterion) {
    let mut group = c.benchmark_group("vp_masp");

    let amount = Amount::whole(500);

    for bench_name in ["shielding", "unshielding", "shielded"] {
        group.bench_function(bench_name, |b| {
            let mut shielded_ctx = BenchShieldedCtx::default();
            let vp_code_hash: Hash = shielded_ctx
                .shell
                .read_storage_key(&Key::wasm_hash(VP_MASP_WASM))
                .unwrap();

            let albert_spending_key = shielded_ctx
                .wallet
                .find_spending_key(ALBERT_SPENDING_KEY, None)
                .unwrap()
                .to_owned();
            let albert_payment_addr = shielded_ctx
                .wallet
                .find_payment_addr(ALBERT_PAYMENT_ADDRESS)
                .unwrap()
                .to_owned();
            let bertha_payment_addr = shielded_ctx
                .wallet
                .find_payment_addr(BERTHA_PAYMENT_ADDRESS)
                .unwrap()
                .to_owned();

            // Shield some tokens for Albert
            let shield_tx = shielded_ctx.generate_masp_tx(
                amount,
                TransferSource::Address(defaults::albert_address()),
                TransferTarget::PaymentAddress(albert_payment_addr),
            );
            shielded_ctx.shell.execute_tx(&shield_tx);
            shielded_ctx.shell.wl_storage.commit_tx();
            shielded_ctx.shell.commit();

            let signed_tx = match bench_name {
                "shielding" => shielded_ctx.generate_masp_tx(
                    amount,
                    TransferSource::Address(defaults::albert_address()),
                    TransferTarget::PaymentAddress(albert_payment_addr),
                ),
                "unshielding" => shielded_ctx.generate_masp_tx(
                    amount,
                    TransferSource::ExtendedSpendingKey(albert_spending_key),
                    TransferTarget::Address(defaults::albert_address()),
                ),
                "shielded" => shielded_ctx.generate_masp_tx(
                    amount,
                    TransferSource::ExtendedSpendingKey(albert_spending_key),
                    TransferTarget::PaymentAddress(bertha_payment_addr),
                ),
                _ => panic!("Unexpected bench test"),
            };
            shielded_ctx.shell.execute_tx(&signed_tx);
            let (verifiers, keys_changed) = shielded_ctx
                .shell
                .wl_storage
                .write_log
                .verifiers_and_changed_keys(&BTreeSet::default());

            b.iter(|| {
                assert!(run::vp(
                    &vp_code_hash,
                    &signed_tx,
                    &TxIndex(0),
                    &defaults::validator_address(),
                    &shielded_ctx.shell.wl_storage.storage,
                    &shielded_ctx.shell.wl_storage.write_log,
                    &mut VpGasMeter::new(u64::MAX, 0),
                    &BTreeMap::default(),
                    &keys_changed,
                    &verifiers,
                    shielded_ctx.shell.vp_wasm_cache.clone(),
                    #[cfg(not(feature = "mainnet"))]
                    false,
                )
                .unwrap());
            })
        });
    }

    group.finish();
}

criterion_group!(
    whitelisted_vps,
    vp_user,
    vp_implicit,
    vp_validator,
    vp_masp,
    vp_token
);
criterion_main!(whitelisted_vps);
