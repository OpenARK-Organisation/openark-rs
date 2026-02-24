use std::str::FromStr;

pub fn create_regtest_bdk_wallet() -> bdk_wallet::Wallet {
    use bdk_wallet::bitcoin::{Network, bip32::Xpriv};
    use bdk_wallet::template::Bip84;
    use bdk_wallet::{KeychainKind, Wallet};

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
