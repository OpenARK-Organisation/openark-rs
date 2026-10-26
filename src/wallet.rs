pub fn create_regtest_bdk_wallet() -> bdk_wallet::Wallet {
    use bdk_wallet::bitcoin::{Network, bip32::Xpriv};
    use bdk_wallet::template::Bip84;
    use bdk_wallet::{KeychainKind, Wallet};
    use std::str::FromStr;

    let xprv = Xpriv::from_str("tprv8ZgxMBicQKsPeZRHk4rTG6orPS2CRNFX3njhUXx5vj9qGog5ZMH4uGReDWN5kCkY3jmWEtWause41CDvBRXD1shKknAMKxT99o9qUTRVC6m")
        .expect("hardcoded test xprv should parse");

    Wallet::create(
        Bip84(xprv.clone(), KeychainKind::External),
        Bip84(xprv, KeychainKind::Internal),
    )
    .network(Network::Regtest)
    .create_wallet_no_persist()
    .expect("BDK wallet descriptor should be valid")
}

pub fn create_regtest_bdk_wallet_from_seed_phrase(seed_phrase: &str) -> bdk_wallet::Wallet {
    use bdk_wallet::bitcoin::{Network, bip32::Xpriv};
    use bdk_wallet::template::Bip84;
    use bdk_wallet::{KeychainKind, Wallet};

    let mnemonic =
        bip39::Mnemonic::parse(seed_phrase).expect("seed phrase should be a valid BIP39 mnemonic");
    let seed = mnemonic.to_seed_normalized("");
    let xprv = Xpriv::new_master(Network::Regtest, &seed)
        .expect("master key derivation from seed phrase should succeed");

    Wallet::create(
        Bip84(xprv.clone(), KeychainKind::External),
        Bip84(xprv, KeychainKind::Internal),
    )
    .network(Network::Regtest)
    .create_wallet_no_persist()
    .expect("BDK wallet descriptor should be valid")
}

pub fn create_regtest_bdk_wallet_with_new_seed_phrase() -> (bdk_wallet::Wallet, String) {
    let mnemonic = bip39::Mnemonic::generate_in(bip39::Language::English, 12)
        .expect("mnemonic generation should succeed");
    let phrase = mnemonic.to_string();
    let wallet = create_regtest_bdk_wallet_from_seed_phrase(&phrase);
    (wallet, phrase)
}

pub fn derive_master_xprv_base58_from_seed_phrase(seed_phrase: &str) -> String {
    use bdk_wallet::bitcoin::Network;
    use bdk_wallet::bitcoin::bip32::Xpriv;

    let mnemonic =
        bip39::Mnemonic::parse(seed_phrase).expect("seed phrase should be a valid BIP39 mnemonic");
    let seed = mnemonic.to_seed_normalized("");
    let xprv = Xpriv::new_master(Network::Regtest, &seed)
        .expect("master key derivation from seed phrase should succeed");
    xprv.to_string()
}

pub fn derive_bip340_xonly_pubkey_from_seed_phrase(seed_phrase: &str) -> String {
    use bdk_wallet::bitcoin::Network;
    use bdk_wallet::bitcoin::bip32::{ChildNumber, DerivationPath, Xpriv};
    use bdk_wallet::bitcoin::secp256k1::{Keypair, Secp256k1, XOnlyPublicKey};

    let mnemonic =
        bip39::Mnemonic::parse(seed_phrase).expect("seed phrase should be a valid BIP39 mnemonic");
    let seed = mnemonic.to_seed_normalized("");
    let master = Xpriv::new_master(Network::Regtest, &seed)
        .expect("master key derivation from seed phrase should succeed");
    let derivation_path = DerivationPath::from(vec![
        ChildNumber::from_hardened_idx(86).expect("hardened child should be valid"),
        ChildNumber::from_hardened_idx(1).expect("hardened child should be valid"),
        ChildNumber::from_hardened_idx(0).expect("hardened child should be valid"),
        ChildNumber::from_normal_idx(0).expect("normal child should be valid"),
        ChildNumber::from_normal_idx(0).expect("normal child should be valid"),
    ]);

    let secp = Secp256k1::new();
    let child = master
        .derive_priv(&secp, &derivation_path)
        .expect("key derivation should succeed");
    let keypair = Keypair::from_secret_key(&secp, &child.private_key);
    let (xonly, _) = XOnlyPublicKey::from_keypair(&keypair);
    xonly.to_string()
}
